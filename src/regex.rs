//! kemacs の MAGIC 正規表現 (`search.C_` の mcstr/amatch/mceq) に相当する
//! 軽量パターンマッチャー。`, ., [], ^, $, *, \` をサポートし、
//! 既定は C 版 ctab[] 由来の全角/かな折り畳み (MDEXACT のときは完全一致)。
//! `+ ? | ()` は C 版に存在しないためメタ文字として扱わない。

/// パターンを折り畳む際、折り畳みで生じたメタ文字はエスケープしてリテラルに保つ。
fn canonical_pattern(pat: &[u8]) -> Vec<u8> {
    use crate::casefold::fold_char;
    let text = String::from_utf8_lossy(pat);
    let mut chars = text.chars().peekable();
    let mut out = Vec::new();
    fn push(out: &mut Vec<u8>, c: char) {
        let mut buf = [0u8; 4];
        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(b'\\');
            if let Some(&next) = chars.peek() {
                chars.next();
                push(&mut out, fold_char(next));
            }
        } else if matches!(c, '.' | '^' | '$' | '*' | '[' | ']' | '-') {
            push(&mut out, c);
        } else {
            let f = fold_char(c);
            if matches!(f, '.' | '^' | '$' | '*' | '[' | ']' | '\\') {
                out.push(b'\\');
            }
            push(&mut out, f);
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    Lit(u8),
    Any,
    Class { ranges: Vec<(u8, u8)>, neg: bool },
    Bol,
    Eol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Elem {
    atom: Atom,
    closure: bool,
}

#[derive(Clone, Debug)]
pub struct Regex {
    elems: Vec<Elem>,
    pub magical: bool,
    pat: Vec<u8>,
}

fn eq(a: u8, b: u8, exact: bool) -> bool {
    if exact {
        a == b
    } else {
        a.eq_ignore_ascii_case(&b)
    }
}

fn atom_matches(atom: &Atom, c: u8, exact: bool) -> bool {
    match atom {
        Atom::Lit(x) => eq(*x, c, exact),
        Atom::Any => c != b'\n',
        Atom::Class { ranges, neg } => {
            let mut found = false;
            for &(lo, hi) in ranges {
                if exact {
                    if c >= lo && c <= hi {
                        found = true;
                        break;
                    }
                } else {
                    let l = lo.to_ascii_lowercase();
                    let h = hi.to_ascii_uppercase();
                    let cl = c.to_ascii_lowercase();
                    if cl >= l && cl <= h.max(l) {
                        found = true;
                        break;
                    }
                }
            }
            if *neg {
                !found
            } else {
                found
            }
        }
        Atom::Bol | Atom::Eol => false,
    }
}

fn closure_ok(atom: &Atom) -> bool {
    !matches!(atom, Atom::Bol | Atom::Eol)
}

impl Regex {
    /// パターンをコンパイルする。不正なクラスや過度に長いパターンは ERROR。
    pub fn compile(pat: &[u8]) -> Result<Self, String> {
        // match_here() は要素数だけ再帰するため、上限を設けてスタック枯渇を防ぐ。
        if pat.len() > 1024 {
            return Err("pattern too long".to_string());
        }
        let mut elems: Vec<Elem> = Vec::new();
        let mut magical = false;
        let mut i = 0usize;
        while i < pat.len() {
            let c = pat[i];
            let atom = match c {
                b'\\' => {
                    if i + 1 < pat.len() {
                        i += 1;
                        magical = true;
                        Atom::Lit(pat[i])
                    } else {
                        Atom::Lit(b'\\')
                    }
                }
                b'.' => {
                    magical = true;
                    i += 0;
                    Atom::Any
                }
                b'^' if i == 0 => {
                    magical = true;
                    Atom::Bol
                }
                b'$' if i + 1 == pat.len() => {
                    magical = true;
                    Atom::Eol
                }
                b'[' => {
                    magical = true;
                    let mut j = i + 1;
                    let mut neg = false;
                    if j < pat.len() && pat[j] == b'^' {
                        neg = true;
                        j += 1;
                    }
                    let mut ranges = Vec::new();
                    let mut closed = false;
                    while j < pat.len() {
                        if pat[j] == b']' && !ranges.is_empty() {
                            closed = true;
                            break;
                        }
                        let lo = if pat[j] == b'\\' && j + 1 < pat.len() {
                            j += 1;
                            pat[j]
                        } else {
                            pat[j]
                        };
                        j += 1;
                        if j + 1 < pat.len() && pat[j] == b'-' && pat[j + 1] != b']' {
                            j += 1;
                            let hi = if pat[j] == b'\\' && j + 1 < pat.len() {
                                j += 1;
                                pat[j]
                            } else {
                                pat[j]
                            };
                            j += 1;
                            ranges.push((lo, hi));
                        } else {
                            ranges.push((lo, lo));
                        }
                    }
                    if !closed {
                        return Err("Unmatched '['".to_string());
                    }
                    i = j;
                    Atom::Class { ranges, neg }
                }
                _ => Atom::Lit(c),
            };
            i += 1;
            if c == b'*'
                && !elems.is_empty()
                && !elems.last().unwrap().closure
                && closure_ok(&elems.last().unwrap().atom)
            {
                elems.last_mut().unwrap().closure = true;
                magical = true;
                continue;
            }
            elems.push(Elem {
                atom,
                closure: false,
            });
        }
        Ok(Regex {
            elems,
            magical,
            pat: pat.to_vec(),
        })
    }

    /// パターンに C 版の `magical` として扱うメタ文字があるか。
    pub fn is_magical(pat: &[u8]) -> bool {
        Self::compile(pat).map(|r| r.magical).unwrap_or(false)
    }

    fn match_here(&self, idx: usize, text: &[u8], pos: usize, exact: bool) -> Option<usize> {
        if idx == self.elems.len() {
            return Some(pos);
        }
        let e = &self.elems[idx];
        match &e.atom {
            Atom::Bol => {
                if pos == 0 {
                    self.match_here(idx + 1, text, pos, exact)
                } else {
                    None
                }
            }
            Atom::Eol => {
                if pos == text.len() {
                    self.match_here(idx + 1, text, pos, exact)
                } else {
                    None
                }
            }
            atom => {
                if e.closure {
                    let mut p = pos;
                    let mut count = 0usize;
                    while p < text.len() && atom_matches(atom, text[p], exact) {
                        p += 1;
                        count += 1;
                    }
                    for k in (0..=count).rev() {
                        if let Some(r) = self.match_here(idx + 1, text, pos + k, exact) {
                            return Some(r);
                        }
                    }
                    None
                } else if pos < text.len() && atom_matches(atom, text[pos], exact) {
                    self.match_here(idx + 1, text, pos + 1, exact)
                } else {
                    None
                }
            }
        }
    }

    /// 指定行の offset 以降で最初に一致する (開始, 終了) を返す。
    fn match_raw(&self, line: &[u8], off: usize, exact: bool) -> Option<(usize, usize)> {
        for start in off..=line.len() {
            if let Some(end) = self.match_here(0, line, start, exact) {
                // 閉包だけの空一致は実用上無意味なので次の位置へ進む
                if end != start || self.elems.is_empty() {
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// 指定行の offset 以前で最後に一致する (開始, 終了) を返す。
    fn match_raw_before(&self, line: &[u8], off: usize, exact: bool) -> Option<(usize, usize)> {
        for start in (0..=off.min(line.len())).rev() {
            if let Some(end) = self.match_here(0, line, start, exact) {
                if end != start || self.elems.is_empty() {
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// 指定行の offset 以降で最初に一致する (開始, 終了) を返す。
    pub fn match_from(&self, line: &[u8], off: usize, exact: bool) -> Option<(usize, usize)> {
        if exact {
            return self.match_raw(line, off, true);
        }
        use crate::casefold::{canonical_index, canonicalize};
        let cp = canonical_pattern(&self.pat);
        let re = Regex::compile(&cp).ok()?;
        let (cl, map) = canonicalize(line);
        let start = canonical_index(&map, off);
        re.match_raw(&cl, start, false)
            .map(|(a, b)| (map[a], if b < map.len() { map[b] } else { line.len() }))
    }

    /// 指定行の offset 以前で最後に一致する (開始, 終了) を返す。
    pub fn match_before(&self, line: &[u8], off: usize, exact: bool) -> Option<(usize, usize)> {
        if exact {
            return self.match_raw_before(line, off, true);
        }
        use crate::casefold::{canonical_index, canonicalize};
        let cp = canonical_pattern(&self.pat);
        let re = Regex::compile(&cp).ok()?;
        let (cl, map) = canonicalize(line);
        let end = canonical_index(&map, off);
        re.match_raw_before(&cl, end, false)
            .map(|(a, b)| (map[a], if b < map.len() { map[b] } else { line.len() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pat: &str, text: &str) -> Option<(usize, usize)> {
        Regex::compile(pat.as_bytes())
            .unwrap()
            .match_from(text.as_bytes(), 0, true)
    }

    #[test]
    fn literal_and_any() {
        assert_eq!(m("abc", "xxabcxx"), Some((2, 5)));
        assert_eq!(m("a.c", "xxabcxx"), Some((2, 5)));
        assert!(Regex::is_magical(b"a.c"));
        assert!(!Regex::is_magical(b"abc"));
    }

    #[test]
    fn character_classes() {
        assert_eq!(m("[a-c]*xy", "zzabccxy!"), Some((2, 8)));
        assert_eq!(m("[^0-9][0-9]", "a1"), Some((0, 2)));
        assert_eq!(m(r"\.*", "a.*b"), Some((1, 2)));
    }

    #[test]
    fn anchors_and_closure() {
        assert_eq!(m("^foo", "foobar"), Some((0, 3)));
        assert_eq!(
            Regex::compile(b"^foo")
                .unwrap()
                .match_from(b"xfoo", 0, true),
            None
        );
        assert_eq!(m("bar$", "foobar"), Some((3, 6)));
        assert_eq!(m("ab*c", "abbbc"), Some((0, 5)));
        assert_eq!(m("ab*c", "ac"), Some((0, 2)));
    }

    #[test]
    fn fullwidth_casefold() {
        let re = Regex::compile("。".as_bytes()).unwrap();
        assert_eq!(re.match_from("abc.def".as_bytes(), 0, false), Some((3, 4)));
        let re2 = Regex::compile("ＡＢＣ".as_bytes()).unwrap();
        assert_eq!(re2.match_from("xabcY".as_bytes(), 0, false), Some((1, 4)));
    }

    #[test]
    fn non_c_metacharacters_are_literals() {
        for pat in ["a+b", "a?b", "a|b", "(a)", "a)b"] {
            let re = Regex::compile(pat.as_bytes()).unwrap();
            assert!(!re.magical, "{} must not be magical", pat);
            assert_eq!(re.match_from(pat.as_bytes(), 0, true), Some((0, pat.len())));
        }
    }

    #[test]
    fn case_insensitive_by_default() {
        let re = Regex::compile(b"[a-z]*").unwrap();
        let hit = re.match_from(b"ABC", 0, false);
        assert!(hit.is_some());
        assert_eq!(
            Regex::compile(b"[a-z]*")
                .unwrap()
                .match_from(b"ABC", 0, true),
            None
        );
    }
}
