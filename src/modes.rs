//! C / Python / Markdown モードの詳細なインデントと自動補完。
//! C 版 `cmode.C_`, `python.C_`, `mdmode.C_` に相当。

use crate::buffer::{Pos, MDCMOD, MDMDMOD, MDPYMOD};
use crate::editor::{Editor, Status};

fn ts(ed: &Editor) -> usize {
    if ed.tabsize > 0 {
        ed.tabsize
    } else {
        8
    }
}

fn line_bytes(ed: &Editor, li: usize) -> Vec<u8> {
    ed.buffer().lines.get(li).cloned().unwrap_or_default()
}

fn leading_ws_end(line: &[u8]) -> usize {
    let mut i = 0;
    while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
        i += 1;
    }
    i
}

fn indent_cols(line: &[u8], tab: usize) -> usize {
    let mut col = 0;
    for &c in line {
        match c {
            b' ' => col += 1,
            b'\t' => col = (col / tab + 1) * tab,
            _ => break,
        }
    }
    col
}

/// 指定列数をタブ/スペースで表現する。
fn indent_bytes(cols: usize, use_spaces: bool, width: usize) -> Vec<u8> {
    if cols == 0 {
        return Vec::new();
    }
    if use_spaces {
        return vec![b' '; cols];
    }
    let w = width.max(1);
    let mut out = vec![b'\t'; cols / w];
    out.extend(std::iter::repeat_n(b' ', cols % w));
    out
}

fn replace_line(ed: &mut Editor, li: usize, text: Vec<u8>, off: usize) {
    let bi = ed.buffer_index();
    if li < ed.buffers[bi].lines.len() {
        ed.buffers[bi].lines[li] = text;
        ed.buffers[bi].dot = Pos::new(li, off.min(ed.buffers[bi].line_len(li)));
    }
}

/// 現在行の先頭インデントを cols 列に置き換える (C 版 *_delete_indent + insert_indent)。
fn set_indent(ed: &mut Editor, li: usize, cols: usize, use_spaces: bool, width: usize) {
    let line = line_bytes(ed, li);
    let ws = leading_ws_end(&line);
    let mut new = indent_bytes(cols, use_spaces, width);
    new.extend_from_slice(&line[ws..]);
    replace_line(ed, li, new, indent_bytes(cols, use_spaces, width).len());
}

fn last_nonws(line: &[u8], off: usize) -> u8 {
    for &c in line[..off.min(line.len())].iter().rev() {
        if c != b' ' && c != b'\t' {
            return c;
        }
    }
    0
}

fn skip_ws(line: &[u8], mut off: usize) -> usize {
    while off < line.len() && (line[off] == b' ' || line[off] == b'\t') {
        off += 1;
    }
    off
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn is_keyword(line: &[u8], off: usize, kw: &[u8]) -> bool {
    if off + kw.len() > line.len() {
        return false;
    }
    if &line[off..off + kw.len()] != kw {
        return false;
    }
    if off + kw.len() < line.len() && is_ident(line[off + kw.len()]) {
        return false;
    }
    if off > 0 && (is_ident(line[off - 1]) || line[off - 1] == b'.') {
        return false;
    }
    true
}

fn starts_pp(line: &[u8], kw: &[u8]) -> bool {
    let mut off = skip_ws(line, 0);
    if off >= line.len() || line[off] != b'#' {
        return false;
    }
    off = skip_ws(line, off + 1);
    is_keyword(line, off, kw)
}

/// コメント / 文字列リテラルをスキップした後のオフセット。
fn skip_comment_or_string(line: &[u8], off: usize) -> usize {
    let len = line.len();
    if off >= len {
        return off;
    }
    let c = line[off];
    if c == b'/' && off + 1 < len {
        if line[off + 1] == b'/' {
            return len;
        }
        if line[off + 1] == b'*' {
            let mut i = off + 2;
            while i + 1 < len {
                if line[i] == b'*' && line[i + 1] == b'/' {
                    return i + 2;
                }
                i += 1;
            }
            return len;
        }
    }
    if c == b'"' || c == b'\'' {
        let mut i = off + 1;
        while i < len {
            if line[i] == b'\\' && i + 1 < len {
                i += 2;
                continue;
            }
            if line[i] == c {
                return i + 1;
            }
            i += 1;
        }
        return i.min(len);
    }
    off
}

fn bracket_balance(line: &[u8], from: usize) -> (i32, i32, i32) {
    let (mut pa, mut br, mut cu) = (0i32, 0i32, 0i32);
    let mut i = from;
    while i < line.len() {
        let skipped = skip_comment_or_string(line, i);
        if skipped != i {
            i = skipped;
            continue;
        }
        match line[i] {
            b'(' => pa += 1,
            b')' => pa -= 1,
            b'[' => br += 1,
            b']' => br -= 1,
            b'{' => cu += 1,
            b'}' => cu -= 1,
            _ => {}
        }
        i += 1;
    }
    (pa, br, cu)
}

fn is_ctrl_keyword(line: &[u8], off: usize) -> bool {
    [
        "if", "for", "while", "switch", "else", "do", "try", "catch", "finally",
    ]
    .iter()
    .any(|k| is_keyword(line, off, k.as_bytes()))
}

/// C 版 c_is_function_definition(): 関数定義行か (paren が閉じて `{` または行末)。
fn is_function_definition(line: &[u8]) -> bool {
    let mut depth = 0i32;
    let mut found = false;
    let mut i = skip_ws(line, 0);
    while i < line.len() {
        let skipped = skip_comment_or_string(line, i);
        if skipped != i {
            i = skipped;
            continue;
        }
        match line[i] {
            b'(' => {
                depth += 1;
                found = true;
            }
            b')' => depth -= 1,
            b'{' if depth == 0 && found => return true,
            b';' => return false,
            _ => {}
        }
        i += 1;
    }
    depth == 0 && found && last_nonws(line, line.len()) == b')'
}

fn prev_nonblank(ed: &Editor, li: usize) -> Option<usize> {
    let mut i = li;
    while i > 0 {
        i -= 1;
        let l = line_bytes(ed, i);
        if !l.iter().all(|c| *c == b' ' || *c == b'\t') {
            return Some(i);
        }
    }
    None
}

/// 後方スキャンで現在の brace / paren / pp コンテキストの基準インデントを求める。
fn scan_brace_level(ed: &Editor, li: usize) -> usize {
    let (mut cu, mut pa, mut br, mut pp) = (0i32, 0i32, 0i32, 0i32);
    let mut i = li;
    while i > 0 {
        i -= 1;
        let line = line_bytes(ed, i);
        if starts_pp(&line, b"if") || starts_pp(&line, b"ifdef") || starts_pp(&line, b"ifndef") {
            if pp == 0 {
                return indent_cols(&line, ts(ed)) + ed.c_pp_indent;
            }
            pp -= 1;
        } else if starts_pp(&line, b"endif") {
            pp += 1;
        } else if (starts_pp(&line, b"else") || starts_pp(&line, b"elif")) && pp == 0 {
            return indent_cols(&line, ts(ed));
        }
        let (p, b, c) = bracket_balance(&line, 0);
        cu += c;
        pa += p;
        br += b;
        if cu == 0 && pa == 0 && br == 0 && pp == 0 {
            let off = skip_ws(&line, 0);
            if is_ctrl_keyword(&line, off) {
                return indent_cols(&line, ts(ed));
            }
            continue;
        }
        if cu > 0 && pa == 0 && br == 0 && pp == 0 {
            return indent_cols(&line, ts(ed));
        }
    }
    0
}

fn scan_matching_if(ed: &Editor, li: usize) -> usize {
    let (mut cu, mut pa) = (0i32, 0i32);
    let mut i = li;
    while i > 0 {
        i -= 1;
        let line = line_bytes(ed, i);
        if starts_pp(&line, b"if")
            || starts_pp(&line, b"ifdef")
            || starts_pp(&line, b"ifndef")
            || starts_pp(&line, b"else")
            || starts_pp(&line, b"elif")
        {
            continue;
        }
        let (p, _, c) = bracket_balance(&line, 0);
        cu += c;
        pa += p;
        if cu == 0 && pa == 0 {
            let off = skip_ws(&line, 0);
            if is_keyword(&line, off, b"else") {
                continue;
            }
            if is_keyword(&line, off, b"if") {
                return indent_cols(&line, ts(ed));
            }
        }
    }
    0
}

fn has_brace_after(line: &[u8], from: usize, to: usize) -> bool {
    let mut i = from;
    while i < to {
        let skipped = skip_comment_or_string(line, i);
        if skipped != i {
            i = skipped;
            continue;
        }
        if line[i] == b'{' {
            return true;
        }
        i += 1;
    }
    false
}

/// C 版 cinsert(): Enter 時の自動インデント。
pub fn c_insert(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for _ in 0..n {
        let li = ed.buffer().dot.line;
        let off = ed.buffer().dot.off;
        let line = line_bytes(ed, li);
        let cur_indent = indent_cols(&line[..off.min(line.len())], ts(ed));
        let last = last_nonws(&line, off);
        let mut target = cur_indent;
        if ed.c_follow_source {
            if let Some(p) = prev_nonblank(ed, li) {
                target = indent_cols(&line_bytes(ed, p), ts(ed));
            }
        } else {
            match last {
                b'{' => target = cur_indent + ed.c_indent_width,
                b'}' => {
                    let mut bal = 0i32;
                    let mut j = off;
                    while j > 0 {
                        j -= 1;
                        let skipped = skip_comment_or_string(&line, j);
                        if skipped != j {
                            j = skipped;
                            continue;
                        }
                        if line[j] == b'}' {
                            bal += 1;
                        } else if line[j] == b'{' {
                            if bal == 0 {
                                target = indent_cols(&line[..j], ts(ed));
                                break;
                            }
                            bal -= 1;
                        }
                    }
                    if j == 0 && last == b'}' {
                        target = scan_brace_level(ed, li);
                    }
                }
                _ => {
                    let (pa, br, cu) = bracket_balance(&line, 0);
                    if pa + br + cu > 0 {
                        target = cur_indent + ed.c_cont_indent;
                    } else {
                        let o = skip_ws(&line, 0);
                        if is_ctrl_keyword(&line, o) && !has_brace_after(&line, o, off) {
                            target = cur_indent + ed.c_indent_width;
                        }
                    }
                }
            }
            if starts_pp(&line, b"if") || starts_pp(&line, b"ifdef") || starts_pp(&line, b"ifndef")
            {
                target = cur_indent + ed.c_pp_indent + ed.c_indent_width;
            } else if starts_pp(&line, b"else")
                || starts_pp(&line, b"elif")
                || starts_pp(&line, b"endif")
            {
                target = scan_brace_level(ed, li);
            }
            if ed.c_brace_style == 6
                && ed.c_brace_func
                && last == b')'
                && is_function_definition(&line[..off.min(line.len())])
            {
                // Stroustrup: 関数の開き波括弧は次行 (同列) に置く
                target = cur_indent;
            }
            if ed.c_brace_style == 7 && last == b'}' {
                target = scan_brace_level(ed, li);
            }
        }
        if !ed.buffer_mut().newline() {
            return Status::False;
        }
        let pad = indent_bytes(target, ed.c_use_spaces, ed.c_indent_width);
        ed.buffer_mut().insert_bytes(&pad);
    }
    Status::True
}

/// C 版 cindent(): 現在行を再インデント (M-J)。
pub fn c_indent(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for step in 0..n.max(1) as usize {
        let li = ed.buffer().dot.line;
        let line = line_bytes(ed, li);
        let mut target = 0usize;
        if let Some(p) = prev_nonblank(ed, li) {
            let prev = line_bytes(ed, p);
            let pi = indent_cols(&prev, ts(ed));
            target = pi;
            let last = last_nonws(&prev, prev.len());
            match last {
                b'{' => target = pi + ed.c_indent_width,
                b'}' => target = scan_brace_level(ed, li),
                b';' => target = pi,
                _ => {
                    let (pa, br, _) = bracket_balance(&prev, 0);
                    if pa + br > 0 {
                        target = pi + ed.c_cont_indent;
                    } else {
                        let off = skip_ws(&prev, 0);
                        if is_ctrl_keyword(&prev, off) && !has_brace_after(&prev, off, prev.len()) {
                            target = pi + ed.c_indent_width;
                        }
                    }
                }
            }
            if starts_pp(&prev, b"if") || starts_pp(&prev, b"ifdef") || starts_pp(&prev, b"ifndef")
            {
                target = pi + ed.c_pp_indent + ed.c_indent_width;
            }
            let o = skip_ws(&line, 0);
            let dedent = is_keyword(&line, o, b"else")
                || is_keyword(&line, o, b"case")
                || is_keyword(&line, o, b"default")
                || line.get(o) == Some(&b'}');
            if dedent {
                let scan = scan_brace_level(ed, li);
                if is_keyword(&line, o, b"else")
                    || is_keyword(&line, o, b"do")
                    || line.get(o) == Some(&b'}')
                {
                    target = scan;
                } else {
                    target = scan + ed.c_case_indent;
                }
            }
            if ed.c_brace_style == 7 && is_keyword(&line, o, b"else") {
                target = scan_matching_if(ed, li);
            }
            // Stroustrup (style 6): 関数定義の `{` は独立行で制御行と同じ列、
            // 制御の `else` は対応する if の列に揃える。
            if ed.c_brace_style == 6 && ed.c_brace_func {
                if line.get(o) == Some(&b'{') && is_function_definition(&prev) {
                    target = pi;
                }
                if is_keyword(&line, o, b"else") {
                    target = scan_matching_if(ed, li);
                }
            }
            if starts_pp(&line, b"else") || starts_pp(&line, b"elif") || starts_pp(&line, b"endif")
            {
                target = scan_brace_level(ed, li);
            }
            let o2 = skip_ws(&line, 0);
            if is_keyword(&line, o2, b"while") && last_nonws(&line, line.len()) == b';' {
                if let Some(d) = prev_nonblank(ed, li) {
                    let dl = line_bytes(ed, d);
                    if is_keyword(&dl, skip_ws(&dl, 0), b"do") {
                        target = indent_cols(&dl, ts(ed));
                    }
                }
            }
        }
        set_indent(ed, li, target, ed.c_use_spaces, ed.c_indent_width);
        if step + 1 < n.max(1) as usize {
            let next = ed.buffer().dot.line + 1;
            if next < ed.buffer().lines.len() {
                ed.buffer_mut().dot = Pos::new(next, 0);
            }
        }
    }
    Status::True
}

/// C 版 cindent_region(): リージョン内の行を前方スキャンで再インデント。
pub fn c_indent_region(ed: &mut Editor) -> Status {
    let Some(mark) = ed.buffer().mark else {
        return Status::False;
    };
    let dot = ed.buffer().dot;
    let (start, end) = if mark <= dot {
        (mark, dot)
    } else {
        (dot, mark)
    };
    let mut li = start.line;
    while li <= end.line && li < ed.buffer().lines.len() {
        ed.buffer_mut().dot = Pos::new(li, 0);
        let _ = c_indent(ed, 1);
        li += 1;
    }
    Status::True
}

/// C 版 c_detect_style(): 既存ソースからブレーススタイルを推定。
pub fn c_detect_style(ed: &mut Editor, _n: i32) -> Status {
    let mut own_line = 0usize;
    let mut same_line = 0usize;
    for line in &ed.buffer().lines {
        let t = skip_ws(line, 0);
        if line.get(t) == Some(&b'{') {
            own_line += 1;
        }
        if last_nonws(line, line.len()) == b'{' && line.get(t) != Some(&b'{') {
            same_line += 1;
        }
    }
    ed.c_brace_style = if own_line > same_line { 1 } else { 0 };
    ed.mlwrite(format!("[C style: {}]", ed.c_brace_style));
    Status::True
}

pub fn c_follow_source(ed: &mut Editor) -> Status {
    ed.c_follow_source = !ed.c_follow_source;
    ed.mlwrite(format!("[follow-source: {}]", ed.c_follow_source));
    Status::True
}

pub fn c_mode(ed: &mut Editor, f: bool, n: i32) -> Status {
    let on = if f {
        n > 0
    } else {
        ed.buffer().mode & MDCMOD == 0
    };
    if on {
        ed.buffer_mut().mode |= MDCMOD;
    } else {
        ed.buffer_mut().mode &= !MDCMOD;
    }
    ed.mlwrite(format!("C mode {}", if on { "ON" } else { "OFF" }));
    Status::True
}

// ---------------------------------------------------------------------------
// Python Mode
// ---------------------------------------------------------------------------

fn ends_with_colon(line: &[u8]) -> bool {
    last_nonws(line, line.len()) == b':'
}

fn is_dedent_kw(line: &[u8]) -> bool {
    let o = skip_ws(line, 0);
    ["else", "elif", "except", "finally"].iter().any(|k| {
        let kw = k.as_bytes();
        if o + kw.len() > line.len() || &line[o..o + kw.len()] != kw {
            return false;
        }
        o + kw.len() == line.len() || matches!(line[o + kw.len()], b':' | b' ')
    })
}

fn py_target(ed: &Editor, li: usize) -> usize {
    let Some(p) = prev_nonblank(ed, li) else {
        return 0;
    };
    let prev = line_bytes(ed, p);
    let mut col = indent_cols(&prev, ts(ed));
    if ends_with_colon(&prev) {
        col += ed.py_indent_width;
    }
    let cur = line_bytes(ed, li);
    if is_dedent_kw(&cur) {
        col = col.saturating_sub(ed.py_indent_width);
    }
    col
}

pub fn py_insert(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for _ in 0..n {
        let li = ed.buffer().dot.line;
        let off = ed.buffer().dot.off;
        let line = line_bytes(ed, li);
        let mut indent = indent_cols(&line[..off.min(line.len())], ts(ed));
        if off > 0 && last_nonws(&line, off) == b':' {
            indent += ed.py_indent_width;
        }
        if !ed.buffer_mut().newline() {
            return Status::False;
        }
        let pad = indent_bytes(indent, ed.py_use_spaces, ed.py_indent_width);
        ed.buffer_mut().insert_bytes(&pad);
    }
    Status::True
}

pub fn py_indent(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for step in 0..n as usize {
        let li = ed.buffer().dot.line;
        let target = py_target(ed, li);
        set_indent(ed, li, target, ed.py_use_spaces, ed.py_indent_width);
        if step + 1 < n as usize {
            let next = li + 1;
            if next < ed.buffer().lines.len() {
                ed.buffer_mut().dot = Pos::new(next, 0);
            }
        }
    }
    Status::True
}

pub fn py_backspace(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let off = ed.buffer().dot.off;
    let line = line_bytes(ed, li);
    if off > 0 && line[..off].iter().all(|c| *c == b' ' || *c == b'\t') {
        let del = if ed.py_use_spaces {
            ed.py_indent_width.min(off)
        } else {
            1
        };
        let mut new = line;
        new.drain(off - del..off);
        replace_line(ed, li, new, off - del);
        return Status::True;
    }
    Status::from_ok(ed.buffer_mut().back_delete(false).0)
}

pub fn py_indent_region(ed: &mut Editor) -> Status {
    let Some(mark) = ed.buffer().mark else {
        return Status::False;
    };
    let dot = ed.buffer().dot;
    let (start, end) = if mark <= dot {
        (mark, dot)
    } else {
        (dot, mark)
    };
    let mut li = start.line;
    while li <= end.line && li < ed.buffer().lines.len() {
        let line = line_bytes(ed, li);
        if !line.iter().all(|c| *c == b' ' || *c == b'\t') {
            let mut col = if li > 0 {
                let prev = line_bytes(ed, li - 1);
                let mut c = indent_cols(&prev, ts(ed));
                if ends_with_colon(&prev) || last_nonws(&prev, prev.len()) == b'\\' {
                    c += ed.py_indent_width;
                }
                c
            } else {
                0
            };
            if is_dedent_kw(&line) {
                col = col.saturating_sub(ed.py_indent_width);
            }
            set_indent(ed, li, col, ed.py_use_spaces, ed.py_indent_width);
        }
        li += 1;
    }
    ed.buffer_mut().mark = None;
    Status::True
}

pub fn py_mode(ed: &mut Editor, f: bool, n: i32) -> Status {
    let on = if f {
        n > 0
    } else {
        ed.buffer().mode & MDPYMOD == 0
    };
    if on {
        ed.buffer_mut().mode |= MDPYMOD;
    } else {
        ed.buffer_mut().mode &= !MDPYMOD;
    }
    ed.mlwrite(format!("Python mode {}", if on { "ON" } else { "OFF" }));
    Status::True
}

// ---------------------------------------------------------------------------
// Markdown Mode
// ---------------------------------------------------------------------------

fn md_is_fence(line: &[u8]) -> bool {
    let mut i = leading_ws_end(line);
    let mut n = 0;
    while i < line.len() && line[i] == b'`' {
        i += 1;
        n += 1;
    }
    n >= 3
}

pub fn md_in_fence_at(ed: &Editor, li: usize) -> bool {
    let mut count = 0;
    for i in 0..li.min(ed.buffer().lines.len()) {
        if md_is_fence(&ed.buffer().lines[i]) {
            count += 1;
        }
    }
    count % 2 == 1
}

pub fn md_heading_level(line: &[u8]) -> usize {
    let mut i = 0;
    let mut n = 0;
    while i < line.len() && line[i] == b' ' {
        i += 1;
        if i > 3 {
            return 0;
        }
    }
    while i < line.len() && line[i] == b'#' {
        i += 1;
        n += 1;
    }
    if n == 0 || n > 6 {
        return 0;
    }
    if i < line.len() && line[i] != b' ' && line[i] != b'\t' {
        return 0;
    }
    n
}

fn heading_span(line: &[u8]) -> Option<(usize, usize, usize)> {
    let level = md_heading_level(line);
    if level == 0 {
        return None;
    }
    let mut end = 0;
    while end < line.len() && line[end] == b'#' {
        end += 1;
    }
    while end < line.len() && (line[end] == b' ' || line[end] == b'\t') {
        end += 1;
    }
    Some((0, end, level))
}

/// 行頭の引用プレフィックス (>, >> など) の終端オフセット。
fn md_quote_end(line: &[u8]) -> usize {
    let mut i = leading_ws_end(line);
    loop {
        if i < line.len() && line[i] == b'>' {
            i += 1;
            while i < line.len() && (line[i] == b' ' || line[i] == b'\t') {
                i += 1;
            }
        } else {
            break;
        }
    }
    i
}

#[derive(Clone, Copy, Debug)]
struct MdMarker {
    kind: u8, // 0=bullet, 1=ordered, 2=task
    start: usize,
    end: usize,
    num: usize,
}

fn md_parse_marker(line: &[u8], from: usize) -> Option<MdMarker> {
    let mut i = skip_ws(line, from);
    let start = i;
    let c = *line.get(i)?;
    if c == b'-' || c == b'*' || c == b'+' {
        i += 1;
        if i < line.len() && matches!(line[i], b' ' | b'\t') {
            i += 1;
        } else if i < line.len() {
            return None;
        }
        let mut kind = 0;
        let mut end = i;
        if end + 3 <= line.len()
            && line[end] == b'['
            && matches!(line[end + 1], b' ' | b'x' | b'X')
            && line[end + 2] == b']'
        {
            kind = 2;
            end += 3;
            if end < line.len() && line[end] == b' ' {
                end += 1;
            }
        }
        return Some(MdMarker {
            kind,
            start,
            end,
            num: 0,
        });
    }
    if c.is_ascii_digit() && c != b'0' {
        let mut num = 0usize;
        while i < line.len() && line[i].is_ascii_digit() {
            num = num
                .saturating_mul(10)
                .saturating_add((line[i] - b'0') as usize);
            i += 1;
        }
        if i < line.len() && line[i] == b'.' {
            i += 1;
            if i < line.len() && matches!(line[i], b' ' | b'\t') {
                i += 1;
            } else if i < line.len() {
                return None;
            }
            return Some(MdMarker {
                kind: 1,
                start,
                end: i,
                num,
            });
        }
    }
    None
}

pub fn md_insert(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for _ in 0..n {
        let li = ed.buffer().dot.line;
        let off = ed.buffer().dot.off;
        let line = line_bytes(ed, li);
        let indent = indent_cols(&line[..off.min(line.len())], ts(ed));
        let mut cont = Vec::new();
        if !md_in_fence_at(ed, li) && off > 0 {
            let q = md_quote_end(&line);
            if q <= off {
                cont.extend_from_slice(&line[..q]);
                if let Some(m) = md_parse_marker(&line, q) {
                    if m.end <= off && ed.md_auto_list {
                        match m.kind {
                            0 => {
                                cont.push(line[m.start]);
                                cont.push(b' ');
                            }
                            1 => {
                                cont.extend_from_slice(
                                    format!("{}. ", m.num.saturating_add(1)).as_bytes(),
                                );
                            }
                            2 => cont.extend_from_slice(b"- [ ] "),
                            _ => {}
                        }
                    }
                } else if q >= line.len() && cont.last() == Some(&b' ') {
                    cont.pop();
                }
            }
        }
        if !ed.buffer_mut().newline() {
            return Status::False;
        }
        let mut text = indent_bytes(indent, ed.md_use_spaces, ts(ed));
        text.extend_from_slice(&cont);
        ed.buffer_mut().insert_bytes(&text);
    }
    Status::True
}

pub fn md_backspace(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let off = ed.buffer().dot.off;
    let line = line_bytes(ed, li);
    if off > 0 && line[..off].iter().all(|c| *c == b' ' || *c == b'\t') {
        let del = if ed.md_use_spaces {
            ed.md_indent_width.min(off)
        } else {
            1
        };
        let mut new = line;
        new.drain(off - del..off);
        replace_line(ed, li, new, off - del);
        return Status::True;
    }
    let q = md_quote_end(&line);
    if let Some(m) = md_parse_marker(&line, q) {
        if m.end <= off && line[m.end..off].iter().all(|c| *c == b' ' || *c == b'\t') {
            let del = m.end - q;
            let mut new = line;
            new.drain(m.end - del..m.end);
            replace_line(ed, li, new, off - del);
            return Status::True;
        }
    }
    Status::from_ok(ed.buffer_mut().back_delete(false).0)
}

pub fn md_tab(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let off = ed.buffer().dot.off;
    let line = line_bytes(ed, li);
    if let Some(pos) = line
        .iter()
        .enumerate()
        .skip(off + 1)
        .find(|(_, c)| **c == b'|')
    {
        ed.buffer_mut().dot = Pos::new(li, pos.0);
        return Status::True;
    }
    let q = md_quote_end(&line);
    if line[..off].iter().all(|c| *c == b' ' || *c == b'\t') && md_parse_marker(&line, q).is_some()
    {
        let cols = indent_cols(&line[..q], ts(ed)) + ed.md_indent_width;
        let ws = leading_ws_end(&line);
        let mut new = indent_bytes(cols, ed.md_use_spaces, ts(ed));
        new.extend_from_slice(&line[ws..]);
        replace_line(ed, li, new, cols);
        return Status::True;
    }
    do_tab(ed)
}

fn do_tab(ed: &mut Editor) -> Status {
    if ed.tabsize == 0 {
        return Status::from_ok(ed.buffer_mut().insert_repeat(1, b'\t'));
    }
    let li = ed.buffer().dot.line;
    let off = ed.buffer().dot.off;
    let line = line_bytes(ed, li);
    let col = crate::display::column_of(&line[..off.min(line.len())], ed.tabsize);
    let pad = ed.tabsize - col % ed.tabsize;
    Status::from_ok(ed.buffer_mut().insert_repeat(pad, b' '))
}

pub fn md_indent(ed: &mut Editor, n: i32) -> Status {
    if n <= 0 {
        return Status::False;
    }
    for step in 0..n.max(1) as usize {
        let li = ed.buffer().dot.line;
        let line = line_bytes(ed, li);
        if line.is_empty() || md_is_fence(&line) {
            continue;
        }
        let q = md_quote_end(&line);
        let ws_end = skip_ws(&line, q);
        let bcols = indent_cols(&line[q..ws_end], ts(ed));
        let found = md_parse_marker(&line, q);
        let mut desired = bcols;
        let width = ed.md_indent_width.max(1);
        if found.is_some() {
            let level = bcols / width;
            if level * width != bcols {
                desired = level * width;
            }
        }
        let mut prev_cols = 0usize;
        let mut prev_is_list = false;
        if li > 0 {
            let prev = line_bytes(ed, li - 1);
            let pq = md_quote_end(&prev);
            prev_cols = indent_cols(&prev[pq..skip_ws(&prev, pq)], ts(ed));
            prev_is_list = md_parse_marker(&prev, pq).is_some();
        }
        if prev_is_list && bcols >= prev_cols + width && desired > prev_cols + width {
            desired = prev_cols + width;
        }
        let mut new = indent_bytes(desired, ed.md_use_spaces, ts(ed));
        new.extend_from_slice(&line[ws_end..]);
        replace_line(ed, li, new, q + desired);
        if step + 1 < n.max(1) as usize {
            let next = li + 1;
            if next < ed.buffer().lines.len() {
                ed.buffer_mut().dot = Pos::new(next, 0);
            }
        }
    }
    Status::True
}

/// C 版 pattbl[] によるファイル名パターンでの自動モード設定。
pub fn auto_mode(ed: &mut Editor, bi: usize, fname: &str) {
    let lower = fname.to_ascii_lowercase();
    let bit = if lower.ends_with(".c") || lower.ends_with(".h") {
        Some(MDCMOD)
    } else if lower.ends_with(".py") {
        Some(MDPYMOD)
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Some(MDMDMOD)
    } else {
        None
    };
    if let Some(b) = bit {
        ed.buffers[bi].mode |= b;
    }
    if ed.c_auto_detect && bit == Some(MDCMOD) {
        let save = ed.buffer_index();
        ed.set_window_buffer(bi);
        let _ = c_detect_style(ed, 1);
        ed.set_window_buffer(save);
    }
}

pub fn md_mode(ed: &mut Editor, f: bool, n: i32) -> Status {
    let on = if f {
        n > 0
    } else {
        ed.buffer().mode & MDMDMOD == 0
    };
    if on {
        ed.buffer_mut().mode |= MDMDMOD;
    } else {
        ed.buffer_mut().mode &= !MDMDMOD;
    }
    ed.mlwrite(format!("Markdown mode {}", if on { "ON" } else { "OFF" }));
    Status::True
}

/// M-J のモード別ディスパッチ (C 版 smart_indent() 相当)。
pub fn indent_auto(ed: &mut Editor, n: i32) -> Status {
    let mode = ed.buffer().mode;
    if mode & MDMDMOD != 0 {
        md_indent(ed, n)
    } else if mode & MDPYMOD != 0 {
        py_indent(ed, n)
    } else if mode & MDCMOD != 0 {
        c_indent(ed, n)
    } else {
        Status::False
    }
}

/// Enter のモード別ディスパッチ (C 版 newline() 相当)。
pub fn newline_auto(ed: &mut Editor, n: i32) -> Status {
    let mode = ed.buffer().mode;
    if mode & MDMDMOD != 0 {
        md_insert(ed, n)
    } else if mode & MDPYMOD != 0 {
        py_insert(ed, n)
    } else if mode & MDCMOD != 0 {
        c_insert(ed, n)
    } else if n > 0 {
        let mut ok = true;
        for _ in 0..n {
            ok &= ed.buffer_mut().newline();
        }
        Status::from_ok(ok)
    } else {
        Status::False
    }
}

/// Backspace のモード別ディスパッチ。
pub fn backspace_auto(ed: &mut Editor) -> Status {
    let mode = ed.buffer().mode;
    if mode & MDMDMOD != 0 {
        md_backspace(ed)
    } else if mode & MDPYMOD != 0 {
        py_backspace(ed)
    } else {
        Status::from_ok(ed.buffer_mut().back_delete(false).0)
    }
}

/// TAB のモード別ディスパッチ。
pub fn tab_auto(ed: &mut Editor, f: bool, n: i32) -> Status {
    if n == 1 && ed.buffer().mode & MDMDMOD != 0 {
        return md_tab(ed);
    }
    if n < 0 {
        return Status::False;
    }
    if f && n > 1 {
        ed.tabsize = n as usize;
        return Status::True;
    }
    do_tab(ed)
}

// --- Markdown 見出し / リスト操作 ---

fn replace_heading(ed: &mut Editor, li: usize, level: usize) -> Status {
    let line = line_bytes(ed, li);
    let Some((_, end, _)) = heading_span(&line) else {
        return Status::False;
    };
    let mut new = vec![b'#'; level];
    if level > 0 {
        new.push(b' ');
    }
    new.extend_from_slice(&line[end..]);
    let off = new.len().min(level + 1);
    replace_line(ed, li, new, off);
    Status::True
}

pub fn md_insert_heading(ed: &mut Editor, f: bool, n: i32) -> Status {
    let max = ed.md_heading_max.max(1);
    let level = if f && n > 0 { n as usize } else { 1 }.clamp(1, max);
    let mut text = vec![b'#'; level];
    text.push(b' ');
    ed.buffer_mut().insert_bytes(&text);
    Status::True
}

pub fn md_heading_up(ed: &mut Editor, n: i32) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let level = md_heading_level(&line);
    if level == 0 {
        ed.buffer_mut().insert_bytes(b"# ");
        return Status::True;
    }
    let new = (level + n.max(1) as usize).min(ed.md_heading_max);
    replace_heading(ed, li, new)
}

pub fn md_heading_down(ed: &mut Editor, n: i32) -> Status {
    let li = ed.buffer().dot.line;
    let level = md_heading_level(&line_bytes(ed, li));
    if level == 0 {
        return Status::False;
    }
    replace_heading(ed, li, level.saturating_sub(n.max(1) as usize))
}

pub fn md_heading_cycle(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let level = md_heading_level(&line_bytes(ed, li));
    let new = if level >= ed.md_heading_max {
        0
    } else {
        level + 1
    };
    replace_heading(ed, li, new)
}

pub fn md_list_toggle(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let q = md_quote_end(&line);
    let ws = leading_ws_end(&line);
    let indent = &line[ws..q];
    let body_start = md_parse_marker(&line, q).map(|m| m.end).unwrap_or(q);
    let body = &line[body_start..];
    let mut new = indent.to_vec();
    new.extend_from_slice(&line[..q]);
    new.extend_from_slice(
        &line[ws..q]
            .iter()
            .filter(|c| **c == b'>')
            .copied()
            .collect::<Vec<_>>(),
    );
    // 既存マーカーを剥がす/付ける
    let mut out = Vec::new();
    out.extend_from_slice(&line[..ws]);
    out.extend_from_slice(&line[ws..q]);
    if md_parse_marker(&line, q).is_some() {
        out.extend_from_slice(body);
    } else {
        out.extend_from_slice(b"- ");
        out.extend_from_slice(body);
    }
    let off = out.len().min(q + 2 + body.len());
    replace_line(ed, li, out, off);
    let _ = new;
    Status::True
}

pub fn md_list_demote(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let mut out = indent_bytes(ed.md_indent_width, ed.md_use_spaces, ts(ed));
    out.extend_from_slice(&line);
    replace_line(ed, li, out, ed.md_indent_width);
    Status::True
}

pub fn md_list_promote(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let ws = leading_ws_end(&line);
    let cut = ws.min(if ed.md_use_spaces {
        ed.md_indent_width
    } else {
        1
    });
    let mut out = line;
    out.drain(..cut);
    replace_line(ed, li, out, ws.saturating_sub(cut));
    Status::True
}

pub fn md_list_renumber(ed: &mut Editor) -> Status {
    let bi = ed.buffer_index();
    let mut num = 1usize;
    let mut li = 0usize;
    while li < ed.buffer().lines.len() {
        let line = line_bytes(ed, li);
        let q = md_quote_end(&line);
        if let Some(m) = md_parse_marker(&line, q) {
            if m.kind == 1 {
                let mut new = line[..m.start].to_vec();
                new.extend_from_slice(format!("{}. ", num).as_bytes());
                new.extend_from_slice(&line[m.end..]);
                ed.buffers[bi].lines[li] = new;
                num += 1;
            }
        }
        li += 1;
    }
    Status::True
}

pub fn md_task_toggle(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let q = md_quote_end(&line);
    let Some(m) = md_parse_marker(&line, q) else {
        return Status::False;
    };
    if m.kind != 2 {
        return Status::False;
    }
    let mut new = line;
    new[m.start + 3] = if new[m.start + 3] == b' ' { b'x' } else { b' ' };
    replace_line(ed, li, new, m.end);
    Status::True
}

/// リージョンまたは単語をマーカーで囲む (太字/斜体/コード/打ち消し)。
fn wrap_markup(ed: &mut Editor, marker: &[u8]) -> Status {
    let start = ed.buffer().dot;
    let Some(mark) = ed.buffer().mark else {
        return Status::False;
    };
    let (a, b) = if mark <= start {
        (mark, start)
    } else {
        (start, mark)
    };
    let mut text = ed.buffer().text();
    let off_a = ed.buffer().pos_to_offset(a);
    let off_b = ed.buffer().pos_to_offset(b);
    if off_a > off_b || off_b > text.len() {
        return Status::False;
    }
    let mut rep = marker.to_vec();
    rep.extend_from_slice(&text[off_a..off_b]);
    rep.extend_from_slice(marker);
    text.splice(off_a..off_b, rep.iter().copied());
    ed.buffer_mut().lines = text.split(|&c| c == b'\n').map(|s| s.to_vec()).collect();
    ed.buffer_mut().dot = Pos::new(a.line, a.off + marker.len());
    ed.buffer_mut().mark = None;
    Status::True
}

pub fn md_bold(ed: &mut Editor) -> Status {
    wrap_markup(ed, b"**")
}
pub fn md_italic(ed: &mut Editor) -> Status {
    wrap_markup(ed, b"*")
}
pub fn md_code(ed: &mut Editor) -> Status {
    wrap_markup(ed, b"`")
}
pub fn md_strike(ed: &mut Editor) -> Status {
    wrap_markup(ed, b"~~")
}

pub fn md_insert_link(ed: &mut Editor) -> Status {
    let label = ed.prompt("Link text: ", None).unwrap_or_default();
    let url = ed.prompt("URL: ", None).unwrap_or_default();
    let text = format!("[{}]({})", label, url);
    ed.buffer_mut().insert_bytes(text.as_bytes());
    Status::True
}

pub fn md_quote_toggle(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let line = line_bytes(ed, li);
    let mut out = line.clone();
    if md_quote_end(&line) > 0 {
        let i = leading_ws_end(&line);
        while i < out.len() && out[i] == b'>' {
            out.remove(i);
            if i < out.len() && out[i] == b' ' {
                out.remove(i);
            }
        }
    } else {
        let mut pre = vec![b'>', b' '];
        pre.extend_from_slice(&line);
        out = pre;
    }
    replace_line(ed, li, out.clone(), out.len());
    Status::True
}

pub fn md_quote_region(ed: &mut Editor) -> Status {
    let Some(mark) = ed.buffer().mark else {
        return Status::False;
    };
    let a = mark.min(ed.buffer().dot).line;
    let b = mark.max(ed.buffer().dot).line;
    for li in a..=b {
        let bi = ed.buffer_index();
        if li < ed.buffers[bi].lines.len() {
            let mut l = vec![b'>', b' '];
            l.extend_from_slice(&ed.buffers[bi].lines[li]);
            ed.buffers[bi].lines[li] = l;
        }
    }
    Status::True
}

pub fn md_insert_fence(ed: &mut Editor) -> Status {
    let lang = ed.prompt("Fence language: ", None).unwrap_or_default();
    let text = format!("```{}\n\n```\n", lang);
    ed.buffer_mut().insert_bytes(text.as_bytes());
    Status::True
}

pub fn md_goto_fence(ed: &mut Editor) -> Status {
    let cur = ed.buffer().dot.line;
    let (dir_start, step) = (0isize, 1isize);
    let mut i = cur as isize + step;
    while i >= 0 && i < ed.buffer().lines.len() as isize {
        if md_is_fence(&line_bytes(ed, i as usize)) {
            ed.buffer_mut().dot = Pos::new(i as usize, 0);
            return Status::True;
        }
        i += if dir_start < 0 { -1 } else { 1 };
    }
    Status::False
}

pub fn md_table_next_cell(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let off = ed.buffer().dot.off;
    let line = line_bytes(ed, li);
    if let Some(pos) = line
        .iter()
        .enumerate()
        .skip(off + 1)
        .find(|(_, c)| **c == b'|')
    {
        ed.buffer_mut().dot = Pos::new(li, pos.0);
        return Status::True;
    }
    Status::False
}

pub fn md_outline(ed: &mut Editor) -> Status {
    let mut lines = Vec::new();
    for (i, line) in ed.buffer().lines.iter().enumerate() {
        let level = md_heading_level(line);
        if level > 0 {
            let title = String::from_utf8_lossy(line)
                .trim_start_matches('#')
                .trim()
                .to_string();
            lines.extend(
                format!(
                    "{:indent$}{} {}",
                    "",
                    "-".repeat(level),
                    title,
                    indent = (level - 1) * 2
                )
                .lines()
                .map(|s| s.as_bytes().to_vec()),
            );
        }
        let _ = i;
    }
    let bi = ed.find_buffer_or_create("*Outline*");
    ed.buffers[bi].lines = if lines.is_empty() {
        vec![Vec::new()]
    } else {
        lines
    };
    ed.set_window_buffer(bi);
    Status::True
}

pub fn md_goto_heading(ed: &mut Editor) -> Status {
    let title = ed.prompt("Heading: ", None).unwrap_or_default();
    for (i, line) in ed.buffer().lines.iter().enumerate() {
        if md_heading_level(line) > 0 && String::from_utf8_lossy(line).contains(&title) {
            ed.buffer_mut().dot = Pos::new(i, 0);
            return Status::True;
        }
    }
    Status::False
}

pub fn md_auto_pair_char(c: u8) -> bool {
    matches!(c, b'*' | b'`' | b'[' | b'_' | b'~')
}

/// MDMDMOD + $md-auto-pair のときの自己挿入。
pub fn md_self_insert(ed: &mut Editor, c: u8, n: i32) -> Status {
    let pair = if c == b'[' { b']' } else { c };
    ed.buffer_mut().insert_repeat(n.max(1) as usize, c);
    ed.buffer_mut().insert_repeat(n.max(1) as usize, pair);
    ed.buffer_mut().dot.off = ed.buffer_mut().dot.off.saturating_sub(n.max(1) as usize);
    Status::True
}

pub fn md_conceal_toggle(ed: &mut Editor) -> Status {
    ed.md_conceal = !ed.md_conceal;
    ed.mlwrite(format!("[conceal: {}]", ed.md_conceal));
    Status::True
}

#[cfg(test)]
mod md_tests {
    use super::*;
    use crate::input::ScriptedIo;

    fn ed() -> Editor {
        Editor::new(Box::new(ScriptedIo::new()), 24, 80)
    }

    #[test]
    fn bullet_continuation() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"- item");
        e.buffer_mut().mode |= MDMDMOD;
        md_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"- item\n- ");
    }

    #[test]
    fn ordered_numbering_and_task() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"3. three");
        e.buffer_mut().mode |= MDMDMOD;
        md_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"3. three\n4. ");
        let mut e2 = ed();
        e2.buffer_mut().insert_bytes(b"- [x] done");
        md_insert(&mut e2, 1);
        assert_eq!(e2.buffer().text(), b"- [x] done\n- [ ] ");
    }

    #[test]
    fn quote_and_fence_behavior() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"> quote");
        e.buffer_mut().mode |= MDMDMOD;
        md_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"> quote\n> ");
        let mut e2 = ed();
        e2.buffer_mut().insert_bytes(b"```rust\ncode");
        e2.buffer_mut().dot = Pos::new(1, 4);
        md_insert(&mut e2, 1);
        assert_eq!(e2.buffer().text(), b"```rust\ncode\n");
    }

    #[test]
    fn backspace_removes_marker() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"- item");
        e.buffer_mut().dot = Pos::new(0, 2);
        md_backspace(&mut e);
        assert_eq!(e.buffer().text(), b"item");
    }

    #[test]
    fn heading_commands() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"title");
        e.buffer_mut().goto_bob();
        md_insert_heading(&mut e, false, 1);
        assert_eq!(e.buffer().text(), b"# title");
        md_heading_up(&mut e, 1);
        assert_eq!(e.buffer().text(), b"## title");
        md_heading_down(&mut e, 1);
        assert_eq!(e.buffer().text(), b"# title");
        md_heading_cycle(&mut e);
        assert_eq!(e.buffer().text(), b"## title");
    }

    #[test]
    fn auto_pair_and_markup() {
        let mut e = ed();
        md_self_insert(&mut e, b'*', 1);
        assert_eq!(e.buffer().text(), b"**");
        assert_eq!(e.buffer().dot.off, 1);
        let mut e2 = ed();
        e2.buffer_mut().insert_bytes(b"hello");
        e2.buffer_mut().mark = Some(Pos::new(0, 0));
        e2.buffer_mut().dot = Pos::new(0, 5);
        md_bold(&mut e2);
        assert_eq!(e2.buffer().text(), b"**hello**");
    }

    #[test]
    fn smart_indent_dispatch() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b" - item");
        e.buffer_mut().mode |= MDMDMOD;
        indent_auto(&mut e, 1);
        assert_eq!(e.buffer().text(), b"- item");
    }
}

#[cfg(test)]
mod c_py_tests {
    use super::*;
    use crate::input::ScriptedIo;

    fn ed() -> Editor {
        Editor::new(Box::new(ScriptedIo::new()), 24, 80)
    }

    #[test]
    fn c_insert_after_brace() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"if (x) {");
        e.buffer_mut().mode |= MDCMOD;
        assert_eq!(c_insert(&mut e, 1), Status::True);
        assert_eq!(e.buffer().text(), b"if (x) {\n\t");
    }

    #[test]
    fn c_insert_no_brace_and_close() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"x = 1");
        e.buffer_mut().mode |= MDCMOD;
        c_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"x = 1\n");
        let mut e2 = ed();
        e2.buffer_mut().insert_bytes(b"    }");
        e2.buffer_mut().mode |= MDCMOD;
        c_insert(&mut e2, 1);
        assert_eq!(e2.buffer().text(), b"    }\n");
    }

    #[test]
    fn c_indent_after_open_brace() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"if (x) {\nfoo");
        e.buffer_mut().mode |= MDCMOD;
        e.buffer_mut().dot = Pos::new(1, 3);
        c_indent(&mut e, 1);
        assert_eq!(e.buffer().text(), b"if (x) {\n\tfoo");
    }

    #[test]
    fn stroustrup_function_brace() {
        let mut e = ed();
        e.c_brace_style = 6;
        e.c_brace_func = true;
        e.buffer_mut().mode |= MDCMOD;
        e.buffer_mut().insert_bytes(b"int foo()");
        c_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"int foo()\n");
        e.buffer_mut().insert_bytes(b"{");
        c_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"int foo()\n{\n\t");
        // M-J: 獨立行の { は関数定義行と同じ列
        e.buffer_mut().dot = Pos::new(1, 1);
        c_indent(&mut e, 1);
        assert_eq!(e.buffer().text(), b"int foo()\n{\n\t");
    }

    #[test]
    fn py_insert_and_indent() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"if True:\nx = 1");
        e.buffer_mut().mode |= MDPYMOD;
        e.buffer_mut().dot = Pos::new(0, 8);
        py_insert(&mut e, 1);
        assert_eq!(e.buffer().text(), b"if True:\n    \nx = 1");
        e.buffer_mut().dot = Pos::new(2, 0);
        py_indent(&mut e, 1);
        assert_eq!(e.buffer().text(), b"if True:\n    \n    x = 1");
    }

    #[test]
    fn py_backspace_dedents_one_level() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"    x = 1");
        e.buffer_mut().dot = Pos::new(0, 4);
        py_backspace(&mut e);
        assert_eq!(e.buffer().text(), b"x = 1");
    }
}
