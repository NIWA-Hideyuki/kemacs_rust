//! キーボードマクロ以外のスクリプト言語: `exec.C_` / `eval.C_` の
//! docmd() / トークン評価 / ユーザー変数 / 名前付きプロシージャに相当。

use crate::buffer::{MDVIEW, MODE_NAMES};
use crate::editor::{Editor, Status};

/// C 版 token() 相当。空白区切り、ダブルクォート、~n/~r/~t/~b/~f エスケープ。
pub fn tokenize(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let mut tok = Vec::new();
        let mut quoted = false;
        while i < bytes.len() {
            let c = bytes[i];
            if c == b'~' && i + 1 < bytes.len() {
                i += 1;
                let e = bytes[i];
                tok.push(match e {
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 8,
                    b'f' => 12,
                    _ => e,
                });
                i += 1;
                continue;
            }
            if c == b'"' {
                quoted = !quoted;
                i += 1;
                continue;
            }
            if !quoted && (c == b' ' || c == b'\t') {
                break;
            }
            tok.push(c);
            i += 1;
        }
        out.push(String::from_utf8_lossy(&tok).into_owned());
    }
    out
}

pub fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    ) || s.trim().parse::<i64>().map(|n| n != 0).unwrap_or(false)
}

/// C 版 stoi() と同じく 32bit 整数へ飽和させる。
fn stoi(s: &str) -> i32 {
    match s.trim().parse::<i64>() {
        Ok(n) => n.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        Err(_) => 0,
    }
}

fn ltos(v: bool) -> String {
    if v {
        "TRUE".to_string()
    } else {
        "FALSE".to_string()
    }
}

/// 関数のアリティ。C 版 funcs[] の f_type に相当。
fn arity(name: &str) -> usize {
    match name {
        "add" | "sub" | "times" | "div" | "mod" | "equal" | "less" | "greater" | "sequal"
        | "sless" | "sgreat" | "cat" | "left" | "right" | "and" | "or" => 2,
        "mid" => 3,
        "getkey" | "get-key" => 0,
        _ => 1,
    }
}

/// `$` で始まる組み込み/環境変数を返す。
pub fn get_bvar(ed: &Editor, name: &str) -> Option<String> {
    let b = ed.buffer();
    let v = match name.to_ascii_lowercase().as_str() {
        "fill-column" => ed.fillcol.to_string(),
        "tab-size" | "tabsize" => ed.tabsize.to_string(),
        "undo-limit" => b.undo_limit.to_string(),
        "linenumbers" => ltos(ed.show_line_numbers),
        "linenumwidth" => ed.line_number_width.to_string(),
        "mode-line-format" | "gmodeline" | "bmodeline" => ed.mode_line_format.clone(),
        "pagelen" => ed.window().rows.to_string(),
        "curcol" => (b.dot.off + 1).to_string(),
        "curline" => (b.dot.line + 1).to_string(),
        "curchar" => b
            .lines
            .get(b.dot.line)
            .and_then(|l| l.get(b.dot.off))
            .map(|&c| (c as char).to_string())
            .unwrap_or_default(),
        "cbufname" => b.name.clone(),
        "cfname" => b.fname.clone().unwrap_or_default(),
        "bcode" | "kcode" | "tcode" => b.code.as_str().to_string(),
        "crypt" => match b.crypt {
            crate::crypt::CryptKind::None => "NONE".to_string(),
            crate::crypt::CryptKind::Classic => "CLASSIC".to_string(),
            crate::crypt::CryptKind::Strong => "STRONG".to_string(),
        },
        "fcode" => ed.default_code.as_str().to_string(),
        "fcolor" => ed.fg.as_var(),
        "bcolor" => ed.bg.as_var(),
        "debug" => ltos(false),
        "script-interactive" => ltos(ed.script_interactive),
        "shell-command-mode" => ltos(true),
        "undo-count" => b.undo.len().to_string(),
        "asave" => ed.asave.to_string(),
        "acount" => ed.acount.to_string(),
        "md-preview-command" => ed
            .user_vars
            .get("md-preview-command")
            .cloned()
            .unwrap_or_default(),
        _ => std::env::var(name).ok()?,
    };
    Some(v)
}

/// 変数を設定する (eval.C_ setvar() 相当)。
pub fn set_bvar(ed: &mut Editor, name: &str, val: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if matches!(
        n.as_str(),
        "bcode" | "fcode" | "kcode" | "tcode" | "crlf" | "eol"
    ) {
        let (code, eol) = crate::encoding::Code::parse(val);
        if n == "bcode" || n == "kcode" || n == "tcode" {
            ed.buffer_mut().recode(code, eol);
        } else {
            ed.default_code = code;
            ed.default_eol = eol;
        }
        return true;
    }
    if let Ok(v) = val.trim().parse::<usize>() {
        match n.as_str() {
            "fill-column" => ed.fillcol = v,
            "tab-size" | "tabsize" => ed.tabsize = v,
            "undo-limit" => ed.buffer_mut().undo_limit = v,
            "linenumwidth" => ed.line_number_width = v.clamp(1, 8),
            "pagelen" => {
                ed.screen_rows = v.max(3);
                for w in ed.windows.iter_mut() {
                    w.rows = ed.screen_rows.saturating_sub(2).max(1);
                }
            }
            _ => {}
        }
    }
    if let Ok(v) = val.trim().parse::<usize>() {
        match n.as_str() {
            "asave" => ed.asave = v.max(1),
            "acount" => ed.acount = v,
            "py-indent-width" => ed.py_indent_width = v,
            "c-indent-width" => ed.c_indent_width = v,
            "c-brace-style" => ed.c_brace_style = v as i32,
            "c-gnu-indent" => ed.c_gnu_indent = v,
            "c-cont-indent" => ed.c_cont_indent = v,
            "c-case-indent" => ed.c_case_indent = v,
            "c-pp-indent" => ed.c_pp_indent = v,
            "md-indent-width" => ed.md_indent_width = v,
            "md-heading-max" => ed.md_heading_max = v,
            _ => {}
        }
    }
    let flag = truthy(val);
    match n.as_str() {
        "py-use-spaces" => ed.py_use_spaces = flag,
        "c-use-spaces" => ed.c_use_spaces = flag,
        "c-auto-detect" => ed.c_auto_detect = flag,
        "c-follow-source" => ed.c_follow_source = flag,
        "c-auto-close" => ed.c_auto_close = flag,
        "script-interactive" => ed.script_interactive = flag,
        "md-use-spaces" => ed.md_use_spaces = flag,
        "md-auto-list" => ed.md_auto_list = flag,
        "md-conceal" => ed.md_conceal = flag,
        "md-auto-pair" => ed.md_auto_pair = flag,
        _ => {}
    }
    if matches!(n.as_str(), "md-preview-command") {
        ed.user_vars.insert(n, val.to_string());
        return true;
    }
    match n.as_str() {
        "fcolor" => {
            ed.fg = crate::editor::ColorSpec::parse(val);
            ed.color_on = true;
        }
        "bcolor" => {
            ed.bg = crate::editor::ColorSpec::parse(val);
            ed.color_on = true;
        }
        "linenumbers" | "show-line-numbers" => ed.show_line_numbers = truthy(val),
        "mode-line-format" | "gmodeline" | "bmodeline" => ed.mode_line_format = val.to_string(),
        _ => {
            ed.user_vars.insert(n, val.to_string());
        }
    }
    true
}

/// トークンを評価して文字列を返す (eval.C_ getval() 相当)。
pub fn eval_token(ed: &mut Editor, tok: &str) -> String {
    let t = tok.trim();
    if let Some(inner) = t.strip_prefix('"') {
        return inner.strip_suffix('"').unwrap_or(inner).to_string();
    }
    if let Some(name) = t.strip_prefix('$') {
        return get_bvar(ed, name).unwrap_or_default();
    }
    if let Some(name) = t.strip_prefix('%') {
        return ed.user_vars.get(name).cloned().unwrap_or_default();
    }
    if let Some(name) = t.strip_prefix('#') {
        if name.is_empty() {
            return String::from_utf8_lossy(&ed.buffer().text()).into_owned();
        }
        if let Some(i) = ed.find_buffer(name) {
            return String::from_utf8_lossy(&ed.buffers[i].text()).into_owned();
        }
        return String::new();
    }
    if let Some(fname) = t.strip_prefix('&') {
        let args: Vec<String> = (0..arity(fname))
            .map(|_| {
                let a = ed.macro_args.pop_front().unwrap_or_default();
                eval_token(ed, &a)
            })
            .collect();
        return eval_func(ed, &fname.to_ascii_lowercase(), &args);
    }
    if t.starts_with('@') {
        return "1".to_string();
    }
    if t.starts_with('*') {
        return "0".to_string();
    }
    t.to_string()
}

fn eval_func(ed: &mut Editor, name: &str, a: &[String]) -> String {
    let s = |i: usize| a.get(i).cloned().unwrap_or_default();
    match name {
        "add" => stoi(&s(0))
            .checked_add(stoi(&s(1)))
            .map(|v| v.to_string())
            .unwrap_or_else(|| "ERROR".to_string()),
        "sub" => stoi(&s(0))
            .checked_sub(stoi(&s(1)))
            .map(|v| v.to_string())
            .unwrap_or_else(|| "ERROR".to_string()),
        "times" => stoi(&s(0))
            .checked_mul(stoi(&s(1)))
            .map(|v| v.to_string())
            .unwrap_or_else(|| "ERROR".to_string()),
        "div" => {
            let d = stoi(&s(1));
            if d == 0 || (stoi(&s(0)) == i32::MIN && d == -1) {
                "ERROR".to_string()
            } else {
                (stoi(&s(0)) / d).to_string()
            }
        }
        "mod" => {
            let d = stoi(&s(1));
            if d == 0 || (stoi(&s(0)) == i32::MIN && d == -1) {
                "ERROR".to_string()
            } else {
                (stoi(&s(0)) % d).to_string()
            }
        }
        "neg" => {
            let v = stoi(&s(0));
            if v == i32::MIN {
                "ERROR".to_string()
            } else {
                (-v).to_string()
            }
        }
        "abs" => {
            let v = stoi(&s(0));
            if v == i32::MIN {
                "ERROR".to_string()
            } else {
                v.abs().to_string()
            }
        }
        "cat" => format!("{}{}", s(0), s(1)),
        "left" => {
            let text = s(0);
            let n = stoi(&s(1)).clamp(0, text.chars().count() as i32) as usize;
            let end = text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
            text[..end].to_string()
        }
        "right" => {
            let text = s(0);
            let n = stoi(&s(1)).clamp(0, text.chars().count() as i32) as usize;
            let start = if n == 0 {
                text.len()
            } else {
                text.char_indices().rev().nth(n - 1).map_or(0, |(i, _)| i)
            };
            text[start..].to_string()
        }
        "mid" => {
            let text = s(0);
            let chars = text.chars().count();
            let from = stoi(&s(1)).clamp(1, chars as i32 + 1) as usize - 1;
            let len = stoi(&s(2)).clamp(0, (chars - from) as i32) as usize;
            let a = text.char_indices().nth(from).map_or(text.len(), |(i, _)| i);
            let b = text
                .char_indices()
                .nth(from + len)
                .map_or(text.len(), |(i, _)| i);
            text[a..b].to_string()
        }
        "length" => s(0).len().to_string(),
        "upper" => s(0).to_uppercase(),
        "lower" => s(0).to_lowercase(),
        "truth" => ltos(stoi(&s(0)) != 0),
        "not" => ltos(stoi(&s(0)) == 0),
        "equal" => ltos(stoi(&s(0)) == stoi(&s(1))),
        "less" => ltos(stoi(&s(0)) < stoi(&s(1))),
        "greater" => ltos(stoi(&s(0)) > stoi(&s(1))),
        "sequal" => ltos(s(0) == s(1)),
        "sless" => ltos(s(0) < s(1)),
        "sgreat" => ltos(s(0) > s(1)),
        "and" => ltos(truthy(&s(0)) && truthy(&s(1))),
        "or" => ltos(truthy(&s(0)) || truthy(&s(1))),
        "ascii" => s(0)
            .as_bytes()
            .first()
            .map(|&b| b.to_string())
            .unwrap_or_default(),
        "chr" => {
            let c = stoi(&s(0)).clamp(0, 255) as u8;
            (c as char).to_string()
        }
        "getkey" | "get-key" => {
            let k = ed.read_key();
            let c = crate::keycode::code(k);
            (c as char).to_string()
        }
        "rnd" => {
            let m = stoi(&s(0));
            if m == 0 || m == i32::MIN {
                "ERROR".to_string()
            } else {
                (ed.seed().abs() % (m.abs() as i64) + 1).to_string()
            }
        }
        "ind" => eval_token(ed, &s(0)),
        "env" => std::env::var(s(0)).unwrap_or_default(),
        "bmode" | "gmode" => {
            let want = s(0).to_ascii_uppercase();
            let mode = if name == "bmode" {
                ed.buffer().mode
            } else {
                ed.global_mode
            };
            let found = MODE_NAMES
                .iter()
                .position(|m| *m == want)
                .map(|i| mode & (1 << i) != 0)
                .unwrap_or(false);
            ltos(found)
        }
        "bind" => ed.command_for_key_name(&s(0)).unwrap_or_default(),
        "escape" => s(0)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_string()
                } else {
                    format!("\\{}", c)
                }
            })
            .collect(),
        _ => "ERROR".to_string(),
    }
}

/// 式全体 (関数 + 引数) を評価する。引数は再帰的に評価される。
pub fn eval_expr(ed: &mut Editor, expr: &str) -> String {
    let toks = tokenize(expr);
    if toks.is_empty() {
        return String::new();
    }
    let saved = std::mem::take(&mut ed.macro_args);
    for t in toks[1..].iter().rev() {
        ed.macro_args.push_front(t.clone());
    }
    let v = eval_token(ed, &toks[0]);
    ed.macro_args = saved;
    v
}

/// 行を実行する (exec.C_ docmd() 相当)。
pub fn docmd(ed: &mut Editor, line: &str) -> Status {
    let mut cline = line.trim_start();
    if cline.is_empty() || cline.starts_with(';') {
        return Status::True;
    }
    if cline.starts_with("!endm") || cline.starts_with("[end]") {
        ed.mstore = None;
        return Status::True;
    }
    if let Some(name) = ed.mstore.clone() {
        let bi = ed.find_buffer_or_create(&name);
        ed.buffers[bi].lines.push(cline.as_bytes().to_vec());
        return Status::True;
    }
    if cline.starts_with('*') && !cline.starts_with("**") {
        return Status::True;
    }
    if let Some(rest) = cline.strip_prefix("!force") {
        cline = rest.trim_start();
    }
    if let Some(d) = cline.strip_prefix('!') {
        let d = d.trim_start();
        let (kind, arg) = d.split_once(char::is_whitespace).unwrap_or((d, ""));
        let kind = kind.to_ascii_lowercase();
        match kind.as_str() {
            "if" => {
                let parent = ed.script_active();
                let cond = parent && truthy(&eval_expr(ed, arg.trim()));
                ed.if_stack.push(crate::editor::IfFrame {
                    parent_active: parent,
                    cond,
                    in_else: false,
                });
            }
            "else" => {
                if let Some(f) = ed.if_stack.last_mut() {
                    f.in_else = !f.in_else;
                } else {
                    ed.if_stack.push(crate::editor::IfFrame {
                        parent_active: true,
                        cond: false,
                        in_else: true,
                    });
                }
            }
            "endif" => {
                ed.if_stack.pop();
            }
            "return" => {
                return if ed.script_active() {
                    Status::Ret
                } else {
                    Status::True
                };
            }
            "goto" => {
                if ed.script_active() {
                    ed.golabel = Some(arg.trim().trim_start_matches('*').to_string());
                    return Status::Goline;
                }
            }
            _ => {
                ed.mlwrite(format!("%Unknown Directive: {}", kind));
                return Status::False;
            }
        }
        ed.execlevel = if ed.script_active() { 0 } else { 1 };
        return Status::True;
    }
    if !ed.script_active() {
        return Status::True;
    }
    let tokens = tokenize(cline);
    if tokens.is_empty() {
        return Status::True;
    }
    let mut idx = 0usize;
    let mut f = false;
    let mut n = 1i32;
    if !crate::commands::known_command(&tokens[0]) {
        f = true;
        let v = eval_token(ed, &tokens[0]);
        n = v.parse::<i32>().unwrap_or(0);
        idx = 1;
        if idx >= tokens.len() {
            return Status::False;
        }
    }
    let cmd = tokens[idx].clone();
    idx += 1;
    if !ed.script_interactive {
        ed.macro_args.extend(tokens[idx..].iter().cloned());
    }
    let st = ed.execute_named(&cmd, f, n);
    ed.macro_args.clear();
    st
}

/// バッファー内の行を順に実行する (exec.C_ dobuf() 相当)。
pub fn dobuf(ed: &mut Editor, bufname: &str) -> Status {
    let Some(bi) = ed.find_buffer(bufname) else {
        ed.mlwrite("No such procedure");
        return Status::False;
    };
    if ed.script_depth >= 32 {
        ed.mlwrite("Script recursion too deep");
        return Status::False;
    }
    ed.script_depth += 1;
    let lines = ed.buffers[bi].lines.clone();
    let mut i = 0usize;
    let saved_stack = std::mem::take(&mut ed.if_stack);
    let saved_execlevel = ed.execlevel;
    ed.execlevel = 0;
    let mut steps = 0usize;
    let result = loop {
        if i >= lines.len() || steps >= 100_000 {
            break Status::True;
        }
        steps += 1;
        let text = String::from_utf8_lossy(&lines[i]).into_owned();
        let st = docmd(ed, &text);
        match st {
            Status::Goline => {
                let label = ed.golabel.take().unwrap_or_default();
                if let Some(pos) = lines.iter().position(|l| {
                    let s = String::from_utf8_lossy(l);
                    s.trim() == format!("*{}", label)
                }) {
                    i = pos;
                } else {
                    ed.mlwrite(format!("No such label: {}", label));
                    break Status::False;
                }
            }
            Status::Ret => break Status::True,
            Status::False | Status::Abort => break st,
            _ => i += 1,
        }
    };
    let st = result;
    ed.script_depth -= 1;
    ed.if_stack = saved_stack;
    ed.execlevel = saved_execlevel;
    st
}

fn proc_name(buf: &str) -> String {
    format!("[[{}]]", buf)
}

/// 名前付きプロシージャの記録を開始する (exec.C_ storeproc() 相当)。
pub fn storeproc(ed: &mut Editor, f: bool, n: i32) -> Status {
    if f {
        return storemac(ed, n);
    }
    let Some(name) = ed.prompt("Procedure name: ", None) else {
        return Status::False;
    };
    let bn = proc_name(name.trim());
    let bi = ed.find_buffer_or_create(&bn);
    ed.buffers[bi].lines = vec![Vec::new()];
    ed.buffers[bi].mode &= !MDVIEW;
    ed.mstore = Some(bn);
    Status::True
}

/// 番号付きマクロの記録を開始する (exec.C_ storemac() 相当)。
pub fn storemac(ed: &mut Editor, n: i32) -> Status {
    if !(1..=40).contains(&n) {
        ed.mlwrite("Macro number out of range");
        return Status::False;
    }
    let bn = format!("[[{:02}]]", n);
    let bi = ed.find_buffer_or_create(&bn);
    ed.buffers[bi].lines = vec![Vec::new()];
    ed.mstore = Some(bn);
    Status::True
}

pub fn execproc(ed: &mut Editor, n: i32) -> Status {
    let Some(name) = ed.prompt("Execute procedure: ", None) else {
        return Status::False;
    };
    let bn = proc_name(name.trim());
    for _ in 0..n.max(1) {
        let st = dobuf(ed, &bn);
        if st != Status::True {
            return st;
        }
    }
    Status::True
}

pub fn execbuf(ed: &mut Editor) -> Status {
    let Some(name) = ed.prompt("Execute buffer: ", None) else {
        return Status::False;
    };
    dobuf(ed, name.trim())
}

pub fn cbuf(ed: &mut Editor, n: i32) -> Status {
    dobuf(ed, &format!("[[{:02}]]", n))
}

/// キーシーケンスを (key, f, n) として再生する (main.C_ ctlxe() 相当)。
pub fn play_macro(ed: &mut Editor, rep: i32) -> Status {
    if ed.kbdmode != crate::editor::KbdMode::Stop {
        ed.mlwrite("%Macro already active");
        return Status::False;
    }
    if rep <= 0 || ed.kbdm.is_empty() {
        return Status::True;
    }
    let keys = ed.kbdm.clone();
    ed.in_macro_play = true;
    let mut ok = true;
    for _ in 0..rep {
        for &(k, f, n) in &keys {
            let st = ed.execute(k, f, n);
            if st == Status::False {
                ok = false;
                break;
            }
        }
        if !ok {
            break;
        }
    }
    ed.in_macro_play = false;
    Status::from_ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ScriptedIo;
    use std::collections::VecDeque;

    #[test]
    fn tokenize_escapes_and_quotes() {
        assert_eq!(
            tokenize("  &add 1 ~n2 \"a b\""),
            vec!["&add", "1", "\n2", "a b"]
        );
    }

    #[test]
    fn variables_and_functions() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        ed.user_vars.insert("x".to_string(), "3".to_string());
        assert_eq!(eval_token(&mut ed, "%x"), "3");
        ed.macro_args = VecDeque::from(vec!["%x".to_string(), "4".to_string()]);
        assert_eq!(eval_token(&mut ed, "&add"), "7");
        assert_eq!(eval_token(&mut ed, "$fill-column"), "72");
    }

    #[test]
    fn nested_if_stack() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        docmd(&mut ed, "!if TRUE");
        docmd(&mut ed, "!if FALSE");
        docmd(&mut ed, "insert-string wrong");
        docmd(&mut ed, "!else");
        docmd(&mut ed, "insert-string inner");
        docmd(&mut ed, "!endif");
        docmd(&mut ed, "insert-string outer");
        docmd(&mut ed, "!endif");
        assert_eq!(ed.buffer().text(), b"innerouter");
        assert!(ed.if_stack.is_empty());
    }

    #[test]
    fn interactive_script_prompts_minibuffer() {
        let mut io = ScriptedIo::with_prompts(&["first", "second"]);
        io.keys.push_back(b'z' as crate::keycode::Ckey);
        let mut ed = Editor::new(Box::new(io), 24, 80);
        ed.script_interactive = true;
        docmd(&mut ed, "insert-string");
        docmd(&mut ed, "insert-string");
        assert_eq!(eval_token(&mut ed, "&getkey"), "z");
        assert_eq!(ed.buffer().text(), b"firstsecond");
        assert_eq!(ed.prompt_depth, 0);
    }

    #[test]
    fn string_functions_respect_char_boundaries() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        let two = ["あい".to_string(), "1".to_string()];
        assert_eq!(eval_func(&mut ed, "left", &two), "あ");
        assert_eq!(eval_func(&mut ed, "right", &two), "い");
        let three = ["あい".to_string(), "2".to_string(), "1".to_string()];
        assert_eq!(eval_func(&mut ed, "mid", &three), "い");
    }

    #[test]
    fn dobuf_rejects_excessive_recursion() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        let bi = ed.find_buffer_or_create("loop");
        ed.buffers[bi].lines = vec![b"noop".to_vec()];
        ed.script_depth = 32;
        assert_eq!(dobuf(&mut ed, "loop"), Status::False);
        assert_eq!(ed.script_depth, 32);
    }

    #[test]
    fn getkey_reads_terminal_input() {
        let mut io = ScriptedIo::new();
        io.keys.push_back(b'x' as crate::keycode::Ckey);
        let mut ed = Editor::new(Box::new(io), 24, 80);
        assert_eq!(eval_token(&mut ed, "&getkey"), "x");
    }

    #[test]
    fn docmd_with_directives_and_labels() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        docmd(&mut ed, "insert-string hello");
        assert_eq!(ed.buffer().text(), b"hello");
        docmd(&mut ed, "!if FALSE");
        docmd(&mut ed, "insert-string skipped");
        docmd(&mut ed, "!endif");
        assert_eq!(ed.buffer().text(), b"hello");
    }
}
