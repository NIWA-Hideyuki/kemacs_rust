//! クリップボード連携 (端末との共有)。
//!
//! - 書き込み: 内部バッファに保存し、OSC 52 (`ESC ] 52 ; c ; <base64> BEL`)
//!   で端末側のクリップボードにも転送する。Windows Terminal / iTerm2 /
//!   kitty / WezTerm などの主要な端末がこれを受け付ける。
//! - 読み出し: まず内部バッファ (kemacs がコピーした内容) を返し、無ければ
//!   `wl-paste` / `xclip` / `pbpaste` で OS クリップボードを読む。
//!   さらに端末からのブラケットペースト (`ESC [ 200~ ... ESC [ 201~`) は
//!   `input.rs` が擬似キーとして受け取り、同じ経路で挿入される。

use std::io::{IsTerminal, Write};
use std::process::Command;
use std::sync::Mutex;

static CLIPBOARD: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// テキストをクリップボードへ置く。
pub fn copy(text: &[u8]) {
    if let Ok(mut guard) = CLIPBOARD.lock() {
        *guard = Some(text.to_vec());
    }
    // 端末のクリップボードにも OSC 52 で通知する (非 TTY では何もしない)。
    if !std::io::stdout().is_terminal() {
        return;
    }
    let encoded = base64(text);
    let mut out = std::io::stdout();
    let _ = out.write_all(format!("\x1b]52;c;{}\x07", encoded).as_bytes());
    let _ = out.flush();
}

/// クリップボードの内容を読む。
///
/// まず kemacs がコピーした内容 (内部バッファ) を返し、それが無いときだけ
/// OS クリップボード (`wl-paste` / `xclip` / `pbpaste`) を読む。
/// どちらも空なら空を返す。
pub fn paste() -> Vec<u8> {
    if let Ok(guard) = CLIPBOARD.lock() {
        if let Some(text) = guard.as_ref() {
            return text.clone();
        }
    }
    read_os_clipboard().unwrap_or_default()
}

/// OS クリップボードを読める外部コマンドを順に試す。
///
/// Wayland / X11 / macOS のコマンドが無い環境 (WSL など) では Windows の
/// クリップボード (`powershell.exe -Command Get-Clipboard`) を読む。ブラウザや
/// 他のターミナルでコピーしたテキストはこちらの経路で取り込む。
fn read_os_clipboard() -> Option<Vec<u8>> {
    const CANDIDATES: [&[&str]; 4] = [
        &["wl-paste", "--no-newline"],
        &["xclip", "-selection", "clipboard", "-o"],
        &["xsel", "--clipboard", "--output"],
        &["pbpaste"],
    ];
    for cand in CANDIDATES {
        let Some((prog, args)) = cand.split_first() else {
            continue;
        };
        if let Ok(out) = Command::new(prog).args(args).output() {
            if out.status.success() && !out.stdout.is_empty() {
                return Some(normalize_pasted(&out.stdout));
            }
        }
    }
    read_windows_clipboard().map(|v| normalize_pasted(&v))
}

/// Windows クリップボードを PowerShell で読み出す (WSL / Windows 向け)。
///
/// 既定の出力エンコーディングのままだと CP932 (Shift-JIS) バイトが返るため、
/// 先に UTF-8 へ切り替えてから取得する。日本語の貼り付けに必須。
fn read_windows_clipboard() -> Option<Vec<u8>> {
    let out = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; Get-Clipboard",
        ])
        .output()
        .ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

/// 貼り付けテキストの改行を LF にそろえる (Windows の CRLF / 旧 Mac の CR)。
fn normalize_pasted(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        match text[i] {
            b'\r' => {
                out.push(b'\n');
                if text.get(i + 1) == Some(&b'\n') {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// 標準 Base64 (パディング付き) に符号化する。
fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(n >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[n as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn pasted_newlines_are_normalized() {
        assert_eq!(normalize_pasted(b"a\r\nb\rc\n"), b"a\nb\nc\n");
        assert_eq!(normalize_pasted(b""), b"");
    }

    // copy()/paste() はプロセス全体で共有される内部バッファを使うため、
    // 他のテストと競合しないよう単体テストでは扱わない。
    // クリップボード往復は PTY テスト (mouse_clipboard_pty.py) が検証する。
}
