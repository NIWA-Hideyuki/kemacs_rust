//! 文字列検索。C 版 `search.C_` のリテラル検索と MAGIC 正規表現に対応。

use crate::buffer::Pos;
use crate::casefold::{canonical_index, canonicalize};
use crate::regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Reverse,
}

/// start から pat を探し、(一致開始位置, 一致終了位置) を返す。
pub fn find(
    lines: &[Vec<u8>],
    start: Pos,
    pat: &[u8],
    dir: Direction,
    exact: bool,
) -> Option<(Pos, Pos)> {
    if pat.is_empty() {
        return None;
    }
    find_mode(lines, start, pat, dir, exact, false)
}

/// MAGIC モードのときだけメタ文字を解釈する (C 版 readpattern() と同じ)。
pub fn find_mode(
    lines: &[Vec<u8>],
    start: Pos,
    pat: &[u8],
    dir: Direction,
    exact: bool,
    magic: bool,
) -> Option<(Pos, Pos)> {
    if lines.is_empty() {
        return None;
    }
    if magic && Regex::is_magical(pat) {
        return find_regex(lines, start, pat, dir, exact);
    }
    if canonicalize(pat).0.is_empty() {
        return None;
    }
    let last = lines.len().saturating_sub(1);
    let order: Vec<usize> = match dir {
        Direction::Forward => (start.line.min(last)..=last).collect(),
        Direction::Reverse => (0..=start.line.min(last)).rev().collect(),
    };
    for li in order {
        let line = &lines[li];
        let (cp, cl, cmap) = if exact {
            (
                pat.to_vec(),
                line.clone(),
                (0..=line.len()).collect::<Vec<_>>(),
            )
        } else {
            let (cp, _) = canonicalize(pat);
            let (cl, cmap) = canonicalize(line);
            (cp, cl, cmap)
        };
        if cl.len() < cp.len() {
            continue;
        }
        let bound = if li == start.line {
            start.off.min(line.len())
        } else if dir == Direction::Forward {
            0
        } else {
            line.len()
        };
        let cbound = canonical_index(&cmap, bound);
        let range: Box<dyn Iterator<Item = usize>> = if dir == Direction::Forward {
            Box::new(cbound..=cl.len() - cp.len())
        } else {
            // 後方検索は「開始位置で始まる一致」を再び返さない。
            // ただし開始位置で終わる一致は C 版と同じく対象に含める。
            let max_start = cbound.saturating_sub(cp.len()).min(cl.len() - cp.len());
            Box::new((0..=max_start).rev())
        };
        for ci in range {
            if cl[ci..ci + cp.len()].iter().zip(&cp).all(|(a, b)| a == b) {
                let a = cmap[ci];
                let b = if ci + cp.len() < cmap.len() {
                    cmap[ci + cp.len()]
                } else {
                    line.len()
                };
                return Some((Pos::new(li, a), Pos::new(li, b)));
            }
        }
    }
    None
}

/// メタ文字を含むパターンを Regex で検索する (C 版 mcscanner/amatch 相当)。
fn find_regex(
    lines: &[Vec<u8>],
    start: Pos,
    pat: &[u8],
    dir: Direction,
    exact: bool,
) -> Option<(Pos, Pos)> {
    let re = Regex::compile(pat).ok()?;
    match dir {
        Direction::Forward => {
            for (li, line) in lines.iter().enumerate().skip(start.line) {
                let min = if li == start.line { start.off } else { 0 };
                if let Some((a, b)) = re.match_from(line, min, exact) {
                    return Some((Pos::new(li, a), Pos::new(li, b)));
                }
            }
            None
        }
        Direction::Reverse => {
            let mut li = start.line.min(lines.len().saturating_sub(1));
            loop {
                let line = &lines[li];
                let max = if li == start.line {
                    start.off
                } else {
                    line.len()
                };
                if let Some((a, b)) = re.match_before(line, max, exact) {
                    return Some((Pos::new(li, a), Pos::new(li, b)));
                }
                if li == 0 {
                    break;
                }
                li -= 1;
            }
            None
        }
    }
}

#[cfg(test)]
mod regex_tests {
    use super::*;

    #[test]
    fn fullwidth_casefold_search() {
        let lines = vec!["abc。DEF".as_bytes().to_vec(), Vec::new()];
        assert_eq!(
            find_mode(
                &lines,
                Pos::new(0, 0),
                "ABC.".as_bytes(),
                Direction::Forward,
                false,
                false
            ),
            Some((Pos::new(0, 0), Pos::new(0, 6)))
        );
        assert_eq!(
            find_mode(
                &lines,
                Pos::new(0, 0),
                "，".as_bytes(),
                Direction::Forward,
                false,
                false
            ),
            None
        );
    }

    #[test]
    fn automatic_magic_search() {
        let lines = vec![b"abc 123".to_vec(), b"XYZ 456".to_vec(), Vec::new()];
        assert_eq!(
            find_mode(
                &lines,
                Pos::new(0, 0),
                b"[0-9]*",
                Direction::Forward,
                true,
                true
            ),
            Some((Pos::new(0, 4), Pos::new(0, 7)))
        );
        assert_eq!(
            find_mode(
                &lines,
                Pos::new(0, 0),
                b"^XYZ",
                Direction::Forward,
                false,
                true
            ),
            Some((Pos::new(1, 0), Pos::new(1, 3)))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ls() -> Vec<Vec<u8>> {
        vec![b"abc xyz".to_vec(), b"ABC abc".to_vec(), Vec::new()]
    }

    #[test]
    fn forward_search() {
        assert_eq!(
            find(&ls(), Pos::new(0, 0), b"xyz", Direction::Forward, true),
            Some((Pos::new(0, 4), Pos::new(0, 7)))
        );
        assert_eq!(
            find(&ls(), Pos::new(0, 5), b"abc", Direction::Forward, false),
            Some((Pos::new(1, 0), Pos::new(1, 3)))
        );
        assert_eq!(
            find(&ls(), Pos::new(0, 5), b"abc", Direction::Forward, true),
            Some((Pos::new(1, 4), Pos::new(1, 7)))
        );
        assert_eq!(
            find(&ls(), Pos::new(0, 0), b"zzz", Direction::Forward, false),
            None
        );
    }

    #[test]
    fn reverse_search() {
        assert_eq!(
            find(&ls(), Pos::new(1, 7), b"abc", Direction::Reverse, true),
            Some((Pos::new(1, 4), Pos::new(1, 7)))
        );
        assert_eq!(
            find(&ls(), Pos::new(1, 3), b"abc", Direction::Reverse, false),
            Some((Pos::new(1, 0), Pos::new(1, 3)))
        );
    }
}
