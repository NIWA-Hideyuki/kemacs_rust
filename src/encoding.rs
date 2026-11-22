//! 日本語エンコーディング変換。C 版 `kanji/kgetc.c` / `kputc.c` の iconv 処理に相当。
//! UTF-8 を内部表現とし、読み込み時に変換・書き出し時に逆変換する。
//! 変換できないバイトは素通しすることでラウンドトリップを保証する。

use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};

extern "C" {
    fn iconv_open(tocode: *const c_char, fromcode: *const c_char) -> *mut c_void;
    fn iconv(
        cd: *mut c_void,
        inbuf: *mut *mut c_char,
        inbytesleft: *mut usize,
        outbuf: *mut *mut c_char,
        outbytesleft: *mut usize,
    ) -> usize;
    fn iconv_close(cd: *mut c_void) -> c_int;
}

const CONV_ERR: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Code {
    #[default]
    Utf8,
    Ujis,
    Sjis,
    Jis,
}

impl Code {
    pub fn iconv_name(self) -> &'static str {
        match self {
            Code::Utf8 => "UTF-8",
            Code::Ujis => "EUC-JP",
            Code::Sjis => "SHIFT_JIS",
            Code::Jis => "ISO-2022-JP",
        }
    }

    /// C 版 `code_string()` 相当の名前。
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Utf8 => "UTF8",
            Code::Ujis => "UJIS",
            Code::Sjis => "SJIS",
            Code::Jis => "JIS-NEW-ROMAJI",
        }
    }

    /// C 版 `string_code()` 相当。改行種別も一緒に解釈する。
    pub fn parse(s: &str) -> (Self, Eol) {
        let upper = s.trim().to_ascii_uppercase();
        let eol = if upper.contains("CRLF") {
            Eol::Crlf
        } else if upper.starts_with("CR/") {
            Eol::Cr
        } else {
            Eol::Lf
        };
        let code = if upper.contains("SJIS") {
            Code::Sjis
        } else if upper.contains("UJIS") || upper.contains("EUC") {
            Code::Ujis
        } else if upper.contains("JIS") {
            Code::Jis
        } else {
            Code::Utf8
        };
        (code, eol)
    }

    /// バイト列から符号化方式を推定する (UTF-8 -> SJIS -> EUC-JP -> JIS)。
    pub fn detect(bytes: &[u8]) -> Self {
        if std::str::from_utf8(bytes).is_ok() {
            return Code::Utf8;
        }
        for c in [Code::Sjis, Code::Ujis, Code::Jis] {
            if convert(bytes, c.iconv_name(), "UTF-8").is_some() {
                return c;
            }
        }
        Code::Utf8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Eol {
    #[default]
    Lf,
    Cr,
    Crlf,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "LF",
            Eol::Cr => "CR",
            Eol::Crlf => "CRLF",
        }
    }

    pub fn detect(bytes: &[u8]) -> Self {
        if bytes.windows(2).any(|w| w == b"\r\n") {
            Eol::Crlf
        } else if bytes.contains(&b'\r') && !bytes.contains(&b'\n') {
            Eol::Cr
        } else {
            Eol::Lf
        }
    }

    pub fn decode(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Eol::Lf => bytes.to_vec(),
            Eol::Crlf => {
                let mut v = bytes.to_vec();
                let mut i = 0;
                while i + 1 < v.len() {
                    if v[i] == b'\r' && v[i + 1] == b'\n' {
                        v.remove(i);
                    }
                    i += 1;
                }
                v
            }
            Eol::Cr => bytes
                .iter()
                .map(|&c| if c == b'\r' { b'\n' } else { c })
                .collect(),
        }
    }

    pub fn encode(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Eol::Lf => bytes.to_vec(),
            Eol::Crlf => {
                let mut out = Vec::with_capacity(bytes.len() + 16);
                for &c in bytes {
                    if c == b'\n' {
                        out.push(b'\r');
                    }
                    out.push(c);
                }
                out
            }
            Eol::Cr => bytes
                .iter()
                .map(|&c| if c == b'\n' { b'\r' } else { c })
                .collect(),
        }
    }
}

/// iconv で一括変換する。失敗したら None。
pub fn convert(bytes: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    let from_c = CString::new(from).ok()?;
    let to_c = CString::new(to).ok()?;
    unsafe {
        let cd = iconv_open(to_c.as_ptr(), from_c.as_ptr());
        if cd as isize == -1 || cd.is_null() {
            return None;
        }
        let mut inptr = bytes.as_ptr() as *mut c_char;
        let mut inleft = bytes.len();
        let mut out = vec![0u8; bytes.len().saturating_mul(4).saturating_add(32)];
        let mut outptr = out.as_mut_ptr() as *mut c_char;
        let mut outleft = out.len();
        let r = iconv(cd, &mut inptr, &mut inleft, &mut outptr, &mut outleft);
        if r == CONV_ERR {
            iconv_close(cd);
            return None;
        }
        // 状態を持つ符号 (ISO-2022-JP) の終端エスケープを出す
        let mut nptr: *mut c_char = std::ptr::null_mut();
        let mut zero_left = 0usize;
        let _ = iconv(cd, &mut nptr, &mut zero_left, &mut outptr, &mut outleft);
        iconv_close(cd);
        let used = out.len() - outleft;
        out.truncate(used);
        Some(out)
    }
}

/// 変換できないバイトを 1 バイトずつ素通しして、全バイトを損失なく変換する。
pub fn convert_lossless(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    if from == to {
        return bytes.to_vec();
    }
    if let Some(all) = convert(bytes, from, to) {
        return all;
    }
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let end = (i + 8).min(bytes.len());
        let mut done = false;
        for j in (i + 1..=end).rev() {
            if let Some(chunk) = convert(&bytes[i..j], from, to) {
                out.extend_from_slice(&chunk);
                i = j;
                done = true;
                break;
            }
        }
        if !done {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// 表現不能文字を可逆エスケープするための ASCII マーカー。
const ESC_MARK: &str = "KEMACS-U+";
/// マーカー文字列そのものを表すエスケープ。
const ESC_LITERAL: &str = "KEMACS-L;";
/// マーカー共用の接頭辞そのものを表すエスケープ。
const ESC_PREFIX: &str = "KEMACS-B;";

fn char_encodable(ch: char, code: Code) -> bool {
    if code == Code::Utf8 {
        return true;
    }
    let mut buf = [0u8; 4];
    let src = ch.encode_utf8(&mut buf).as_bytes();
    match convert(src, "UTF-8", code.iconv_name()) {
        Some(out) => convert(&out, code.iconv_name(), "UTF-8").as_deref() == Some(src),
        None => false,
    }
}

/// 文字コードで表現できない Unicode 文字を `~@KEMACS-U+XXXX;` に変換する。
/// マーカー文字列も退避するため往復が可逆。
pub fn from_utf8(bytes: &[u8], code: Code) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes).replace("KEMACS-", ESC_PREFIX);
    let mut result = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < text.len() {
        let ch = text[i..].chars().next().unwrap();
        let clen = ch.len_utf8();
        if char_encodable(ch, code) {
            result.push(ch);
        } else {
            result.push_str(&format!("{}{:X};", ESC_MARK, ch as u32));
        }
        i += clen;
    }
    convert_lossless(result.as_bytes(), "UTF-8", code.iconv_name())
}

/// `from_utf8()` のエスケープを元の Unicode 文字へ戻す。
pub fn to_utf8(bytes: &[u8], code: Code) -> Vec<u8> {
    let decoded = convert_lossless(bytes, code.iconv_name(), "UTF-8");
    let text = String::from_utf8_lossy(&decoded);
    let mut out = String::with_capacity(text.len());
    let mut rest: &str = &text;
    loop {
        if let Some(pos) = rest.find("KEMACS-") {
            out.push_str(&rest[..pos]);
            let tail = &rest[pos..];
            if let Some(after) = tail.strip_prefix(ESC_PREFIX) {
                out.push_str("KEMACS-");
                rest = after;
                continue;
            }
            if let Some(after) = tail.strip_prefix(ESC_LITERAL) {
                out.push_str(ESC_MARK);
                rest = after;
                continue;
            }
            if let Some(after) = tail.strip_prefix(ESC_MARK) {
                if let Some(end) = after.find(';') {
                    if let Ok(v) = u32::from_str_radix(&after[..end], 16) {
                        if let Some(ch) = char::from_u32(v) {
                            out.push(ch);
                            rest = &after[end + 1..];
                            continue;
                        }
                    }
                }
            }
            out.push_str("KEMACS-");
            rest = &tail[8..];
            continue;
        }
        out.push_str(rest);
        break;
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_sjis_roundtrip() {
        let src = "日本語テスト".as_bytes();
        let sjis = from_utf8(src, Code::Sjis);
        assert_ne!(sjis, src);
        assert_eq!(to_utf8(&sjis, Code::Sjis), src);
    }

    #[test]
    fn utf8_ujis_roundtrip() {
        let src = "漢字かな".as_bytes();
        let ujis = from_utf8(src, Code::Ujis);
        assert_eq!(to_utf8(&ujis, Code::Ujis), src);
    }

    #[test]
    fn jis_escape_sequences() {
        let sjis = from_utf8("日本語".as_bytes(), Code::Sjis);
        let jis = from_utf8("日本語".as_bytes(), Code::Jis);
        assert!(jis.contains(&0x1b));
        assert_eq!(to_utf8(&jis, Code::Jis), "日本語".as_bytes());
        assert!(!sjis.is_empty());
    }

    #[test]
    fn unrepresentable_emoji_roundtrip() {
        let src = "絵文字 😀 と KEMACS-U+  Include".as_bytes();
        for code in [Code::Sjis, Code::Ujis, Code::Jis] {
            let enc = from_utf8(src, code);
            assert_eq!(to_utf8(&enc, code), src, "code={:?}", code);
        }
    }

    #[test]
    fn eol_roundtrip() {
        let src = b"a\r\nb\rc\n";
        let lf = Eol::Crlf.decode(src);
        assert_eq!(lf, b"a\nb\rc\n");
        assert_eq!(Eol::Crlf.encode(b"a\nb\n"), b"a\r\nb\r\n");
        assert_eq!(Eol::Cr.encode(b"a\nb\n"), b"a\rb\r");
    }

    #[test]
    fn parse_code_names() {
        assert_eq!(Code::parse("UTF8"), (Code::Utf8, Eol::Lf));
        assert_eq!(Code::parse("CRLF/UTF8"), (Code::Utf8, Eol::Crlf));
        assert_eq!(Code::parse("UJIS"), (Code::Ujis, Eol::Lf));
        assert_eq!(Code::parse("SJIS"), (Code::Sjis, Eol::Lf));
        assert_eq!(Code::detect("日本語".as_bytes()), Code::Utf8);
    }
}
