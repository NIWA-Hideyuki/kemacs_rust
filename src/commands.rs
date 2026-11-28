//! コマンド実装。コマンド名は C 版 `efunc.H_` の名前をそのまま使う。
//! キーバインド表は `ebind.H_` と同じキーコードで `default_bindings()` に定義。

use crate::buffer::{Pos, MDCMOD, MDMDMOD, MDPYMOD, MDWRAP};
use crate::editor::Editor;
use crate::keycode::{code, Ckey, Status, Status as St, DEL};

pub mod extra;

const CFCPCN: i32 = 0x0001;
const CFKILL: i32 = 0x0002;

/// C 版 `ebind.H_` の KEYTAB と同じ内容。
pub fn default_bindings() -> Vec<(Ckey, &'static str)> {
    use crate::keycode::{ctl, ctlx, ctlxlctl, meta, metac_ctl, spec};
    vec![
        (ctl(b'A'), "gotobol"),
        (ctl(b'B'), "backchar"),
        (ctl(b'C'), "shell-escape"),
        (ctl(b'D'), "forwdel"),
        (ctl(b'E'), "gotoeol"),
        (ctl(b'F'), "forwchar"),
        (ctl(b'H'), "backdel"),
        (ctl(b'I'), "tab"),
        (ctl(b'J'), "indent"),
        (ctl(b'K'), "killtext"),
        (ctl(b'L'), "refreshscr"),
        (ctl(b'M'), "newline"),
        (ctl(b'N'), "forwline"),
        (ctl(b'O'), "openline"),
        (ctl(b'P'), "backline"),
        (ctl(b'Q'), "quote"),
        (ctl(b'R'), "backsearch"),
        (ctl(b'S'), "forwsearch"),
        (ctl(b'T'), "twiddle"),
        (ctl(b'U'), "unarg"),
        (ctl(b'V'), "forwpage"),
        (ctl(b'W'), "killregion"),
        (ctl(b'X'), "cex"),
        (ctl(b'Y'), "yank"),
        (ctl(b'Z'), "backpage"),
        (ctl(b'['), "meta_pfx"),
        (ctlxlctl(b'B'), "listbuffers"),
        (ctlxlctl(b'C'), "quit"),
        (ctlx(b'D'), "bktoshell"),
        (ctlxlctl(b'D'), "detab"),
        (ctlxlctl(b'E'), "entab"),
        (ctlxlctl(b'F'), "filefind"),
        (ctlxlctl(b'G'), "md_insert_heading"),
        (ctlxlctl(b'H'), "md_heading_up"),
        (ctlxlctl(b'I'), "insfile"),
        (ctlxlctl(b'L'), "lowerregion"),
        (ctlxlctl(b'M'), "delmode"),
        (ctlxlctl(b'N'), "mvdnwind"),
        (ctlxlctl(b'O'), "deblank"),
        (ctlxlctl(b'P'), "mvupwind"),
        (ctlxlctl(b'R'), "fileread"),
        (ctlxlctl(b'S'), "filesave"),
        (ctlxlctl(b'T'), "trim"),
        (ctlxlctl(b'U'), "upperregion"),
        (ctlxlctl(b'V'), "viewfile"),
        (ctlxlctl(b'W'), "filewrite"),
        (ctlxlctl(b'X'), "swapmark"),
        (ctlxlctl(b'Z'), "shrinkwind"),
        (ctlx(b'?'), "deskey"),
        (ctlx(b'!'), "spawn"),
        (ctlx(b'@'), "pipecmd"),
        (ctlx(b'#'), "filter"),
        (ctlx(b'='), "showcpos"),
        (ctlx(b'('), "ctlxlp"),
        (ctlx(b')'), "ctlxrp"),
        (ctlx(b'^'), "enlargewind"),
        (ctlx(b'0'), "delwind"),
        (ctlx(b'1'), "onlywind"),
        (ctlx(b'2'), "splitwind"),
        (ctlx(b'A'), "setvar"),
        (ctlx(b'B'), "usebuffer"),
        (ctlx(b'C'), "spawncli"),
        (ctlx(b'E'), "ctlxe"),
        (ctlx(b'F'), "setfillcol"),
        (ctlx(b'K'), "killbuffer"),
        (ctlx(b'M'), "set_mode"),
        (ctlx(b'N'), "filename"),
        (ctlx(b'O'), "nextwind"),
        (ctlx(b'P'), "prevwind"),
        (ctlx(b'l'), "toggle_line_numbers"),
        (ctlx(b'L'), "set_line_number_width"),
        (ctlx(b'S'), "fisearch"),
        (ctlx(b'W'), "resize"),
        (ctlx(b'X'), "nextbuffer"),
        (ctlx(b'Y'), "md_conceal_toggle"),
        (ctlx(b'Z'), "enlargewind"),
        (ctlx(b'R'), "redo"),
        (ctlx(b'U'), "undo"),
        (metac_ctl(b'E'), "execproc"),
        (metac_ctl(b'C'), "wordcount"),
        (metac_ctl(b'F'), "getfence"),
        (metac_ctl(b'H'), "delbword"),
        (metac_ctl(b'K'), "unbindkey"),
        (metac_ctl(b'L'), "reposition"),
        (metac_ctl(b'M'), "delgmode"),
        (metac_ctl(b'N'), "namebuffer"),
        (metac_ctl(b'R'), "qreplace"),
        (metac_ctl(b'S'), "newsize"),
        (metac_ctl(b'T'), "newwidth"),
        (metac_ctl(b'V'), "scrnextup"),
        (metac_ctl(b'W'), "killpara"),
        (metac_ctl(b'Z'), "scrnextdw"),
        (meta(b' '), "setmark"),
        (meta(b'?'), "help"),
        (meta(b'!'), "reposition"),
        (meta(b'.'), "setmark"),
        (meta(b'>'), "gotoeob"),
        (meta(b'<'), "gotobob"),
        (meta(b'~'), "unmark"),
        (meta(b'A'), "apro"),
        (meta(b'B'), "backword"),
        (meta(b'C'), "capword"),
        (meta(b'D'), "delfword"),
        (meta(b'E'), "setcrkey"),
        (meta(b'F'), "forwword"),
        (meta(b'G'), "gotoline"),
        (meta(b'I'), "md_italic"),
        (meta(b'J'), "smart_indent"),
        (meta(b'K'), "bindtokey"),
        (meta(b'L'), "lowerword"),
        (meta(b'M'), "setgmode"),
        (meta(b'N'), "gotoeop"),
        (meta(b'O'), "md_outline"),
        (meta(b'P'), "gotobop"),
        (meta(b'Q'), "fillpara"),
        (meta(b'R'), "sreplace"),
        (meta(b'S'), "bktoshell"),
        (meta(b'U'), "upperword"),
        (meta(b'V'), "backpage"),
        (meta(b'W'), "copyregion"),
        (meta(b'X'), "namedcmd"),
        (meta(b'Y'), "md_preview"),
        (meta(b'Z'), "quickexit"),
        (meta(DEL), "delbword"),
        (spec(meta(b'A') as u8), "backline"),
        (spec(meta(b'B') as u8), "forwline"),
        (spec(meta(b'C') as u8), "forwchar"),
        (spec(meta(b'D') as u8), "backchar"),
        (spec(meta(b'P') as u8), "nextwind"),
        (DEL as Ckey, "backdel"),
        (spec(meta(b'W') as u8), "wrapword"),
        (spec(meta(b'T') as u8), "updmode"),
    ]
}

const EXTRA: &[&str] = &[
    "quit",
    "quickexit",
    "ctrlg",
    "automode",
    "auto-mode-for-file",
    "shell-escape",
    "sreplace",
    "qreplace",
    "setvar",
    "undo",
    "redo",
    "apro",
    "help",
    "deskey",
    "namedcmd",
    "listbuffers",
    "usebuffer",
    "killbuffer",
    "namebuffer",
    "nextbuffer",
    "splitwind",
    "onlywind",
    "delwind",
    "nextwind",
    "prevwind",
    "enlargewind",
    "shrinkwind",
    "mvdnwind",
    "mvupwind",
    "set_mode",
    "delmode",
    "setgmode",
    "delgmode",
    "updmode",
    "millisleep",
    "wheel-up",
    "wheel-down",
    "write-message",
    "insert-string",
    "istring",
    "resize",
    "newsize",
    "newwidth",
    "showcpos",
    "toggle_line_numbers",
    "set_line_number_width",
    "wrapword",
    "refreshscr",
    "reposition",
    "upscreen",
    "savewnd",
    "restwnd",
    "fisearch",
    "risearch",
    "exitrecedit",
    "noop",
    "ctlxlp",
    "ctlxrp",
    "ctlxe",
    "meta_pfx",
    "cex",
    "unarg",
    "storeproc",
    "set-fcolor",
    "set-bcolor",
    "toggle-color",
    "c_mode",
    "cindent",
    "cindent-region",
    "c-detect-style",
    "c-follow-source",
    "py_mode",
    "pyindent",
    "py-backspace",
    "py-format-region",
    "md_mode",
    "mdindent",
    "mdinsert",
    "md-backspace",
    "md-tab",
    "md-insert-heading",
    "md-heading-up",
    "md-heading-down",
    "md-heading-cycle",
    "md-list-toggle",
    "md-list-demote",
    "md-list-promote",
    "md-list-renumber",
    "md-task-toggle",
    "md-bold",
    "md-italic",
    "md-code",
    "md-strike",
    "md-insert-link",
    "md-quote-toggle",
    "md-quote-region",
    "md-insert-fence",
    "md-goto-fence",
    "md-table-next-cell",
    "md-outline",
    "md-goto-heading",
    "md_self_insert",
    "setcrkey",
    "set-strong-key",
    "crypt-off",
    "toggle-script-interactive",
    "bktoshell",
    "preview-close",
    "storemac",
    "execproc",
    "execbuf",
    "execfile",
    "execcmd",
    "run",
    "trim",
    "detab",
    "entab",
    "fillpara",
    "killpara",
    "wordcount",
    "smart_indent",
];

/// M-X 補完に渡す全コマンド名 (重複なし・ソート済み)。
pub fn all_command_names() -> Vec<String> {
    let mut names: Vec<String> = EXTRA
        .iter()
        .map(|s| (*s).to_string())
        .chain(default_bindings().into_iter().map(|(_, n)| n.to_string()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

pub fn known_command(name: &str) -> bool {
    EXTRA.contains(&name) || default_bindings().iter().any(|(_, n)| *n == name)
}

/// 動的なコマンド名を静的な文字列へ正規化する (キー再バインド用)。
/// 未知の名前はリークを避けるため "noop" にフォールバックする。
pub fn intern_command(name: &str) -> &'static str {
    for n in EXTRA {
        if *n == name {
            return n;
        }
    }
    if let Some((_, n)) = default_bindings().into_iter().find(|(_, n)| *n == name) {
        return n;
    }
    "noop"
}

pub fn dispatch(ed: &mut Editor, name: &str, f: bool, n: i32) -> Status {
    match name {
        // 移動
        "gotobol" => {
            ed.buffer_mut().dot.off = 0;
            St::True
        }
        "gotoeol" => {
            let li = ed.buffer().dot.line;
            let len = ed.buffer().line_len(li);
            ed.buffer_mut().dot.off = len;
            St::True
        }
        "forwchar" => move_chars(ed, n, true),
        "backchar" => move_chars(ed, n, false),
        "forwline" => vertical(ed, n),
        "backline" => vertical(ed, -n),
        "forwpage" => vertical(ed, n.saturating_mul((ed.window().rows as i32).max(1))),
        "backpage" => vertical(ed, n.saturating_mul(-(ed.window().rows as i32).max(1))),
        "wheel-up" | "wheel-down" => wheel_move(ed, name == "wheel-up", n),
        "gotobob" => {
            ed.buffer_mut().goto_bob();
            St::True
        }
        "gotoeob" => {
            ed.buffer_mut().goto_eob();
            St::True
        }
        "gotoline" => {
            let p = ed.prompt("Goto line: ", None);
            let ok = p
                .and_then(|s| s.trim().parse::<usize>().ok())
                .map(|v| ed.buffer_mut().goto_line(v))
                .unwrap_or(false);
            St::from_ok(ok)
        }
        "gotobop" | "gotoeop" => paragraph(ed, name == "gotoeop", n),
        "forwword" => repeat_flag(ed, n, |e| St::from_ok(e.buffer_mut().forward_word())),
        "backword" => repeat_flag(ed, n, |e| St::from_ok(e.buffer_mut().backward_word())),

        // 編集
        "newline" => crate::modes::newline_auto(ed, n),
        "indent" => newline_indent(ed, n, false),
        "smart_indent" => crate::modes::indent_auto(ed, n),
        "openline" => {
            let st = ed.buffer_mut().newline();
            ed.buffer_mut().backward_char();
            St::from_ok(st)
        }
        "tab" => crate::modes::tab_auto(ed, f, n),
        "forwdel" => delete_cmd(ed, f, n, false, false),
        "backdel" => delete_cmd(ed, f, n, true, false),
        "killtext" => kill_to_eol(ed, n),
        "twiddle" => transpose(ed),
        "quote" => {
            // ホイールは quoted-insert の文字にしない。
            let k = loop {
                let k = ed.io.read_key();
                if !crate::keycode::is_mouse(k) {
                    break k;
                }
            };
            let c = code(k);
            if c == 0 {
                St::False
            } else {
                St::from_ok(ed.buffer_mut().insert_repeat(n.max(1) as usize, c))
            }
        }
        "yank" => yank(ed, n),
        "clearkill" => {
            ed.kill.clear();
            St::True
        }
        "killregion" => kill_region(ed, true),
        "copyregion" => kill_region(ed, false),
        "delfword" => delete_word(ed, n, false),
        "delbword" => delete_word(ed, n, true),
        "upperword" | "lowerword" | "capword" => case_word(ed, name),
        "upperregion" | "lowerregion" => case_region(ed, name == "upperregion"),
        "deblank" => deblank(ed),
        "insspace" => {
            ed.buffer_mut().insert_repeat(n.max(1) as usize, b' ');
            for _ in 0..n.max(1) {
                ed.buffer_mut().backward_char();
            }
            St::True
        }
        "setmark" => {
            ed.buffer_mut().mark = Some(ed.buffer().dot);
            ed.mlwrite("[set mark]");
            St::True
        }
        "swapmark" => match ed.buffer().mark {
            None => {
                ed.mlwrite("No mark in this window");
                St::False
            }
            Some(m) => {
                let d = ed.buffer().dot;
                ed.buffer_mut().dot = m;
                ed.buffer_mut().mark = Some(d);
                St::True
            }
        },
        "unmark" => {
            ed.buffer_mut().mark = None;
            St::True
        }
        "trim" | "detab" | "entab" | "fillpara" | "killpara" | "wordcount" => {
            extra::simple_text_cmd(ed, name, n)
        }

        // 検索
        "forwsearch" | "backsearch" => extra::search_cmd(ed, name == "backsearch", f, n),
        "fisearch" | "risearch" => extra::isearch(ed, name == "risearch"),
        "sreplace" | "qreplace" => extra::replace_cmd(ed, name == "qreplace"),

        // ファイル / バッファ / ウィンドウ / その他
        "quit" => extra::quit(ed, f),
        "quickexit" => {
            if ed.restricted {
                ed.mlwrite("[Restricted]");
                return St::False;
            }
            extra::quickexit(ed, f, n)
        }
        "exitrecedit" => {
            if crate::system::preview_abort(ed) {
                St::True
            } else {
                ed.quit = true;
                St::True
            }
        }
        "ctrlg" => {
            ed.kbdmode = crate::editor::KbdMode::Stop;
            St::Abort
        }
        "meta_pfx" | "cex" | "unarg" | "noop" => St::True,
        "ctlxlp" => {
            if ed.kbdmode != crate::editor::KbdMode::Stop {
                ed.mlwrite("%Macro already active");
                return St::False;
            }
            ed.kbdm.clear();
            ed.kbdmode = crate::editor::KbdMode::Record;
            ed.mlwrite("[Start macro]");
            St::True
        }
        "ctlxrp" => {
            if ed.kbdmode != crate::editor::KbdMode::Record {
                ed.mlwrite("%Macro not active");
                return St::False;
            }
            ed.kbdmode = crate::editor::KbdMode::Stop;
            ed.mlwrite("[End macro]");
            St::True
        }
        "ctlxe" => crate::script::play_macro(ed, n),
        "wrapping" | "wrapword" => {
            ed.buffer_mut().mode ^= MDWRAP;
            St::True
        }
        "updmode" => {
            ed.window_mut().flag |= 0x10;
            St::True
        }
        "refreshscr" | "reposition" | "upscreen" => {
            ed.window_mut().flag = 0xff;
            ed.buffer_mut().dot = ed.buffer().dot;
            St::True
        }
        "resize" | "newsize" | "newwidth" => extra::resize(ed, name),
        "toggle_line_numbers" => {
            ed.show_line_numbers = !ed.show_line_numbers;
            ed.window_mut().flag = 0xff;
            St::True
        }
        "set_line_number_width" => {
            let p = ed.prompt(
                "Line number width: ",
                Some(&ed.line_number_width.to_string()),
            );
            if let Some(s) = p {
                if let Ok(v) = s.trim().parse::<usize>() {
                    ed.line_number_width = v.clamp(1, 8);
                }
            }
            ed.window_mut().flag = 0xff;
            St::True
        }
        "set-fcolor" => {
            let Some(v) = ed.prompt("Foreground color: ", Some(&ed.fg.as_var())) else {
                return St::False;
            };
            ed.fg = crate::editor::ColorSpec::parse(&v);
            ed.color_on = true;
            St::True
        }
        "set-bcolor" => {
            let Some(v) = ed.prompt("Background color: ", Some(&ed.bg.as_var())) else {
                return St::False;
            };
            ed.bg = crate::editor::ColorSpec::parse(&v);
            ed.color_on = true;
            St::True
        }
        "toggle-color" => {
            ed.color_on = !ed.color_on;
            St::True
        }
        "setcrkey" => {
            let Some(k) = ed.prompt_secret("Encryption String: ") else {
                return St::False;
            };
            ed.mlwrite("Classic cipher is obfuscation only, not secure encryption");
            ed.buffer_mut().set_key(k.as_bytes());
            St::True
        }
        "set-strong-key" => {
            let Some(pw) = ed.prompt_secret("Encryption password (AEAD): ") else {
                return St::False;
            };
            ed.buffer_mut().set_strong_key(pw.as_bytes());
            ed.mlwrite("XChaCha20-Poly1305 enabled (obfuscation, not a password manager)");
            St::True
        }
        "crypt-off" => {
            ed.buffer_mut().disable_crypt();
            ed.mlwrite("Encryption disabled");
            St::True
        }
        "toggle-script-interactive" => {
            ed.script_interactive = !ed.script_interactive;
            ed.mlwrite(format!("Script prompts: {}", ed.script_interactive));
            St::True
        }
        "undo" => St::from_ok(ed.buffer_mut().undo()),
        "redo" => St::from_ok(ed.buffer_mut().redo()),
        "set_mode" | "delmode" | "setgmode" | "delgmode" => extra::mode_cmd(ed, name),
        "setvar" => extra::setvar(ed),
        "millisleep" => St::True,
        _ => extra::dispatch(ed, name, f, n),
    }
}

fn repeat_flag<F>(ed: &mut Editor, n: i32, mut func: F) -> Status
where
    F: FnMut(&mut Editor) -> Status,
{
    let count = n.unsigned_abs().max(1);
    let mut st = St::True;
    for _ in 0..count {
        let r = func(ed);
        if r != St::True {
            st = r;
            break;
        }
        let forward = n >= 0;
        let _ = forward;
    }
    st
}

fn move_chars(ed: &mut Editor, n: i32, forward: bool) -> Status {
    let count = n.unsigned_abs().max(1);
    let go_forward = if forward { n >= 0 } else { n < 0 };
    let mut ok = true;
    for _ in 0..count {
        let r = if go_forward {
            ed.buffer_mut().forward_char()
        } else {
            ed.buffer_mut().backward_char()
        };
        if !r {
            ok = false;
            break;
        }
    }
    St::from_ok(ok)
}

fn display_col(ed: &Editor) -> usize {
    let b = ed.buffer();
    let l = &b.lines[b.dot.line];
    crate::display::column_of(&l[..b.dot.off.min(l.len())], ed.tabsize)
}

fn vertical(ed: &mut Editor, n: i32) -> Status {
    if ed.lastflag & CFCPCN == 0 {
        ed.curgoal = display_col(ed);
    }
    ed.thisflag |= CFCPCN;
    let goal = ed.curgoal;
    let tabsize = ed.tabsize;
    let b = ed.buffer_mut();
    let target = b.dot.line as i64 + n as i64;
    if target < 0 || target as usize >= b.lines.len() {
        return St::False;
    }
    b.dot.line = target as usize;
    let l = b.lines[b.dot.line].clone();
    let len = l.len();
    // バイト単位で走査するとマルチバイト文字の途中にドットが入ってしまう。
    // 文字境界だけを使い、表示桁 (タブ・全角を含む) で目的列を狙う。
    let mut off = 0usize;
    while off < len {
        let clen = b.char_len_at(b.dot.line, off).max(1);
        let next = (off + clen).min(len);
        let col = crate::display::column_of(&l[..next], tabsize);
        if col > goal {
            break;
        }
        off = next;
    }
    b.dot.off = off.min(len);
    St::True
}

/// マウスホイールによるカーソル移動。n は移動行数 (負=上)。
/// vertical() を通すので、連続ノッチでは目的列が保持される。
fn wheel_move(ed: &mut Editor, up: bool, n: i32) -> Status {
    let lines = n.unsigned_abs().max(1);
    vertical(ed, if up { -(lines as i32) } else { lines as i32 })
}

fn paragraph(ed: &mut Editor, forward: bool, n: i32) -> Status {
    let count = n.unsigned_abs().max(1) as i64;
    let dir = if forward { 1 } else { -1 };
    let b = ed.buffer_mut();
    let mut li = b.dot.line as i64;
    for _ in 0..count {
        loop {
            li += dir;
            if li <= 0 || li >= b.lines.len() as i64 - 1 {
                li = li.clamp(0, b.lines.len() as i64 - 1);
                break;
            }
            if b.lines[li as usize].iter().all(|c| c.is_ascii_whitespace()) {
                break;
            }
        }
    }
    b.dot = Pos::new(li.max(0) as usize, 0);
    St::True
}

fn newline_indent(ed: &mut Editor, n: i32, smart: bool) -> Status {
    if n <= 0 {
        return St::False;
    }
    for _ in 0..n {
        let prev = ed.buffer().dot.line;
        let indent = lead_indent(ed, prev, smart);
        if !ed.buffer_mut().newline() {
            return St::False;
        }
        if !indent.is_empty() {
            ed.buffer_mut().insert_bytes(&indent);
        }
    }
    St::True
}

/// C 版 `indent()` 相当: 前行のインデントをタブ/スペースで再利用。
fn lead_indent(ed: &Editor, line: usize, smart: bool) -> Vec<u8> {
    let b = ed.buffer();
    let Some(l) = b.lines.get(line) else {
        return Vec::new();
    };
    let mut count = 0usize;
    for &c in l {
        if c == b' ' {
            count += 1;
        } else if c == b'\t' {
            count = count.saturating_add(ed.tabsize - count % ed.tabsize.max(1));
        } else {
            break;
        }
    }
    let ts = ed.tabsize.max(1);
    if smart {
        let trimmed = String::from_utf8_lossy(l).trim_end().to_string();
        let mode = b.mode;
        let extra = if (mode & MDPYMOD != 0 && trimmed.ends_with(':'))
            || (mode & MDCMOD != 0 && trimmed.ends_with('{'))
            || (mode & MDMDMOD != 0 && trimmed.ends_with(':'))
        {
            ts
        } else if (mode & MDCMOD != 0 || mode & MDPYMOD != 0)
            && trimmed.trim_start().starts_with('}')
        {
            ts.saturating_mul(2)
        } else {
            0
        };
        count = count
            .saturating_add(extra)
            .saturating_sub(if trimmed.starts_with('}') { ts } else { 0 });
    }
    let mut out = Vec::new();
    if ed.tabsize > 0 {
        out.extend(std::iter::repeat_n(b'\t', count / ts));
    }
    out.extend(std::iter::repeat_n(b' ', count % ts.max(1)));
    out
}

fn delete_cmd(ed: &mut Editor, f: bool, n: i32, backward: bool, _killed: bool) -> Status {
    if n < 0 {
        return delete_cmd(ed, f, -n, !backward, _killed);
    }
    if f && ed.lastflag & CFKILL == 0 {
        ed.kill.clear();
    }
    let mut ok = true;
    if backward
        && n == 1
        && !f
        && ed.buffer().mode & (crate::buffer::MDPYMOD | crate::buffer::MDMDMOD) != 0
    {
        return crate::modes::backspace_auto(ed);
    }
    for _ in 0..n.max(1) {
        let r = if backward {
            ed.buffer_mut().back_delete(f)
        } else {
            ed.buffer_mut().delete(n.max(1) as usize, f)
        };
        let (good, killed) = r;
        if f {
            ed.kill.extend_from_slice(&killed);
            ed.thisflag |= CFKILL;
        }
        if !good {
            ok = false;
            break;
        }
        if !backward {
            break;
        }
    }
    St::from_ok(ok)
}

fn kill_append(ed: &mut Editor, data: &[u8]) {
    if ed.lastflag & CFKILL == 0 {
        ed.kill.clear();
    }
    ed.kill.extend_from_slice(data);
    ed.thisflag |= CFKILL;
}

fn kill_to_eol(ed: &mut Editor, n: i32) -> Status {
    if n < 0 {
        return St::False;
    }
    let mut killed = Vec::new();
    for _ in 0..n.max(1) {
        let li = ed.buffer().dot.line;
        let len = ed.buffer().line_len(li);
        let off = ed.buffer().dot.off;
        if off < len {
            let (ok, k) = ed.buffer_mut().delete(len - off, true);
            killed.extend(k);
            if !ok {
                break;
            }
        } else {
            let (_, k) = ed.buffer_mut().delete(1, true);
            killed.extend(k);
        }
    }
    kill_append(ed, &killed);
    St::True
}

fn yank(ed: &mut Editor, n: i32) -> Status {
    if n < 0 {
        return St::False;
    }
    let data = ed.kill.clone();
    if data.is_empty() {
        return St::True;
    }
    for _ in 0..n.max(1) {
        if !ed.buffer_mut().insert_bytes(&data) {
            return St::False;
        }
    }
    St::True
}

fn kill_region(ed: &mut Editor, delete: bool) -> Status {
    let Some(bytes) = ed.buffer().region_bytes() else {
        ed.mlwrite("No mark set in this buffer");
        return St::False;
    };
    kill_append(ed, &bytes);
    if delete {
        ed.buffer_mut().delete_region();
    }
    St::True
}

fn delete_word(ed: &mut Editor, n: i32, backward: bool) -> Status {
    let count = n.unsigned_abs() as usize;
    let dir = if backward { n < 0 } else { n >= 0 };
    let mut killed = Vec::new();
    for _ in 0..count {
        let start = ed.buffer().dot;
        let moved = if dir {
            ed.buffer_mut().backward_word()
        } else {
            ed.buffer_mut().forward_word()
        };
        if !moved {
            break;
        }
        let end = ed.buffer().dot;
        let (a, b) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        ed.buffer_mut().dot = b;
        let (ok, k) = ed
            .buffer_mut()
            .delete(b.off.saturating_sub(a.off).max(1), true);
        killed.extend(k);
        if !ok {
            break;
        }
    }
    kill_append(ed, &killed);
    St::True
}

fn transpose(ed: &mut Editor) -> Status {
    let b = ed.buffer_mut();
    let before = b.prev_char_len(b.dot.line, b.dot.off);
    if before == 0 {
        return St::False;
    }
    let p = b.dot;
    b.dot.off -= before;
    let first_len = b.char_len_at(b.dot.line, b.dot.off);
    let li = b.dot.line;
    let off = b.dot.off;
    let line = b.lines[li].clone();
    if off + first_len + before > line.len() {
        b.dot = p;
        return St::False;
    }
    let second = line[off + first_len..off + first_len + before].to_vec();
    b.lines[li].splice(off + first_len..off + first_len + before, []);
    b.lines[li].splice(off..off, second);
    b.dot = Pos::new(li, off + before + first_len);
    St::True
}

fn case_word(ed: &mut Editor, name: &str) -> Status {
    let start = ed.buffer().dot;
    if !ed.buffer_mut().forward_word() {
        return St::False;
    }
    let end = ed.buffer().dot;
    let b = ed.buffer_mut();
    let li = start.line;
    if li != end.line {
        b.dot = start;
        return St::False;
    }
    let s = String::from_utf8_lossy(&b.lines[li][start.off..end.off]).to_string();
    let new = match name {
        "upperword" => s.to_uppercase(),
        "lowerword" => s.to_lowercase(),
        "capword" => {
            let mut it = s.chars();
            match it.next() {
                Some(c) => c.to_uppercase().collect::<String>() + &it.as_str().to_lowercase(),
                None => s,
            }
        }
        _ => s,
    };
    b.lines[li].splice(start.off..end.off, new.bytes());
    b.dot = Pos::new(li, start.off + new.len());
    St::True
}

fn case_region(ed: &mut Editor, upper: bool) -> Status {
    let Some(data) = ed.buffer().region_bytes() else {
        return St::False;
    };
    let s = String::from_utf8_lossy(&data).to_string();
    let new = if upper {
        s.to_uppercase()
    } else {
        s.to_lowercase()
    };
    St::from_ok(ed.buffer_mut().replace_region(new.as_bytes()))
}

fn deblank(ed: &mut Editor) -> Status {
    let li = ed.buffer().dot.line;
    let mut start = li;
    while start > 0
        && ed.buffer().lines[start - 1]
            .iter()
            .all(|c| c.is_ascii_whitespace())
    {
        start -= 1;
    }
    let mut end = li;
    while end + 1 < ed.buffer().lines.len().saturating_sub(1)
        && ed.buffer().lines[end + 1]
            .iter()
            .all(|c| c.is_ascii_whitespace())
    {
        end += 1;
    }
    if end > start {
        ed.buffer_mut().dot = Pos::new(start + 1, 0);
        let _ = ed.buffer_mut().delete(end - start, false);
    }
    ed.buffer_mut().dot = Pos::new(start, 0);
    St::True
}

/// ユーザーがキーを 1 コマンド分だけ押すまで読む (C 版 getckey() 相当)。
pub fn read_key_command(ed: &mut Editor) -> Option<(Ckey, bool, i32)> {
    loop {
        let k = ed.read_key();
        if crate::keycode::is_mouse(k) {
            continue;
        }
        // ペースト/リサイズの擬似キーはキー割り当ての確認対象にしない。
        if k == crate::input::PASTE_KEY || k == crate::input::RESIZE_KEY {
            continue;
        }
        if k == crate::keycode::ctl(b'G') {
            return None;
        }
        if let Some(v) = ed.feed_key(k) {
            return Some(v);
        }
    }
}

pub fn set_curgoal_from(ed: &mut Editor) {
    ed.curgoal = display_col(ed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::EditorIo;
    use crate::keycode::{ctl, meta};

    struct Quiet;
    impl EditorIo for Quiet {
        fn prompt(&mut self, _: &str, _: Option<&str>) -> Option<String> {
            None
        }
        fn yesno(&mut self, _: &str) -> bool {
            false
        }
        fn read_key(&mut self) -> Ckey {
            0
        }
    }

    fn ed() -> Editor {
        Editor::new(Box::new(Quiet), 24, 80)
    }

    #[test]
    fn query_replace_caret_undoes_last_replacement() {
        let mut e = Editor::new(
            Box::new(crate::input::ScriptedIo::with_prompts(&[
                "a", "b", "y", "y", "^", "y", "q",
            ])),
            24,
            80,
        );
        e.buffer_mut().insert_bytes(b"a a a");
        e.buffer_mut().goto_bob();
        assert_eq!(e.run("qreplace"), St::True);
        assert_eq!(e.buffer().text(), b"b b a");
        assert_eq!(e.message, "Replaced 2 occurrence(s)");
    }

    #[test]
    fn named_command_completion_uses_candidate_list() {
        let names = all_command_names();
        assert!(names.contains(&"toggle-color".to_string()));
        assert!(names.contains(&"automode".to_string()));
        // 重複・順序が保証されている。
        assert!(names.windows(2).all(|w| w[0] <= w[1]));
        let mut e = Editor::new(
            Box::new(crate::input::ScriptedIo::with_prompts(&["toggle-color"])),
            24,
            80,
        );
        assert_eq!(e.run("namedcmd"), St::True);
        assert!(e.color_on);
    }

    #[test]
    fn auto_mode_for_file_registers_regexp() {
        let mut e = Editor::new(
            Box::new(crate::input::ScriptedIo::with_prompts(&[
                "CMODE",
                ".*\\.txt$",
            ])),
            24,
            80,
        );
        assert_eq!(e.run("automode"), St::True);
        assert_eq!(e.auto_mode_patterns.len(), 1);
        crate::modes::auto_mode(&mut e, 0, "notes.txt");
        assert_ne!(e.buffers[0].mode & crate::buffer::MDCMOD, 0);
        let mut e2 = Editor::new(Box::new(crate::input::ScriptedIo::new()), 24, 80);
        crate::modes::auto_mode(&mut e2, 0, "notes.txt");
        assert_eq!(e2.buffers[0].mode & crate::buffer::MDCMOD, 0);
    }

    #[test]
    fn caret_c_launches_shell_escape() {
        let e = ed();
        assert_eq!(e.keymap.get(&ctl(b'C')).copied(), Some("shell-escape"));
        assert!(known_command("exitrecedit"));
    }

    #[test]
    fn restricted_mode_blocks_file_and_shell_commands() {
        let mut e = ed();
        e.restricted = true;
        for name in [
            "filefind",
            "viewfile",
            "fileread",
            "insfile",
            "filesave",
            "filewrite",
            "spawn",
            "spawncli",
            "bktoshell",
            "shell-escape",
            "pipecmd",
            "filter",
            "quickexit",
            "execfile",
        ] {
            assert_eq!(e.run(name), St::False, "{} should be blocked", name);
        }
        assert_eq!(e.message, "[Restricted]");
        // 編集系コマンドはそのまま使える。
        e.buffer_mut().insert_bytes(b"x");
        assert_eq!(e.buffer().text(), b"x");
    }

    #[test]
    fn bindings_match_ebind() {
        let mut e = ed();
        assert_eq!(e.execute(ctl(b'A'), false, 1), St::True);
        assert_eq!(e.buffer().dot.off, 0);
        e.buffer_mut().insert_bytes(b"ab cd");
        e.execute(meta(b'B'), false, 1);
        assert_eq!(e.buffer().dot.off, 3);
        e.execute(ctl(b'F'), false, 1);
        assert_eq!(e.buffer().dot.off, 4);
    }

    #[test]
    fn kill_and_yank_roundtrip() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"hello world");
        e.buffer_mut().goto_bob();
        for _ in 0..5 {
            e.buffer_mut().forward_char();
        }
        e.run("killtext");
        assert_eq!(e.kill, b" world");
        e.buffer_mut().goto_eob();
        e.run("yank");
        assert_eq!(e.buffer().text(), b"hello world");
    }

    #[test]
    fn mark_region_copy_and_kill() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"abcdef");
        e.buffer_mut().goto_bob();
        e.run("setmark");
        for _ in 0..3 {
            e.buffer_mut().forward_char();
        }
        e.run("copyregion");
        assert_eq!(e.kill, b"abc");
        e.run("killregion");
        assert_eq!(e.buffer().text(), b"def");
    }

    #[test]
    fn universal_argument_flow() {
        let mut e = ed();
        assert_eq!(e.feed_key(ctl(b'U')), None);
        assert_eq!(e.feed_key(b'1' as Ckey), None);
        assert_eq!(e.feed_key(b'2' as Ckey), None);
        let (k, f, n) = e.feed_key(ctl(b'F')).unwrap();
        assert_eq!(k, ctl(b'F'));
        assert!(f);
        assert_eq!(n, 12);
    }

    #[test]
    fn vertical_keeps_char_boundary_on_japanese_lines() {
        let mut e = ed();
        e.buffer_mut()
            .insert_bytes("あいうえお\nかきくけこ\n".as_bytes());
        e.buffer_mut().goto_bob();
        for _ in 0..3 {
            e.buffer_mut().forward_char();
        }
        // 目的列は 6 (「え」の手前)。縦移動でも文字境界を維持する。
        assert_eq!(e.execute_named("forwline", false, 1), St::True);
        assert_eq!(e.buffer().dot.line, 1);
        assert_eq!(e.buffer().dot.off, 9);
    }

    #[test]
    fn wheel_command_maps_pseudo_keys_to_line_steps() {
        let mut e = ed();
        assert_eq!(
            crate::mouse::decode(crate::keycode::WHEELUP).map(|ev| ev.kind),
            Some(crate::mouse::MouseKind::WheelUp)
        );
        e.wheel_lines = 5;
        for i in 0..20 {
            e.buffer_mut()
                .insert_bytes(format!("line {:02}\n", i + 1).as_bytes());
        }
        e.buffer_mut().goto_bob();
        handle_mouse(&mut e, crate::mouse::MouseKind::WheelDown);
        assert_eq!(e.buffer().dot.line, 5);
    }

    fn handle_mouse(e: &mut Editor, kind: crate::mouse::MouseKind) {
        e.mouse_event(crate::mouse::MouseEvent {
            kind,
            row: 0,
            col: 0,
        });
    }

    #[test]
    fn wheel_moves_cursor_and_keeps_goal_column() {
        let mut e = ed();
        let mut text = Vec::new();
        for i in 0..20 {
            text.extend_from_slice(format!("line {:02} xxxxx\n", i + 1).as_bytes());
        }
        e.buffer_mut().insert_bytes(&text);
        e.buffer_mut().goto_bob();
        // 4 文字目へ進んでからホイールを回す。目的列は保持される。
        for _ in 0..4 {
            e.buffer_mut().forward_char();
        }
        let wl = e.wheel_lines as i32;
        assert_eq!(e.execute_named("wheel-down", false, wl), St::True);
        assert_eq!(e.buffer().dot.line, 3);
        assert_eq!(e.buffer().dot.off, 4);
        assert_eq!(e.execute_named("wheel-up", false, wl), St::True);
        assert_eq!(e.buffer().dot, crate::buffer::Pos::new(0, 4));
        // バッファ先頭では上に動けない。
        assert_eq!(e.execute_named("wheel-up", false, wl), St::False);
        assert_eq!(e.buffer().dot, crate::buffer::Pos::new(0, 4));
    }
}
