//! ファイル / バッファ / ウィンドウ / 検索 / その他のコマンド。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::{St, CFKILL};
use crate::buffer::{Buffer, Pos, BFCHG, BFINVS, MDEXACT, MDVIEW, MODE_CODES, MODE_NAMES};
use crate::editor::Editor;
use crate::keycode::Status;
use crate::keycode::{code, ctl, flags};
use crate::search::{self, Direction};

fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn path_string(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn read_file(ed: &mut Editor, bi: usize, path: &std::path::Path) -> bool {
    match fs::read(path) {
        Ok(bytes) => {
            // 強暗号ファイルは読み込み時にパスワードを尋ねる。
            if bytes.starts_with(crate::crypt::STRONG_MAGIC) && ed.buffers[bi].password.is_none() {
                let Some(pw) = ed.prompt_secret("File password: ") else {
                    return false;
                };
                ed.buffers[bi].set_strong_key(pw.as_bytes());
            }
            let code = crate::encoding::Code::detect(&bytes);
            let eol = crate::encoding::Eol::detect(&bytes);
            let b = &mut ed.buffers[bi];
            b.load_encoded(&bytes, code, eol);
            b.fname = Some(path_string(path));
            b.flags &= !BFINVS;
            if b.decrypt_failed {
                b.mode |= crate::buffer::MDVIEW;
                ed.mlwrite("Decryption failed - buffer opened read-only");
            }
            true
        }
        Err(e) => {
            ed.mlwrite(format!("{}: {}", path.display(), e));
            false
        }
    }
}

fn write_file(ed: &mut Editor, bi: usize, path: &std::path::Path) -> bool {
    let data = ed.buffers[bi].encoded_text();
    match fs::write(path, data) {
        Ok(()) => {
            let b = &mut ed.buffers[bi];
            b.fname = Some(path_string(path));
            b.flags &= !BFCHG;
            ed.mlwrite(format!("Wrote {}", path.display()));
            true
        }
        Err(e) => {
            ed.mlwrite(format!("{}: {}", path.display(), e));
            false
        }
    }
}

fn prompt_file(ed: &mut Editor, prompt: &str) -> Option<PathBuf> {
    let default = ed.buffer().fname.clone();
    ed.io
        .prompt(prompt, default.as_deref())
        .map(|s| expand(s.trim()))
}

pub fn quit(ed: &mut Editor, f: bool) -> Status {
    let dirty = ed
        .buffers
        .iter()
        .any(|b| b.flags & BFCHG != 0 && b.flags & BFINVS == 0);
    if f || !dirty || ed.yesno("Modified buffers exist. Leave anyway") {
        // 実行中のプレビューを止め、grip 等がポートを掴んだまま
        // 残留しないようにする (C 版 quit() の preview_close() 相当)。
        crate::system::preview_close(ed);
        ed.quit = true;
        St::True
    } else {
        St::False
    }
}

pub fn quickexit(ed: &mut Editor, _f: bool, _n: i32) -> Status {
    let dirty: Vec<usize> = ed
        .buffers
        .iter()
        .enumerate()
        .filter(|(_, b)| b.flags & BFCHG != 0 && b.flags & BFINVS == 0 && b.fname.is_some())
        .map(|(i, _)| i)
        .collect();
    for i in dirty {
        let path = match ed.buffers[i].fname.clone() {
            Some(p) => expand(&p),
            None => continue,
        };
        let _ = write_file(ed, i, &path);
    }
    quit(ed, true)
}

pub fn resize(ed: &mut Editor, name: &str) -> Status {
    let prompt = if name == "newwidth" {
        "New width: "
    } else {
        "New size: "
    };
    let ans = ed.prompt(prompt, None);
    let Some(s) = ans else { return St::False };
    let Ok(v) = s.trim().parse::<usize>() else {
        return St::False;
    };
    if name == "newwidth" {
        ed.screen_cols = v.max(20);
    } else {
        ed.screen_rows = v.max(3);
        for w in ed.windows.iter_mut() {
            w.rows = ed.screen_rows.saturating_sub(2).max(1);
        }
    }
    ed.window_mut().flag = 0xff;
    St::True
}

pub fn mode_cmd(ed: &mut Editor, name: &str) -> Status {
    let ans = ed.prompt("Mode: ", None);
    let Some(s) = ans else { return St::False };
    let up = s.trim().to_ascii_uppercase();
    let Some(&first) = up.as_bytes().first() else {
        ed.mlwrite("Unknown mode");
        return St::False;
    };
    let Some(i) = MODE_NAMES
        .iter()
        .position(|m| *m == up)
        .or_else(|| MODE_CODES.iter().position(|c| *c == first))
    else {
        ed.mlwrite("Unknown mode");
        return St::False;
    };
    let bit = 1u32 << i;
    if name == "set_mode" || name == "setgmode" {
        if name == "setgmode" {
            ed.global_mode |= bit;
        }
        for b in ed.buffers.iter_mut() {
            b.mode |= bit;
        }
    } else {
        if name == "delgmode" {
            ed.global_mode &= !bit;
        }
        for b in ed.buffers.iter_mut() {
            b.mode &= !bit;
        }
    }
    St::True
}

pub fn setvar(ed: &mut Editor) -> Status {
    let Some(name) = ed.prompt("Variable to set: ", None) else {
        return St::False;
    };
    let default = crate::script::get_bvar(ed, name.trim().trim_start_matches(['$', '%']));
    let Some(v) = ed.prompt("Value: ", default.as_deref()) else {
        return St::False;
    };
    St::from_ok(crate::script::set_bvar(
        ed,
        name.trim().trim_start_matches(['$', '%']),
        &v,
    ))
}

pub fn search_cmd(ed: &mut Editor, reverse: bool, _f: bool, n: i32) -> Status {
    let default = String::from_utf8_lossy(&ed.search_pattern).to_string();
    let Some(ans) = ed.prompt(
        if reverse {
            "Search backward: "
        } else {
            "Search forward: "
        },
        Some(&default),
    ) else {
        return St::False;
    };
    if ans.is_empty() {
        return St::False;
    }
    let pat = ans.into_bytes();
    ed.search_pattern = pat.clone();
    ed.search_reverse = reverse;
    let dir = if reverse {
        Direction::Reverse
    } else {
        Direction::Forward
    };
    let count = n.max(1) as usize;
    let exact = ed.buffer().mode & crate::buffer::MDEXACT != 0;
    let mut found = false;
    for _ in 0..count {
        let start = ed.buffer().dot;
        let b = ed.buffer();
        match search::find_mode(
            &b.lines,
            start,
            &pat,
            dir,
            exact,
            ed.buffer().mode & crate::buffer::MDMAGIC != 0,
        ) {
            Some((m, e)) => {
                ed.buffer_mut().mark = Some(m);
                // C 版と同じく後方検索は一致開始位置へ、前方検索は末尾へ移る。
                ed.buffer_mut().dot = if dir == Direction::Reverse { m } else { e };
                found = true;
            }
            None => break,
        }
    }
    if found {
        St::True
    } else {
        ed.mlwrite("Search failed");
        St::False
    }
}

pub fn isearch(ed: &mut Editor, reverse: bool) -> Status {
    let orig = ed.buffer().dot;
    let mut pat: Vec<u8> = Vec::new();
    let mut dir = if reverse {
        Direction::Reverse
    } else {
        Direction::Forward
    };
    let exact = ed.buffer().mode & MDEXACT != 0;
    let show = |e: &mut Editor, p: &[u8], d: Direction| {
        let arrow = if d == Direction::Forward {
            "C-S"
        } else {
            "C-R"
        };
        e.mlwrite(format!(
            "I-search {}: {}",
            arrow,
            String::from_utf8_lossy(p)
        ));
    };
    show(ed, &pat, dir);
    loop {
        let k = ed.io.read_key();
        let c = code(k);
        if k == ctl(b'G') {
            ed.buffer_mut().dot = orig;
            ed.mlwrite("Quit");
            return St::False;
        }
        if c == 8 || c == 127 {
            pat.pop();
        } else if k == ctl(b'S') || k == ctl(b'R') {
            let newdir = if k == ctl(b'S') {
                Direction::Forward
            } else {
                Direction::Reverse
            };
            if pat.is_empty() {
                dir = newdir;
            } else if newdir != dir {
                dir = newdir;
                let start = ed.buffer().dot;
                let b = ed.buffer();
                if let Some((m, _)) = search::find_mode(
                    &b.lines,
                    start,
                    &pat,
                    dir,
                    exact,
                    ed.buffer().mode & crate::buffer::MDMAGIC != 0,
                ) {
                    ed.buffer_mut().dot = m;
                }
            } else {
                let start = ed.buffer().dot;
                let b = ed.buffer();
                if let Some((m, _)) = search::find_mode(
                    &b.lines,
                    start,
                    &pat,
                    dir,
                    exact,
                    ed.buffer().mode & crate::buffer::MDMAGIC != 0,
                ) {
                    ed.buffer_mut().dot = m;
                }
            }
        } else if k == ctl(b'M') || c == b'\r' || c == b'\n' {
            break;
        } else if flags(k) == 0 && c >= 0x20 {
            pat.push(c);
            let start = ed.buffer().dot;
            let b = ed.buffer();
            if let Some((m, e)) = search::find_mode(
                &b.lines,
                start,
                &pat,
                dir,
                exact,
                ed.buffer().mode & crate::buffer::MDMAGIC != 0,
            ) {
                ed.buffer_mut().dot = e;
                ed.buffer_mut().mark = Some(m);
            }
        }
        show(ed, &pat, dir);
    }
    if !pat.is_empty() {
        ed.search_pattern = pat;
    }
    St::True
}

pub fn replace_cmd(ed: &mut Editor, query: bool) -> Status {
    let prev = String::from_utf8_lossy(&ed.search_pattern).to_string();
    let Some(from) = ed.prompt("Replace string: ", Some(&prev)) else {
        return St::False;
    };
    if from.is_empty() {
        return St::False;
    }
    let Some(to) = ed.prompt("with: ", None) else {
        return St::False;
    };
    let pat = from.into_bytes();
    let rep = to.into_bytes();
    ed.search_pattern = pat.clone();
    let exact = ed.buffer().mode & MDEXACT != 0;
    let mut count = 0usize;
    let mut replace_all = false;
    loop {
        let start = ed.buffer().dot;
        let b = ed.buffer();
        let Some((m, e)) = search::find_mode(
            &b.lines,
            start,
            &pat,
            Direction::Forward,
            exact,
            ed.buffer().mode & crate::buffer::MDMAGIC != 0,
        ) else {
            break;
        };
        if query && !replace_all {
            let ans = ed
                .prompt(
                    &format!("Replace '{}'? (y/n/!/q): ", String::from_utf8_lossy(&pat)),
                    None,
                )
                .unwrap_or_default();
            match ans.trim().chars().next() {
                Some('n') | Some('N') => {
                    ed.buffer_mut().dot = e;
                    continue;
                }
                Some('q') | Some('Q') => break,
                Some('!') => replace_all = true,
                _ => {}
            }
        }
        ed.buffer_mut().mark = Some(m);
        ed.buffer_mut().dot = e;
        ed.buffer_mut().replace_region(&rep);
        ed.buffer_mut().dot = Pos::new(m.line, m.off + rep.len());
        count += 1;
        if count > 10000 {
            break;
        }
    }
    ed.mlwrite(format!("Replaced {} occurrence(s)", count));
    St::True
}

pub fn simple_text_cmd(ed: &mut Editor, name: &str, n: i32) -> Status {
    let tabsize = ed.tabsize;
    let bi = ed.buffer_index();
    let b = &mut ed.buffers[bi];
    if b.mode & MDVIEW != 0 {
        return St::False;
    }
    match name {
        "trim" => {
            let li = b.dot.line.min(b.lines.len().saturating_sub(1));
            let line = &mut b.lines[li];
            while line.last() == Some(&b' ') || line.last() == Some(&b'\t') {
                line.pop();
            }
            St::True
        }
        "detab" => {
            let li = b.dot.line.min(b.lines.len().saturating_sub(1));
            let old = b.lines[li].clone();
            let mut out = Vec::new();
            for c in old {
                if c == b'\t' {
                    let pad = tabsize - out.len() % tabsize.max(1);
                    out.extend(std::iter::repeat_n(b' ', pad));
                } else {
                    out.push(c);
                }
            }
            b.lines[li] = out;
            St::True
        }
        "entab" => {
            let li = b.dot.line.min(b.lines.len().saturating_sub(1));
            let old = b.lines[li].clone();
            let mut out = Vec::new();
            let mut col = 0usize;
            let mut i = 0;
            while i < old.len() {
                if old[i] == b' ' {
                    let mut run = 0;
                    while i < old.len() && old[i] == b' ' {
                        i += 1;
                        run += 1;
                        col += 1;
                    }
                    if col.is_multiple_of(tabsize.max(1)) {
                        out.push(b'\t');
                    } else {
                        out.extend(std::iter::repeat_n(b' ', run));
                    }
                } else {
                    out.push(old[i]);
                    col += 1;
                    i += 1;
                }
            }
            b.lines[li] = out;
            St::True
        }
        "wordcount" => {
            let raw = b.text();
            let text = String::from_utf8_lossy(&raw);
            let words = text.split_whitespace().count();
            let lines = b.line_count();
            ed.mlwrite(format!("{} words, {} lines", words, lines));
            St::True
        }
        "fillpara" => St::True,
        "killpara" => {
            let start = b.dot.line;
            let mut end = start;
            while end + 1 < b.lines.len() && !b.lines[end].is_empty() {
                end += 1;
            }
            let removed = b.lines.drain(start..=end.min(b.lines.len() - 1)).count();
            if b.lines.is_empty() {
                b.lines.push(Vec::new());
            }
            b.dot = Pos::new(start.min(b.lines.len() - 1), 0);
            ed.kill.clear();
            ed.thisflag |= CFKILL;
            ed.mlwrite(format!("Killed {} lines", removed));
            St::True
        }
        _ => {
            let _ = n;
            St::False
        }
    }
}

fn buffer_menu(ed: &mut Editor, title: &str, lines: Vec<String>) {
    let bi = ed.find_buffer_or_create(title);
    ed.buffers[bi].lines = lines.into_iter().map(|s| s.into_bytes()).collect();
    if ed.buffers[bi].lines.is_empty() {
        ed.buffers[bi].lines.push(Vec::new());
    }
    ed.buffers[bi].dot = Pos::default();
    ed.buffers[bi].mark = None;
    ed.buffers[bi].mode |= MDVIEW;
    ed.set_window_buffer(bi);
}

pub fn dispatch(ed: &mut Editor, name: &str, f: bool, n: i32) -> Status {
    match name {
        "filefind" => {
            let Some(p) = prompt_file(ed, "Find file: ") else {
                return St::False;
            };
            let bname = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_string(&p));
            let bi = ed.find_buffer_or_create(&bname);
            // 存在しないファイルでもバッファ名とファイル名は設定する (C 版と同じ)
            ed.buffers[bi].fname = Some(path_string(&p));
            ed.buffers[bi].flags &= !BFINVS;
            if ed.buffers[bi].lines == vec![Vec::new()] && p.exists() {
                let _ = read_file(ed, bi, &p);
            }
            crate::modes::auto_mode(ed, bi, &path_string(&p));
            ed.set_window_buffer(bi);
            St::True
        }
        "viewfile" => {
            let Some(p) = prompt_file(ed, "View file: ") else {
                return St::False;
            };
            let bname = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_string(&p));
            let bi = ed.find_buffer_or_create(&bname);
            let _ = read_file(ed, bi, &p);
            ed.buffers[bi].mode |= MDVIEW;
            ed.set_window_buffer(bi);
            St::True
        }
        "fileread" => {
            let Some(p) = prompt_file(ed, "Read file: ") else {
                return St::False;
            };
            let bi = ed.buffer_index();
            St::from_ok(read_file(ed, bi, &p))
        }
        "insfile" => {
            let Some(p) = prompt_file(ed, "Insert file: ") else {
                return St::False;
            };
            match fs::read(&p) {
                Ok(data) => {
                    let code = crate::encoding::Code::detect(&data);
                    let eol = crate::encoding::Eol::detect(&data);
                    let text = crate::encoding::to_utf8(&eol.decode(&data), code);
                    St::from_ok(ed.buffer_mut().insert_bytes(&text))
                }
                Err(e) => {
                    ed.mlwrite(format!("{}: {}", p.display(), e));
                    St::False
                }
            }
        }
        "filesave" => {
            let path = ed.buffer().fname.clone().map(|s| expand(&s));
            match path {
                Some(p) => St::from_ok(write_file(ed, ed.buffer_index(), &p)),
                None => dispatch(ed, "filewrite", f, n),
            }
        }
        "filewrite" => {
            let Some(p) = prompt_file(ed, "Write file: ") else {
                return St::False;
            };
            let bi = ed.buffer_index();
            let ok = write_file(ed, bi, &p);
            if ok {
                ed.buffers[bi].mode &= !MDVIEW;
            }
            St::from_ok(ok)
        }
        "filename" => {
            let Some(p) = prompt_file(ed, "File name: ") else {
                return St::False;
            };
            ed.buffer_mut().fname = Some(path_string(&p));
            St::True
        }
        "listbuffers" => {
            let lines = ed
                .buffers
                .iter()
                .map(|b| {
                    format!(
                        "{:2} {:16} {} {:>5} lines",
                        if b.nwnd > 0 { "*" } else { " " },
                        b.name,
                        if b.flags & BFCHG != 0 { "*" } else { " " },
                        b.line_count()
                    )
                })
                .collect();
            buffer_menu(ed, "Buffer List", lines);
            St::True
        }
        "usebuffer" => {
            let Some(ans) = ed.prompt("Use buffer: ", None) else {
                return St::False;
            };
            let name = ans.trim();
            if let Some(i) = ed.find_buffer(name) {
                ed.set_window_buffer(i);
                St::True
            } else {
                let i = ed.find_buffer_or_create(name);
                ed.set_window_buffer(i);
                St::True
            }
        }
        "nextbuffer" => {
            let cur = ed.buffer_index();
            ed.set_window_buffer((cur + 1) % ed.buffers.len());
            St::True
        }
        "namebuffer" => {
            let old = ed.buffer().name.clone();
            let Some(ans) = ed.prompt("Name buffer: ", Some(&old)) else {
                return St::False;
            };
            if ed.find_buffer(ans.trim()).is_some() && ans.trim() != old {
                ed.mlwrite("Buffer name in use");
                return St::False;
            }
            ed.buffer_mut().name = ans.trim().to_string();
            St::True
        }
        "killbuffer" => {
            let cur = ed.buffer().name.clone();
            let Some(ans) = ed.prompt("Kill buffer: ", Some(&cur)) else {
                return St::False;
            };
            let Some(i) = ed.find_buffer(ans.trim()) else {
                ed.mlwrite("No such buffer");
                return St::False;
            };
            if ed.buffers[i].flags & BFCHG != 0 && !ed.yesno("Buffer modified. Kill anyway") {
                return St::False;
            }
            if ed.buffers.len() == 1 {
                ed.buffers[i] = Buffer::new("main");
                return St::True;
            }
            ed.buffers.remove(i);
            for w in ed.windows.iter_mut() {
                if w.buf >= i && w.buf > 0 {
                    w.buf -= 1;
                }
                if w.buf >= ed.buffers.len() {
                    w.buf = 0;
                }
            }
            St::True
        }
        "nextwind" => {
            let next = (ed.cur_win + 1) % ed.windows.len();
            ed.set_cur_win(next);
            St::True
        }
        "prevwind" => {
            let next = (ed.cur_win + ed.windows.len() - 1) % ed.windows.len();
            ed.set_cur_win(next);
            St::True
        }
        "onlywind" => {
            let buf = ed.buffer_index();
            ed.windows.truncate(1);
            ed.cur_win = 0;
            ed.windows[0] = crate::editor::Window {
                buf,
                top: ed.windows[0].top,
                rows: ed.screen_rows - 2,
                flag: 0xff,
                force: 0,
            };
            St::True
        }
        "splitwind" => {
            if ed.windows.len() >= 8 {
                ed.mlwrite("Too many windows");
                return St::False;
            }
            let buf = ed.buffer_index();
            let half = (ed.window().rows / 2).max(1);
            ed.windows[ed.cur_win].rows = half;
            let mut w = crate::editor::Window {
                buf,
                top: ed.windows[ed.cur_win].top,
                rows: ed.screen_rows.saturating_sub(2).saturating_sub(half).max(1),
                flag: 0xff,
                force: 0,
            };
            w.top = ed.windows[ed.cur_win].top + half;
            ed.windows.insert(ed.cur_win + 1, w);
            ed.window_pos.insert(ed.cur_win + 1, HashMap::new());
            St::True
        }
        "delwind" => {
            if ed.windows.len() == 1 {
                ed.mlwrite("Attempt to delete sole ordinary window");
                return St::False;
            }
            ed.windows.remove(ed.cur_win);
            ed.window_pos.remove(ed.cur_win);
            if ed.cur_win >= ed.windows.len() {
                ed.cur_win = ed.windows.len() - 1;
            }
            let rows = ed.screen_rows.saturating_sub(2) / ed.windows.len().max(1);
            for w in ed.windows.iter_mut() {
                w.rows = rows.max(1);
            }
            St::True
        }
        "mvdnwind" => {
            if ed.cur_win + 1 < ed.windows.len() {
                ed.windows.swap(ed.cur_win, ed.cur_win + 1);
                ed.window_pos.swap(ed.cur_win, ed.cur_win + 1);
                ed.cur_win += 1;
            }
            St::True
        }
        "mvupwind" => {
            if ed.cur_win > 0 {
                ed.windows.swap(ed.cur_win, ed.cur_win - 1);
                ed.window_pos.swap(ed.cur_win, ed.cur_win - 1);
                ed.cur_win -= 1;
            }
            St::True
        }
        "enlargewind" => {
            let idx = ed.cur_win;
            if idx + 1 < ed.windows.len() {
                let grow = n.unsigned_abs() as usize;
                let take = grow.min(ed.windows[idx + 1].rows.saturating_sub(1));
                ed.windows[idx].rows += take;
                ed.windows[idx + 1].rows -= take;
            }
            St::True
        }
        "shrinkwind" => {
            let idx = ed.cur_win;
            if idx + 1 < ed.windows.len() {
                let take = (n.unsigned_abs() as usize)
                    .max(1)
                    .min(ed.windows[idx].rows.saturating_sub(1));
                ed.windows[idx].rows -= take;
                ed.windows[idx + 1].rows += take;
            }
            St::True
        }
        "scrnextup" | "scrnextdw" => {
            let delta = if name == "scrnextup" { -1 } else { 1 };
            let idx = (ed.cur_win + 1) % ed.windows.len();
            let bi = ed.windows[idx].buf;
            ed.buffers[bi].dot.line = (ed.buffers[bi].dot.line as i64 + delta).max(0) as usize;
            ed.windows[idx].top = ed.buffers[bi].dot.line;
            St::True
        }
        "savewnd" | "restwnd" => St::True,
        "showcpos" => {
            let b = ed.buffer();
            let li = b.dot.line;
            let len = b.line_count();
            let col = b.dot.off + 1;
            let pct = li.saturating_mul(100).checked_div(len).unwrap_or(0);
            ed.mlwrite(format!(
                "line {}  char {}  column {}  {}%",
                li + 1,
                li + 1,
                col,
                pct
            ));
            St::True
        }
        "help" => {
            let mut lines = vec!["kemacs (Rust port) help -- C-X C-C: exit".to_string()];
            let mut keys: Vec<(String, &str)> = ed
                .keymap
                .iter()
                .map(|(k, n)| (crate::keycode::cmdstr(*k), *n))
                .collect();
            keys.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, n) in keys {
                lines.push(format!("{:16} {}", k, n));
            }
            buffer_menu(ed, "Help", lines);
            St::True
        }
        "deskey" => {
            if let Some((k, _, _)) = super::read_key_command(ed) {
                let name = ed.keymap.get(&k).copied().unwrap_or("Not Bound");
                ed.mlwrite(format!("{} runs {}", crate::keycode::cmdstr(k), name));
            }
            St::True
        }
        "apro" => {
            let Some(ans) = ed.prompt("Apropos: ", None) else {
                return St::False;
            };
            let mut names: Vec<&str> = ed.keymap.values().copied().collect();
            names.sort_unstable();
            names.dedup();
            let lines: Vec<String> = names
                .into_iter()
                .filter(|n| n.contains(ans.trim()))
                .map(|n| n.to_string())
                .collect();
            buffer_menu(ed, "Apropos", lines);
            St::True
        }
        "namedcmd" => {
            let Some(ans) = ed.prompt("M-x ", None) else {
                return St::False;
            };
            let name = ans.trim();
            if ed.keymap.values().any(|n| *n == name) || crate::commands::known_command(name) {
                ed.execute_named(name, false, 1)
            } else {
                ed.mlwrite(format!("[No such command: {}]", name));
                St::False
            }
        }
        "bindtokey" => {
            let Some(cmd) = ed.prompt("Bind command: ", None) else {
                return St::False;
            };
            let cmd = cmd.trim();
            if !crate::commands::known_command(cmd) {
                ed.mlwrite("[No such function]");
                return St::False;
            }
            let Some((k, _, _)) = super::read_key_command(ed) else {
                return St::False;
            };
            ed.keymap.insert(k, crate::commands::intern_command(cmd));
            ed.mlwrite(format!("{} bound to {}", crate::keycode::cmdstr(k), cmd));
            St::True
        }
        "unbindkey" => {
            let Some((k, _, _)) = super::read_key_command(ed) else {
                return St::False;
            };
            match ed.keymap.remove(&k) {
                Some(_) => {
                    ed.mlwrite(format!("{} unbound", crate::keycode::cmdstr(k)));
                    St::True
                }
                None => {
                    ed.mlwrite("Not Bound");
                    St::False
                }
            }
        }
        "storeproc" => crate::script::storeproc(ed, f, n),
        "storemac" => crate::script::storemac(ed, n),
        "execproc" | "run" => crate::script::execproc(ed, n),
        "execbuf" => crate::script::execbuf(ed),
        "execfile" => {
            let Some(name) = ed.prompt("File to execute: ", None) else {
                return St::False;
            };
            let Ok(data) = std::fs::read(name.trim()) else {
                ed.mlwrite("Cannot read file");
                return St::False;
            };
            let text = String::from_utf8_lossy(&data).into_owned();
            let lines: Vec<Vec<u8>> = text.split('\n').map(|s| s.as_bytes().to_vec()).collect();
            let bi = ed.find_buffer_or_create("*exec*");
            ed.buffers[bi].lines = lines;
            crate::script::dobuf(ed, "*exec*")
        }
        "execcmd" => {
            let Some(line) = ed.prompt(": ", None) else {
                return St::False;
            };
            crate::script::docmd(ed, &line)
        }
        "c_mode" => crate::modes::c_mode(ed, f, n),
        "cindent" => crate::modes::c_indent(ed, n.max(1)),
        "cindent-region" | "c-indent-region" => crate::modes::c_indent_region(ed),
        "c_detect_style" | "c-detect-style" => crate::modes::c_detect_style(ed, n),
        "c_follow_source" | "c-follow-source" => crate::modes::c_follow_source(ed),
        "py_mode" => crate::modes::py_mode(ed, f, n),
        "pyindent" => crate::modes::py_indent(ed, n.max(1)),
        "py_backspace" | "py-backspace" => crate::modes::py_backspace(ed),
        "py_indent_region" | "py-format-region" => crate::modes::py_indent_region(ed),
        "md_mode" | "md_mode_toggle" => crate::modes::md_mode(ed, f, n),
        "mdindent" => crate::modes::md_indent(ed, n.max(1)),
        "md_backspace" => crate::modes::md_backspace(ed),
        "md_tab" => crate::modes::md_tab(ed),
        "md_insert_heading" | "md-insert-heading" => crate::modes::md_insert_heading(ed, f, n),
        "md_heading_up" => crate::modes::md_heading_up(ed, n),
        "md_heading_down" => crate::modes::md_heading_down(ed, n),
        "md_heading_cycle" => crate::modes::md_heading_cycle(ed),
        "md_list_toggle" => crate::modes::md_list_toggle(ed),
        "md_list_demote" => crate::modes::md_list_demote(ed),
        "md_list_promote" => crate::modes::md_list_promote(ed),
        "md_list_renumber" => crate::modes::md_list_renumber(ed),
        "md_task_toggle" => crate::modes::md_task_toggle(ed),
        "md_bold" => crate::modes::md_bold(ed),
        "md_italic" => crate::modes::md_italic(ed),
        "md_code" => crate::modes::md_code(ed),
        "md_strike" => crate::modes::md_strike(ed),
        "md_insert_link" => crate::modes::md_insert_link(ed),
        "md_quote_toggle" => crate::modes::md_quote_toggle(ed),
        "md_quote_region" => crate::modes::md_quote_region(ed),
        "md_insert_fence" => crate::modes::md_insert_fence(ed),
        "md_goto_fence" => crate::modes::md_goto_fence(ed),
        "md_table_next_cell" => crate::modes::md_table_next_cell(ed),
        "md_outline" => crate::modes::md_outline(ed),
        "md_goto_heading" => crate::modes::md_goto_heading(ed),
        "md_conceal_toggle" => crate::modes::md_conceal_toggle(ed),
        "getfence" => St::False,
        "spawn" => crate::system::spawn(ed),
        "spawncli" => crate::system::spawncli(ed),
        "bktoshell" => crate::system::bktoshell(ed),
        "pipecmd" => crate::system::pipecmd(ed),
        "filter" => crate::system::filter(ed),
        "shell_escape_cmd" => {
            let Some(s) = ed.prompt("Escape string: ", None) else {
                return St::False;
            };
            ed.mlwrite(crate::system::shell_escape(&s));
            St::True
        }
        "md_preview" => {
            let template = crate::script::get_bvar(ed, "md-preview-command")
                .filter(|s| !s.is_empty())
                // 既定で grip を使う。%f は現在のバッファのファイル名へ展開する。
                .unwrap_or_else(|| "grip %f".to_string());
            let cmd = if template.contains("%f") {
                let Some(fname) = ed.buffer().fname.as_deref() else {
                    ed.mlwrite("Preview needs a file name (save the buffer first)");
                    return St::False;
                };
                template.replace("%f", &crate::system::shell_escape(fname))
            } else {
                template
            };
            let bname = "*Preview*".to_string();
            let st = crate::system::preview_start(ed, &cmd, &bname);
            if st == St::True {
                crate::system::preview_wait_first(ed);
            }
            st
        }
        "preview-close" => {
            crate::system::preview_close(ed);
            St::True
        }
        "insert-string" | "istring" => {
            let Some(ans) = ed.prompt("String: ", None) else {
                return St::False;
            };
            St::from_ok(ed.buffer_mut().insert_bytes(ans.as_bytes()))
        }
        "write-message" => {
            let Some(ans) = ed.prompt("Message: ", None) else {
                return St::False;
            };
            ed.mlwrite(ans);
            St::True
        }
        _ if name.starts_with("cbuf") && name[4..].parse::<i32>().is_ok() => {
            crate::script::cbuf(ed, name[4..].parse::<i32>().unwrap())
        }
        _ => {
            ed.mlwrite(format!("[Not ported: {}]", name));
            St::False
        }
    }
}
