//! OS 連携: 自動保存、シェルコマンド、非同期プレビュー。
//! C 版 `main.C_` (ASAVE), `spawn.C_`, `fileio.C_` の一部に相当。

use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::buffer::{BFCHG, MDASAVE, MDVIEW};
use crate::editor::{Editor, Status};

#[derive(Debug)]
pub struct PreviewState {
    pub child: Option<Child>,
    /// 子プロセスの stdout パイプ (O_NONBLOCK)。
    pub fd: i32,
    pub bufname: String,
    pub prev_buf: usize,
    /// 改行途中の部分行。
    pub pending: Vec<u8>,
    pub finished: bool,
}

/// ファイル名から自動保存用の `#basename#` パスを作る。
pub fn autosave_path(fname: &str) -> PathBuf {
    let p = PathBuf::from(fname);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| fname.to_string());
    let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
    parent.join(format!("#{}#", name))
}

/// C 版の ASAVE 相当。変更済みバッファを `#file#` に保存する (BFCHG は残す)。
pub fn autosave_current(ed: &mut Editor) -> Status {
    let bi = ed.buffer_index();
    let (fname, changed, mode) = {
        let b = &ed.buffers[bi];
        (b.fname.clone(), b.flags & BFCHG != 0, b.mode)
    };
    if !changed || mode & MDASAVE == 0 {
        return Status::False;
    }
    let Some(fname) = fname else {
        return Status::False;
    };
    let data = ed.buffers[bi].encoded_text();
    match fs::write(autosave_path(&fname), data) {
        Ok(()) => {
            ed.mlwrite("[Auto-save]");
            Status::True
        }
        Err(e) => {
            ed.mlwrite(format!("[Auto-save failed: {}]", e));
            Status::False
        }
    }
}

/// POSIX シェル引数として安全にクォートする (C 版 shell_command_escape_char 相当)。
pub fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn run_capture(cmd: &str, input: Option<&[u8]>) -> std::io::Result<(Vec<u8>, i32)> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let (Some(data), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(data)?;
    }
    let out = child.wait_with_output()?;
    let mut bytes = out.stdout;
    bytes.extend_from_slice(&out.stderr);
    Ok((bytes, out.status.code().unwrap_or(-1)))
}

/// C-X ! : 一行コマンドを実行する。
pub fn spawn(ed: &mut Editor) -> Status {
    let Some(cmd) = ed.prompt("!", None) else {
        return Status::False;
    };
    ed.suspend_terminal();
    let result = Command::new("sh").arg("-c").arg(&cmd).status();
    ed.resume_terminal();
    match result {
        Ok(st) if st.success() => {
            ed.mlwrite("[End]");
            Status::True
        }
        Ok(st) => {
            ed.mlwrite(format!("[Exit {}]", st.code().unwrap_or(-1)));
            Status::False
        }
        Err(e) => {
            ed.mlwrite(format!("{}", e));
            Status::False
        }
    }
}

/// C-X C : 対話的シェルを起動する。
pub fn spawncli(ed: &mut Editor) -> Status {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    ed.suspend_terminal();
    let result = Command::new(&shell).status();
    ed.resume_terminal();
    match result {
        Ok(_) => {
            ed.mlwrite("[End]");
            Status::True
        }
        Err(e) => {
            ed.mlwrite(format!("{}", e));
            Status::False
        }
    }
}

/// C-X @ : コマンド出力を別ウィンドウのバッファへ入れる。
pub fn pipecmd(ed: &mut Editor) -> Status {
    let Some(cmd) = ed.prompt("@", None) else {
        return Status::False;
    };
    let (bytes, _) = match run_capture(&cmd, None) {
        Ok(v) => v,
        Err(e) => {
            ed.mlwrite(format!("{}", e));
            return Status::False;
        }
    };
    if ed.execute_named("splitwind", false, 1) != Status::True {
        return Status::False;
    }
    let bi = ed.find_buffer_or_create("[command]");
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let lines: Vec<Vec<u8>> = text.split('\n').map(|s| s.as_bytes().to_vec()).collect();
    ed.buffers[bi].lines = lines;
    ed.buffers[bi].mode |= MDVIEW;
    ed.buffers[bi].fname = None;
    ed.set_window_buffer(bi);
    Status::True
}

/// C-X # : 現在のバッファを外部コマンドでフィルタする。
pub fn filter(ed: &mut Editor) -> Status {
    let Some(cmd) = ed.prompt("#", None) else {
        return Status::False;
    };
    if ed.buffer().mode & MDVIEW != 0 {
        return Status::False;
    }
    // 一時ファイルを使わず、バッファ内容を stdin、結果を stdout として
    // パイプで渡す。予測可能なパスへのシンボリックリンク攻撃を避ける。
    let data = ed.buffer().encoded_text();
    let (code, eol) = (ed.buffer().code, ed.buffer().eol);
    let ok = match run_capture(&cmd, Some(&data)) {
        Ok((out, 0)) => {
            let bi = ed.buffer_index();
            ed.buffers[bi].load_encoded(&out, code, eol);
            ed.buffers[bi].flags |= BFCHG;
            true
        }
        _ => false,
    };
    if ok {
        Status::True
    } else {
        ed.mlwrite("[Filter failed]");
        Status::False
    }
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
    fn read(fd: i32, buf: *mut std::ffi::c_void, count: usize) -> isize;
}

const F_GETFL: i32 = 3;
const F_SETFL: i32 = 4;
const O_NONBLOCK: i32 = 0x800;
const EAGAIN: i32 = 11;

fn set_nonblock(fd: i32) {
    unsafe {
        let flags = fcntl(fd, F_GETFL);
        if flags >= 0 {
            fcntl(fd, F_SETFL, flags | O_NONBLOCK);
        }
    }
}

/// パイプから読めるだけ読み出す (EAGAIN で打ち切る)。
fn read_available(fd: i32) -> Vec<u8> {
    let mut buf = [0u8; 8192];
    let mut out = Vec::new();
    loop {
        let n = unsafe { read(fd, buf.as_mut_ptr() as *mut std::ffi::c_void, buf.len()) };
        if n > 0 {
            out.extend_from_slice(&buf[..n as usize]);
        } else {
            if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(EAGAIN) {
                break;
            }
            break;
        }
    }
    out
}

/// M-S / C-X D : 端末を元に戻して SIGTSTP でサスペンドする。
pub fn bktoshell(ed: &mut Editor) -> Status {
    ed.suspend_terminal();
    unsafe {
        kill(0, 20);
    } // SIGTSTP (Linux)
    ed.resume_terminal();
    ed.window_mut().flag = 0xff;
    Status::True
}

/// 非同期プレビューを開始する (C 版 spawn_preview() 相当)。
/// stdout/stderr を同じ pipe に繋ぎ、C 版と同様 nonblock read でポールする。
pub fn preview_start(ed: &mut Editor, cmd: &str, bname: &str) -> Status {
    if cmd.trim().is_empty() {
        return Status::False;
    }
    preview_close(ed);
    let full = format!("exec 2>&1; {}", cmd);
    let child = match Command::new("sh")
        .arg("-c")
        .arg(&full)
        // 専用プロセスグループを作り、停止時に grip 等の孫プロセスまで
        // kill できるようにする (C 版 spawn_preview() の setsid() 相当)。
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            ed.mlwrite(format!("{}", e));
            return Status::False;
        }
    };
    let fd = match child.stdout.as_ref() {
        Some(out) => out.as_raw_fd(),
        None => return Status::False,
    };
    set_nonblock(fd);
    let prev_buf = ed.buffer_index();
    let bi = ed.find_buffer_or_create(bname);
    ed.buffers[bi].lines = vec![Vec::new()];
    ed.buffers[bi].mode |= MDVIEW;
    ed.buffers[bi].fname = None;
    ed.set_window_buffer(bi);
    ed.preview = Some(PreviewState {
        child: Some(child),
        fd,
        bufname: bname.to_string(),
        prev_buf,
        pending: Vec::new(),
        finished: false,
    });
    Status::True
}

/// 出力をポールしてプレビューバッファへ反映する (C 版 preview_poll() 相当)。
pub fn preview_poll(ed: &mut Editor) {
    let mut lines: Vec<Vec<u8>> = Vec::new();
    let name;
    {
        let Some(state) = ed.preview.as_mut() else {
            return;
        };
        state.pending.extend_from_slice(&read_available(state.fd));
        while let Some(pos) = state.pending.iter().position(|&c| c == b'\n') {
            let mut line: Vec<u8> = state.pending.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(line);
        }
        let exited = match state.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_)) | Err(_)),
            None => true,
        };
        if exited && !state.finished {
            state.finished = true;
            if !state.pending.is_empty() {
                lines.push(std::mem::take(&mut state.pending));
            }
            lines.push(b"[preview finished]".to_vec());
        }
        name = state.bufname.clone();
    }
    if !lines.is_empty() {
        if let Some(bi) = ed.find_buffer(&name) {
            let base = ed.buffers[bi].lines.len().saturating_sub(1);
            for (offset, line) in lines.into_iter().enumerate() {
                ed.buffers[bi].lines.insert(base + offset, line);
            }
        }
    }
}

/// 起動直後の出力を最大 ~300ms ポールし、M-Y が戻った時点で
/// `*Preview*` バッファに grip のバナー/URL が表示されるようにする
/// (C 版 preview_wait_first() 相当)。これがないと、編集ループが
/// read_key() でブロックしている間は出力が回収されず、何かキーを
/// 押すまでプレビューが空のままになる。
pub fn preview_wait_first(ed: &mut Editor) {
    let start = Instant::now();
    loop {
        preview_poll(ed);
        let stop = {
            let Some(p) = ed.preview.as_ref() else {
                break;
            };
            if p.finished {
                true
            } else {
                ed.find_buffer(&p.bufname)
                    .is_some_and(|bi| ed.buffers[bi].lines.len() > 1)
            }
        };
        if stop || start.elapsed() >= Duration::from_millis(300) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn preview_close(ed: &mut Editor) {
    let Some(mut state) = ed.preview.take() else {
        return;
    };
    if let Some(mut child) = state.child.take() {
        let pid = child.id() as i32;
        // シェルだけ kill すると grip 等が孤立し、ポート 6419 を掴んだまま
        // 残る。次回 M-Y で "Address already in use" となり即終了するため、
        // プロセスグループ全体を終了させる (C 版 preview_kill_process() 相当)。
        unsafe {
            kill(-pid, 15); // SIGTERM
        }
        for _ in 0..50 {
            if !matches!(child.try_wait(), Ok(None)) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        // シェルが先に終了しても grip が残っていることがある。
        if unsafe { kill(-pid, 0) } == 0 {
            unsafe {
                kill(-pid, 9); // SIGKILL
            }
        }
        let _ = child.wait();
    }
    if let Some(bi) = ed.find_buffer(&state.bufname) {
        for w in ed.windows.iter_mut() {
            if w.buf == bi {
                w.buf = state.prev_buf.min(ed.buffers.len().saturating_sub(1));
            }
        }
    }
}

pub fn preview_abort(ed: &mut Editor) -> bool {
    if ed.preview.is_some() {
        preview_close(ed);
        true
    } else {
        false
    }
}

pub fn preview_is_current(ed: &Editor) -> bool {
    ed.preview
        .as_ref()
        .is_some_and(|p| ed.buffer().name == p.bufname)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ScriptedIo;

    fn ed() -> Editor {
        Editor::new(Box::new(ScriptedIo::new()), 24, 80)
    }

    #[test]
    fn shell_escape_quotes() {
        assert_eq!(shell_escape("a b"), "'a b'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn autosave_path_convention() {
        assert_eq!(
            autosave_path("/tmp/dir/x.txt"),
            PathBuf::from("/tmp/dir/#x.txt#")
        );
    }

    #[test]
    fn capture_and_filter() {
        let mut e = ed();
        e.buffer_mut().insert_bytes(b"hello");
        e.execute_named("storeproc", false, 1); // プロンプトは ScriptedIo では None
        let out = run_capture("printf world", None).unwrap();
        assert_eq!(out.0, b"world");
    }

    #[test]
    fn preview_close_kills_grandchildren() {
        let pidfile =
            std::env::temp_dir().join(format!("kemacs_preview_pg_{}.pid", std::process::id()));
        let _ = std::fs::remove_file(&pidfile);
        let mut e = ed();
        assert_eq!(
            preview_start(
                &mut e,
                &format!("sleep 30 & echo $! > '{}'; wait", pidfile.display()),
                "*Preview*"
            ),
            Status::True
        );
        let mut grandchild = None;
        for _ in 0..50 {
            if let Ok(s) = std::fs::read_to_string(&pidfile) {
                if let Ok(p) = s.trim().parse::<i32>() {
                    grandchild = Some(p);
                    break;
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
        let pid = grandchild.expect("grandchild pid was not written");
        preview_close(&mut e);
        thread::sleep(Duration::from_millis(100));
        let alive = unsafe { kill(pid, 0) } == 0;
        let _ = std::fs::remove_file(&pidfile);
        assert!(!alive, "grandchild {} survived preview_close", pid);
    }

    #[test]
    fn preview_wait_first_collects_output_immediately() {
        let mut e = ed();
        assert_eq!(
            preview_start(&mut e, "printf 'banner line\\n'; sleep 2", "*Preview*"),
            Status::True
        );
        preview_wait_first(&mut e);
        let bi = e.find_buffer("*Preview*").unwrap();
        let text = String::from_utf8_lossy(&e.buffers[bi].text()).into_owned();
        assert!(text.contains("banner line"), "{}", text);
        preview_close(&mut e);
    }

    #[test]
    fn preview_lifecycle() {
        let mut e = ed();
        assert_eq!(
            preview_start(&mut e, "printf 'preview line\\n'", "*Preview*"),
            Status::True
        );
        for _ in 0..50 {
            e.preview_poll();
            if e.preview.as_ref().unwrap().finished {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let bi = e.find_buffer("*Preview*").unwrap();
        let text = String::from_utf8_lossy(&e.buffers[bi].text()).into_owned();
        assert!(text.contains("preview line"), "{}", text);
        assert!(text.contains("[preview finished]"), "{}", text);
        preview_close(&mut e);
        assert!(e.preview.is_none());
    }
}
