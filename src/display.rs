//! 画面表示。C 版 `display.C_` の vtputc/update に相当する簡易フレーム生成。
//! 実際の端末制御は `input.rs` が ANSI / crossterm で行う。

use crate::buffer::{BFCHG, MDMDMOD, MDVIEW};
use crate::editor::{ColorSpec, Editor};

/// バイト列の先頭から `tabsize` を考慮した表示桁数を返す。
pub fn column_of(bytes: &[u8], tabsize: usize) -> usize {
    let mut col = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\t' {
            let ts = tabsize.max(1);
            col += ts - col % ts;
            i += 1;
            continue;
        }
        let clen = char_len(&bytes[i..]);
        let s = String::from_utf8_lossy(&bytes[i..i + clen]);
        let w = if clen == 1 && bytes[i] < 0x20 {
            2
        } else {
            s.chars()
                .next()
                .and_then(unicode_width::UnicodeWidthChar::width)
                .unwrap_or(1)
        };
        col += w;
        i += clen;
    }
    col
}

fn char_len(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    for n in 1..=bytes.len().min(4) {
        if std::str::from_utf8(&bytes[..n]).is_ok() {
            return n;
        }
    }
    1
}

/// conceal 対象のマーカー位置を false にした表を作る。
fn conceal_flags(bytes: &[u8]) -> Vec<bool> {
    let mut keep = vec![true; bytes.len()];
    let mut i = 0usize;
    let markers: [&[u8]; 7] = [b"**", b"__", b"~~", b"`", b"*", b"_", b"~"];
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            keep[i] = false;
            i += 2;
            continue;
        }
        // [text](url) : 括弧と URL を隠し、テキストは残す
        if bytes[i] == b'[' {
            if let Some(close) = bytes[i + 1..].iter().position(|&c| c == b']') {
                let close = i + 1 + close;
                if close + 1 < bytes.len() && bytes[close + 1] == b'(' {
                    if let Some(end) = bytes[close + 2..].iter().position(|&c| c == b')') {
                        let end = close + 2 + end;
                        keep[i] = false;
                        keep[close] = false;
                        keep[close + 1] = false;
                        keep[end] = false;
                        keep[close + 2..end].fill(false);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        let mut matched = false;
        for m in markers {
            if i + m.len() >= bytes.len() || &bytes[i..i + m.len()] != m {
                continue;
            }
            if let Some(rel) = bytes[i + m.len()..].windows(m.len()).position(|w| w == m) {
                let end = i + m.len() + rel;
                if end > i + m.len() {
                    keep[i..i + m.len()].fill(false);
                    keep[end..end + m.len()].fill(false);
                    i = end + m.len();
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            i += 1;
        }
    }
    keep
}

/// Markdown の強調記号を隠して行を描画する (C 版 conceal 相当)。
/// 返す対応表は元バイト位置 → 表示桁。
pub fn render_line_concealed(bytes: &[u8], tabsize: usize, conceal: bool) -> (String, Vec<usize>) {
    if !conceal {
        return render_line(bytes, tabsize);
    }
    let keep = conceal_flags(bytes);
    let mut filtered = Vec::with_capacity(bytes.len());
    let mut orig_of = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        if keep[i] {
            filtered.push(b);
            orig_of.push(i);
        }
    }
    let (text, fmap) = render_line(&filtered, tabsize);
    let mut map = Vec::with_capacity(bytes.len() + 1);
    let mut j = 0usize;
    for o in 0..=bytes.len() {
        while j < orig_of.len() && orig_of[j] < o {
            j += 1;
        }
        map.push(fmap[j]);
    }
    (text, map)
}

/// 1 行を画面用文字列に変換し、バイトオフセット -> 表示桁の対応表を返す。
pub fn render_line(bytes: &[u8], tabsize: usize) -> (String, Vec<usize>) {
    let mut out = String::new();
    let mut map = Vec::with_capacity(bytes.len() + 1);
    let mut col = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        map.push(col);
        if bytes[i] == b'\t' {
            let ts = tabsize.max(1);
            let pad = ts - col % ts;
            out.push_str(&" ".repeat(pad));
            col += pad;
            i += 1;
        } else {
            let clen = char_len(&bytes[i..]);
            if clen == 1 && bytes[i] < 0x20 {
                out.push('^');
                out.push((bytes[i] + b'@') as char);
                col += 2;
            } else {
                let s = String::from_utf8_lossy(&bytes[i..i + clen]).to_string();
                let w = s
                    .chars()
                    .next()
                    .and_then(unicode_width::UnicodeWidthChar::width)
                    .unwrap_or(1);
                out.push_str(&s);
                col += w;
            }
            i += clen;
        }
    }
    map.push(col);
    (out, map)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub rows: Vec<String>,
    pub modeline: String,
    pub message: String,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub fg: ColorSpec,
    pub bg: ColorSpec,
    pub color_on: bool,
}

/// ドットが画面内に入るように window.top を調整する (C 版 `scroll()` 相当)。
pub fn adjust_top(ed: &mut Editor) {
    let rows = ed.window().rows.max(1);
    let dotline = ed.buffer().dot.line;
    let top = ed.window().top;
    let newtop = if dotline < top {
        dotline
    } else if dotline >= top + rows {
        dotline.saturating_sub(rows - 1)
    } else {
        top
    };
    if newtop != top {
        ed.window_mut().top = newtop;
    }
    ed.window_mut().flag = 0;
}

/// C 版 `prgname()` / `version()` 相当。
pub const PRGNAME: &str = "日本語μEMACS";
pub const VERSION: &str = "2.1k";

fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1))
        .sum()
}

fn truncate_width(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for c in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if col + w > max {
            break;
        }
        out.push(c);
        col += w;
    }
    out
}

/// C 版 `code_string()` 相当 (LF のときは改行種別を省略)。
fn code_string(b: &crate::buffer::Buffer) -> String {
    let prefix = match b.eol {
        crate::encoding::Eol::Lf => "",
        crate::encoding::Eol::Cr => "CR/",
        crate::encoding::Eol::Crlf => "CRLF/",
    };
    format!("{}{}", prefix, b.code.as_str())
}

enum MLinePart {
    Text(String),
    Dash,
    Fill,
}

fn expand_modeline(ed: &Editor) -> String {
    let b = ed.buffer();
    let mode_codes: String = (0..crate::buffer::MODE_NAMES.len())
        .filter(|i| b.mode & (1u32 << i) != 0)
        .map(|i| crate::buffer::MODE_CODES[i] as char)
        .collect();
    let mode_names: String = (0..crate::buffer::MODE_NAMES.len())
        .filter(|i| b.mode & (1u32 << i) != 0)
        .map(|i| crate::buffer::MODE_NAMES[i])
        .collect::<Vec<_>>()
        .join(" ");
    let fname = b.fname.clone().unwrap_or_default();
    let chars: Vec<char> = ed.mode_line_format.chars().collect();
    let mut parts: Vec<MLinePart> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] != '%' || i + 1 >= chars.len() {
            parts.push(MLinePart::Text(chars[i].to_string()));
            i += 1;
            continue;
        }
        let c = chars[i + 1];
        let text = match c {
            '*' => {
                if b.mode & MDVIEW != 0 {
                    "%".to_string()
                } else if b.flags & BFCHG != 0 {
                    "*".to_string()
                } else {
                    "=".to_string()
                }
            }
            'p' | 'P' => PRGNAME.to_string(),
            'v' | 'V' => VERSION.to_string(),
            'm' => mode_codes.clone(),
            'M' => mode_names.clone(),
            'b' | 'B' => b.name.clone(),
            'f' | 'F' => {
                if c == 'f' {
                    fname.rsplit('/').next().unwrap_or(&fname).to_string()
                } else {
                    fname.clone()
                }
            }
            'c' | 'C' => code_string(b),
            'l' => (b.dot.line + 1).to_string(),
            't' | 'T' => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                format!("{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60)
            }
            '<' | '>' => {
                let n = ed.prompt_depth.saturating_sub(1).min(64);
                c.to_string().repeat(n)
            }
            '%' => "%".to_string(),
            '=' => {
                parts.push(MLinePart::Dash);
                i += 2;
                continue;
            }
            '-' => {
                parts.push(MLinePart::Fill);
                i += 2;
                continue;
            }
            _ => format!("%{}", c),
        };
        parts.push(MLinePart::Text(text));
        i += 2;
    }
    let fixed: usize = parts
        .iter()
        .map(|p| match p {
            MLinePart::Text(s) => display_width(s),
            MLinePart::Dash => 1,
            MLinePart::Fill => 0,
        })
        .sum();
    let fill = ed.screen_cols.saturating_sub(fixed).min(4096);
    let mut out = String::new();
    for p in parts {
        match p {
            MLinePart::Text(s) => out.push_str(&s),
            MLinePart::Dash => out.push('='),
            MLinePart::Fill => out.push_str(&"=".repeat(fill)),
        }
    }
    truncate_width(&out, ed.screen_cols.min(4096))
}

pub fn frame(ed: &mut Editor) -> Frame {
    adjust_top(ed);
    let cols = ed.screen_cols;
    let rows_win = ed.window().rows;
    let top = ed.window().top;
    let gutter = if ed.show_line_numbers {
        ed.line_number_width.min(8)
    } else {
        0
    };
    let b = ed.buffer();
    let mut rows = Vec::with_capacity(rows_win);
    for r in 0..rows_win {
        let li = top + r;
        let mut line = String::new();
        if gutter > 0 && li < b.lines.len() && li + 1 < b.lines.len() + 1 {
            line.push_str(&format!(
                "{:>w$}",
                li + 1,
                w = gutter.saturating_sub(1).max(1)
            ));
            line.push(' ');
        } else if gutter > 0 {
            line.push_str(&" ".repeat(gutter));
        }
        let conceal = ed.md_conceal && b.mode & MDMDMOD != 0;
        let (text, _) = render_line_concealed(b.line_or_sentinel(li), ed.tabsize, conceal);
        line.push_str(&text);
        rows.push(line.chars().take(cols).collect());
    }
    let dotline = b.dot.line;
    let conceal = ed.md_conceal && b.mode & MDMDMOD != 0;
    let (_, map) = render_line_concealed(b.line_or_sentinel(dotline), ed.tabsize, conceal);
    let cursor_row = dotline.saturating_sub(top).min(rows_win.saturating_sub(1));
    let cursor_col = (gutter + *map.get(b.dot.off).unwrap_or(map.last().unwrap_or(&0)))
        .min(cols.saturating_sub(1));
    let modeline = expand_modeline(ed);
    let message = if ed.message.len() > cols {
        ed.message.chars().take(cols).collect()
    } else {
        ed.message.clone()
    };
    Frame {
        rows,
        modeline,
        message,
        cursor_row,
        cursor_col,
        fg: ed.fg,
        bg: ed.bg,
        color_on: ed.color_on,
    }
}

/// 前フレームと比べて書き換える必要のある行番号 (C 版 updpos/update 相当)。
pub fn changed_rows(prev: Option<&Frame>, cur: &Frame) -> Vec<usize> {
    match prev {
        None => (0..cur.rows.len()).collect(),
        Some(p) if p.rows.len() != cur.rows.len() => (0..cur.rows.len()).collect(),
        Some(p) => (0..cur.rows.len())
            .filter(|&i| p.rows.get(i) != cur.rows.get(i))
            .collect(),
    }
}

/// 全体再描画が必要か (サイズ・色設定の変化)。
pub fn needs_full_redraw(prev: Option<&Frame>, cur: &Frame) -> bool {
    match prev {
        None => true,
        Some(p) => {
            p.rows.len() != cur.rows.len()
                || p.fg != cur.fg
                || p.bg != cur.bg
                || p.color_on != cur.color_on
        }
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    fn frame(rows: &[&str]) -> Frame {
        Frame {
            rows: rows.iter().map(|s| s.to_string()).collect(),
            modeline: "line 1".to_string(),
            message: String::new(),
            cursor_row: 0,
            cursor_col: 1,
            fg: ColorSpec::Default,
            bg: ColorSpec::Default,
            color_on: false,
        }
    }

    #[test]
    fn markdown_conceal_hides_emphasis() {
        let line = b"**bold** and `code` [txt](url)";
        let (plain, _) = render_line_concealed(line, 8, false);
        let (hidden, _) = render_line_concealed(line, 8, true);
        assert_eq!(plain, "**bold** and `code` [txt](url)");
        assert_eq!(hidden, "bold and code txt");
        let (_, map) = render_line_concealed(line, 8, true);
        assert_eq!(map[2], 0);
        assert_eq!(map[line.len()], 17);
    }

    #[test]
    fn only_modified_rows_change() {
        let a = frame(&["abc", "def", "ghi"]);
        let mut b = frame(&["abc", "dXf", "ghi"]);
        assert_eq!(changed_rows(Some(&a), &b), vec![1]);
        assert!(!needs_full_redraw(Some(&a), &b));
        b.cursor_col = 2;
        assert_eq!(changed_rows(Some(&a), &b), vec![1]);
        b.rows.push("new".to_string());
        assert!(needs_full_redraw(Some(&a), &b));
    }

    #[test]
    fn color_change_forces_full_redraw() {
        let a = frame(&["x"]);
        let mut b = a.clone();
        b.color_on = true;
        assert!(needs_full_redraw(Some(&a), &b));
    }
}

#[cfg(test)]
mod modeline_tests {
    use super::*;
    use crate::editor::Editor;
    use crate::encoding::Eol;
    use crate::input::ScriptedIo;

    #[test]
    fn huge_screen_width_does_not_allocate() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        ed.screen_cols = usize::MAX;
        let f = frame(&mut ed);
        assert!(f.modeline.chars().count() <= 4096 + 128);
    }

    #[test]
    fn c_style_default_modeline() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        ed.buffer_mut().eol = Eol::Crlf;
        let f = frame(&mut ed);
        let expected = format!(
            "== 日本語μEMACS 2.1k () == main == File:  == CRLF/UTF8 {}",
            "=".repeat(25)
        );
        assert_eq!(f.modeline, expected);
    }
}
