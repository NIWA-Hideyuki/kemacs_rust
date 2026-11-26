//! バッファ / 行 / undo の中核。C 版 `line.C_`, `buffer.C_`, `undo.C_` に相当。
//! テキストはバイト列のまま保持し、改行は行境界で表現する (C 版と同じ)。
//! 最終要素は C 版の `b_linep` (バッファ終端行) に相当する空行。

use crate::crypt::{strong_decrypt, strong_encrypt, Cipher, CryptKind};
use crate::encoding::{self, Code, Eol};
use crate::keycode::Status;
use std::collections::VecDeque;

/// 行番号 + バイトオフセット。C 版の (LINE *, offset) の代わりに使う。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub off: usize,
}

impl Pos {
    pub fn new(line: usize, off: usize) -> Self {
        Pos { line, off }
    }
}

pub const BFINVS: u8 = 0x01;
pub const BFCHG: u8 = 0x02;

pub const MDWRAP: u32 = 1 << 0;
pub const MDCMOD: u32 = 1 << 1;
pub const MDPYMOD: u32 = 1 << 2;
pub const MDSPELL: u32 = 1 << 3;
pub const MDEXACT: u32 = 1 << 4;
pub const MDVIEW: u32 = 1 << 5;
pub const MDOVER: u32 = 1 << 6;
pub const MDMDMOD: u32 = 1 << 7;
pub const MDMAGIC: u32 = 1 << 8;
pub const MDCRYPT: u32 = 1 << 9;
pub const MDASAVE: u32 = 1 << 10;
pub const MDBINARY: u32 = 1 << 11;

pub const MODE_NAMES: [&str; 12] = [
    "WRAP", "CMODE", "PYMODE", "SPELL", "EXACT", "VIEW", "OVER", "MDMODE", "MAGIC", "CRYPT",
    "ASAVE", "BINARY",
];
pub const MODE_CODES: [u8; 12] = *b"WCPSEVODMYAB";

/// コマンド実行「前」の状態を一時的に保持するスナップショット。
/// undo 履歴には積まず、`record_undo()` が差分へ変換したのち破棄する。
/// C 版はコマンド境界ごとにレコードをまとめるが、同じ粒度を
/// 「1 コマンド = 1 エントリ」で実現する。
#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub lines: Vec<Vec<u8>>,
    pub dot: Pos,
    pub mark: Option<Pos>,
    pub fname: Option<String>,
    pub mode: u32,
    pub flags: u8,
    pub key: Option<Vec<u8>>,
    pub crypt: CryptKind,
    pub password: Option<Vec<u8>>,
}

/// Snapshot から位置・モード・暗号鍵だけを抜き出した軽量メタデータ。
#[derive(Clone, PartialEq, Eq)]
struct UndoMeta {
    dot: Pos,
    mark: Option<Pos>,
    fname: Option<String>,
    mode: u32,
    flags: u8,
    key: Option<Vec<u8>>,
    crypt: CryptKind,
    password: Option<Vec<u8>>,
}

impl UndoMeta {
    fn from_snapshot(s: &Snapshot) -> Self {
        UndoMeta {
            dot: s.dot,
            mark: s.mark,
            fname: s.fname.clone(),
            mode: s.mode,
            flags: s.flags,
            key: s.key.clone(),
            crypt: s.crypt,
            password: s.password.clone(),
        }
    }

    fn from_buffer(b: &Buffer) -> Self {
        UndoMeta {
            dot: b.dot,
            mark: b.mark,
            fname: b.fname.clone(),
            mode: b.mode,
            flags: b.flags,
            key: b.key.clone(),
            crypt: b.crypt,
            password: b.password.clone(),
        }
    }

    fn apply_to(&self, b: &mut Buffer) {
        b.dot = self.dot;
        b.mark = self.mark;
        b.fname = self.fname.clone();
        b.mode = self.mode;
        b.flags = self.flags;
        b.key = self.key.clone();
        b.crypt = self.crypt;
        b.password = self.password.clone();
    }
}

impl std::fmt::Debug for UndoMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoMeta")
            .field("dot", &self.dot)
            .field("mark", &self.mark)
            .field("fname", &self.fname)
            .field("mode", &self.mode)
            .field("flags", &self.flags)
            .field("key", &self.key.as_ref().map(|_| "<redacted>"))
            .field("crypt", &self.crypt)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// 1 コマンドぶんの差分。共通の行・バイトは長さだけ保持し、実際に
/// 増減したバイト列だけを残すため、長いファイルでも undo 履歴が
/// 行全体のコピーで膨らまない。
#[derive(Clone, Debug)]
pub struct UndoEntry {
    head: usize,
    tail: usize,
    prefix: usize,
    suffix: usize,
    old_mid: Vec<u8>,
    new_mid: Vec<u8>,
    old_region_lines: usize,
    new_region_lines: usize,
    meta: UndoMeta,
}

impl UndoEntry {
    /// 差分そのものは変えず、戻り先の状態メタデータだけ差し替える。
    /// 同じエントリを undo では旧状態へ、redo では新状態へ適用する。
    fn with_meta(&self, meta: UndoMeta) -> Self {
        UndoEntry {
            meta,
            ..self.clone()
        }
    }
}

/// 行配列を改行で連結する (差分計算用の一時表現)。
fn join_lines(lines: &[Vec<u8>]) -> Vec<u8> {
    let len = lines.iter().map(|l| l.len()).sum::<usize>() + lines.len().saturating_sub(1);
    let mut out = Vec::with_capacity(len);
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push(b'\n');
        }
        out.extend_from_slice(line);
    }
    out
}

/// 連結されたバイト列を `count` 行へ戻す。
fn split_region(bytes: &[u8], count: usize) -> Vec<Vec<u8>> {
    if count == 0 {
        return Vec::new();
    }
    if bytes.is_empty() {
        return vec![Vec::new(); count];
    }
    let mut lines: Vec<Vec<u8>> = bytes.split(|&c| c == b'\n').map(|s| s.to_vec()).collect();
    if lines.len() < count {
        lines.resize(count, Vec::new());
    } else if lines.len() > count {
        lines.truncate(count);
    }
    lines
}

#[derive(Clone)]
pub struct Buffer {
    pub name: String,
    pub fname: Option<String>,
    /// 行の配列。最後の要素は常に終端行 (空)。
    pub lines: Vec<Vec<u8>>,
    pub dot: Pos,
    pub mark: Option<Pos>,
    pub mode: u32,
    pub flags: u8,
    pub nwnd: usize,
    pub undo: VecDeque<UndoEntry>,
    pub redo: VecDeque<UndoEntry>,
    pub undo_limit: usize,
    pub code: Code,
    pub eol: Eol,
    pub original_bytes: Option<Vec<u8>>,
    pub original_utf8: Option<Vec<u8>>,
    /// setcrkey で設定した暗号化済みキー (C 版 b_key 相当)。
    pub key: Option<Vec<u8>>,
    /// 強暗号 (AEAD) のパスワード。Classic モードでは未使用。
    pub crypt: CryptKind,
    pub password: Option<Vec<u8>>,
    /// 強暗号の復号に失敗した (パスワード誤りなど)。
    pub decrypt_failed: bool,
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("lines", &self.lines)
            .field("dot", &self.dot)
            .field("mark", &self.mark)
            .field("fname", &self.fname)
            .field("mode", &self.mode)
            .field("flags", &self.flags)
            .field("key", &self.key.as_ref().map(|_| "<redacted>"))
            .field("crypt", &self.crypt)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("name", &self.name)
            .field("fname", &self.fname)
            .field("lines", &self.lines)
            .field("dot", &self.dot)
            .field("mark", &self.mark)
            .field("mode", &self.mode)
            .field("flags", &self.flags)
            .field("nwnd", &self.nwnd)
            .field("undo_len", &self.undo.len())
            .field("redo_len", &self.redo.len())
            .field("code", &self.code)
            .field("eol", &self.eol)
            .field("crypt", &self.crypt)
            .field("key", &self.key.as_ref().map(|_| "<redacted>"))
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("decrypt_failed", &self.decrypt_failed)
            .finish()
    }
}

impl Buffer {
    pub fn new(name: impl Into<String>) -> Self {
        Buffer {
            name: name.into(),
            fname: None,
            lines: vec![Vec::new()],
            dot: Pos::default(),
            mark: None,
            mode: 0,
            // 既定は可視バッファ。BFINVS は一時バッファ用で、ここで付けると
            // quit() の未保存確認から漏れてしまう。
            flags: 0,
            nwnd: 0,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            undo_limit: 0,
            code: Code::Utf8,
            eol: Eol::Lf,
            original_bytes: None,
            original_utf8: None,
            key: None,
            crypt: CryptKind::None,
            password: None,
            decrypt_failed: false,
        }
    }

    /// ファイルのバイト列からバッファを作る。改行で分割し、末尾の改行は
    /// 終端空行として保持する (書き出し時にバイト単位で元に戻る)。
    pub fn from_bytes(name: impl Into<String>, fname: Option<String>, bytes: &[u8]) -> Self {
        let mut b = Buffer::new(name);
        b.fname = fname;
        b.flags &= !BFINVS;
        b.load_encoded(bytes, Code::Utf8, Eol::Lf);
        b
    }

    /// 指定した符号化方式でファイルを読み込む (C 版 readin() 相当)。
    pub fn from_encoded(
        name: impl Into<String>,
        fname: Option<String>,
        bytes: &[u8],
        code: Code,
        eol: Eol,
    ) -> Self {
        let mut b = Buffer::new(name);
        b.fname = fname;
        b.flags &= !BFINVS;
        b.load_encoded(bytes, code, eol);
        b
    }

    pub fn load_encoded(&mut self, bytes: &[u8], code: Code, eol: Eol) {
        let mut raw = bytes.to_vec();
        let mut decrypt_failed = false;
        // 強暗号はバイト列そのものを復号してから文字コード変換する
        if self.crypt == CryptKind::Strong {
            if let Some(pass) = self.password.clone() {
                if let Some(dec) = strong_decrypt(&raw, &pass) {
                    raw = dec;
                } else {
                    decrypt_failed = true;
                }
            } else {
                decrypt_failed = true;
            }
        }
        self.decrypt_failed = decrypt_failed;
        let normalized = eol.decode(&raw);
        let mut utf8 = encoding::to_utf8(&normalized, code);
        if self.crypt == CryptKind::Classic {
            if let Some(k) = self.key.clone() {
                Cipher::from_key(&k).decrypt(&mut utf8);
            }
        }
        self.lines = utf8.split(|&c| c == b'\n').map(|s| s.to_vec()).collect();
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        self.dot = Pos::default();
        self.mark = None;
        self.code = code;
        self.eol = eol;
        self.original_bytes = Some(bytes.to_vec());
        self.original_utf8 = Some(utf8);
        self.undo.clear();
        self.redo.clear();
        self.flags &= !BFCHG;
    }

    /// 保存時に書き出すバイト列。未編集なら元バイト列をそのまま返す。
    pub fn encoded_text(&self) -> Vec<u8> {
        let cur = self.text();
        if self.original_utf8.as_ref() == Some(&cur) {
            if let Some(orig) = &self.original_bytes {
                return orig.clone();
            }
        }
        let mut converted = encoding::from_utf8(&cur, self.code);
        if self.crypt == CryptKind::Classic {
            if let Some(k) = self.key.clone() {
                Cipher::from_key(&k).crypt(&mut converted);
            }
        }
        let out = self.eol.encode(&converted);
        if self.crypt == CryptKind::Strong {
            if let Some(pass) = self.password.clone() {
                if let Some(enc) = strong_encrypt(&out, &pass) {
                    return enc;
                }
            }
        }
        out
    }

    /// 文字コード / 改行種別を変更する (C 版 $bcode 相当)。
    pub fn recode(&mut self, code: Code, eol: Eol) {
        let cur = self.text();
        self.code = code;
        self.eol = eol;
        self.original_bytes = None;
        self.original_utf8 = Some(cur);
    }

    pub fn text(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push(b'\n');
            }
            out.extend_from_slice(line);
        }
        out
    }

    /// 終端行は使わず、全行が実データ。位置が範囲外のときだけ終端とみなす。
    pub fn is_header(&self, p: Pos) -> bool {
        p.line >= self.lines.len()
    }
    pub fn line_len(&self, line: usize) -> usize {
        self.lines.get(line).map_or(0, |l| l.len())
    }
    pub fn eolp(&self) -> bool {
        self.dot.off >= self.line_len(self.dot.line)
    }
    pub fn bobp(&self) -> bool {
        self.dot == Pos::default()
    }
    pub fn eobp(&self) -> bool {
        self.dot.line + 1 == self.lines.len() && self.dot.off == self.line_len(self.dot.line)
    }
    pub fn line_count(&self) -> usize {
        if self.lines.len() == 1 && self.lines[0].is_empty() {
            0
        } else {
            self.lines.len()
        }
    }

    /// C 版の終端行 `b_linep` に対応する空行。
    pub fn b_linep(&self) -> &'static [u8] {
        &[]
    }

    /// 終端行のインデックス。末尾改行は終端行の直前の空行として保持する。
    pub fn b_linep_index(&self) -> usize {
        if self.lines.last().is_some_and(|l| l.is_empty()) {
            self.lines.len().saturating_sub(1)
        } else {
            self.lines.len()
        }
    }

    /// 実在数据の行数 (終端行は含まない)。
    pub fn real_line_count(&self) -> usize {
        self.b_linep_index()
    }

    /// 行番号が終端行なら空スライスを返す (範囲外でも安全)。
    pub fn line_or_sentinel(&self, li: usize) -> &[u8] {
        if li >= self.b_linep_index() {
            self.b_linep()
        } else {
            self.lines[li].as_slice()
        }
    }

    pub fn clamp_dot(&mut self) {
        if self.dot.line >= self.lines.len() {
            self.dot.line = self.lines.len().saturating_sub(1);
        }
        let max = self.line_len(self.dot.line);
        if self.dot.off > max {
            self.dot.off = max;
        }
        if let Some(m) = self.mark {
            let mut m = m;
            if m.line >= self.lines.len() {
                m.line = self.lines.len().saturating_sub(1);
            }
            let ml = self.line_len(m.line);
            if m.off > ml {
                m.off = ml;
            }
            self.mark = Some(m);
        }
    }

    /// UTF-8 なら 1 文字分のバイト数、そうでなければ 1 バイト。
    pub fn char_len_at(&self, line: usize, off: usize) -> usize {
        let Some(l) = self.lines.get(line) else {
            return 0;
        };
        if off >= l.len() {
            return 0;
        }
        if let Ok(s) = std::str::from_utf8(&l[off..]) {
            return s.chars().next().map_or(1, |c| c.len_utf8());
        }
        1
    }

    /// 前方の文字の開始位置までのバイト数。
    pub fn prev_char_len(&self, line: usize, off: usize) -> usize {
        if off == 0 {
            return 0;
        }
        let Some(l) = self.lines.get(line) else {
            return 0;
        };
        if let Ok(s) = std::str::from_utf8(&l[..off.min(l.len())]) {
            return s.chars().last().map_or(1, |c| c.len_utf8());
        }
        1
    }

    /// C 版 `linsert(n, c)` に相当。同じバイトを n 個挿入する。
    pub fn insert_repeat(&mut self, n: usize, c: u8) -> bool {
        if self.mode & MDVIEW != 0 {
            return false;
        }
        if n == 0 {
            return true;
        }
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        let n = n.min(1_000_000);
        let li = self.dot.line.min(self.lines.len() - 1);
        let off = self.dot.off.min(self.lines[li].len());
        self.lines[li].splice(off..off, std::iter::repeat_n(c, n));
        self.dot = Pos::new(li, off + n);
        self.flags |= BFCHG;
        self.original_bytes = None;
        true
    }

    /// バイト列を現在位置に挿入する ('\n' は改行として扱う)。
    pub fn insert_bytes(&mut self, data: &[u8]) -> bool {
        if self.mode & MDVIEW != 0 {
            return false;
        }
        let mut changed = false;
        for &c in data {
            if c == b'\n' {
                self.newline();
                changed = true;
            } else {
                changed |= self.insert_repeat(1, c);
            }
        }
        changed
    }

    /// C 版 `lnewline()` に相当。現在位置で行を分割し、ドットは次の行頭へ。
    pub fn newline(&mut self) -> bool {
        if self.mode & MDVIEW != 0 {
            return false;
        }
        let li = self.dot.line.min(self.lines.len().saturating_sub(1));
        let off = self.dot.off.min(self.lines[li].len());
        let tail = self.lines[li].split_off(off);
        self.lines.insert(li + 1, tail);
        self.dot = Pos::new(li + 1, 0);
        self.flags |= BFCHG;
        self.original_bytes = None;
        true
    }

    /// C 版 `ldelete(n, kflag)` に相当。削除したバイトを返す。
    pub fn delete(&mut self, n: usize, kflag: bool) -> (bool, Vec<u8>) {
        let mut killed = Vec::new();
        if self.mode & MDVIEW != 0 {
            return (false, killed);
        }
        let mut remain = n;
        while remain > 0 {
            self.clamp_dot();
            let li = self.dot.line;
            if self.is_header(self.dot) {
                return (false, killed);
            }
            let len = self.line_len(li);
            let off = self.dot.off.min(len);
            if off < len {
                let chunk = remain.min(len - off);
                if kflag {
                    killed.extend_from_slice(&self.lines[li][off..off + chunk]);
                }
                self.lines[li].drain(off..off + chunk);
                remain -= chunk;
                self.flags |= BFCHG;
                self.original_bytes = None;
                continue;
            }
            // 行末: 改行を削除して次の行と結合する
            if li + 1 < self.lines.len() {
                if kflag {
                    killed.push(b'\n');
                }
                let tail = self.lines.remove(li + 1);
                self.lines[li].extend_from_slice(&tail);
                remain -= 1;
                self.flags |= BFCHG;
                self.original_bytes = None;
                continue;
            }
            // 最終行の末尾では C 版と同じく改行削除は何もしない。
            // ここで break しないと remain 回だけ kill 文字列に改行が積まれる。
            break;
        }
        self.clamp_dot();
        (true, killed)
    }

    /// カーソル位置の 1 文字を削除する (UTF-8 対応)。
    pub fn delete_char(&mut self, kflag: bool) -> (bool, Vec<u8>) {
        let len = self.char_len_at(self.dot.line, self.dot.off);
        if len == 0 {
            return self.delete(1, kflag);
        }
        self.delete(len, kflag)
    }

    /// C 版 `backdel()` に相当。
    pub fn back_delete(&mut self, kflag: bool) -> (bool, Vec<u8>) {
        if self.mode & MDVIEW != 0 {
            return (false, Vec::new());
        }
        if self.dot.off > 0 {
            let n = self.prev_char_len(self.dot.line, self.dot.off);
            self.dot.off -= n;
            return self.delete(n, kflag);
        }
        if self.dot.line == 0 {
            return (false, Vec::new());
        }
        // 前の行末へ移動して改行を削除
        self.dot.line -= 1;
        self.dot.off = self.line_len(self.dot.line);
        self.delete(1, kflag)
    }

    /// mark と dot の間のバイト列 (終端は含まない) を返す。
    /// 終端行 (dot.line == lines.len()) を指していても安全に扱う。
    pub fn region_bytes(&self) -> Option<Vec<u8>> {
        let m = self.mark?;
        let (start, end) = if m <= self.dot {
            (m, self.dot)
        } else {
            (self.dot, m)
        };
        let mut a = self.pos_to_offset(start);
        let mut b = self.pos_to_offset(end);
        let text = self.text();
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        if b > text.len() {
            return None;
        }
        Some(text[a..b].to_vec())
    }

    /// mark と dot の間を data で置き換える (C 版 `sreplace` の内部操作)。
    pub fn pos_to_offset(&self, p: Pos) -> usize {
        let mut off = 0usize;
        let limit = p.line.min(self.lines.len());
        for li in 0..limit {
            off += self.lines[li].len();
            if li + 1 < self.lines.len() {
                off += 1;
            }
        }
        off + p.off.min(self.line_len(p.line))
    }

    /// mark と dot の間を data で置き換える (C 版 `sreplace` の内部操作)。
    pub fn replace_region(&mut self, data: &[u8]) -> bool {
        let Some(m) = self.mark else { return false };
        let (start, end) = if m <= self.dot {
            (m, self.dot)
        } else {
            (self.dot, m)
        };
        let a = self.pos_to_offset(start);
        let b = self.pos_to_offset(end);
        let mut text = self.text();
        if b > text.len() || a > b {
            return false;
        }
        text.splice(a..b, data.iter().copied());
        self.lines = text.split(|&c| c == b'\n').map(|s| s.to_vec()).collect();
        if self.lines.is_empty() {
            self.lines.push(Vec::new());
        }
        self.dot = start;
        self.mark = None;
        self.flags |= BFCHG;
        self.original_bytes = None;
        true
    }

    /// mark と dot の間を削除し、削除したバイト列を返す。
    pub fn delete_region(&mut self) -> Vec<u8> {
        let Some(bytes) = self.region_bytes() else {
            return Vec::new();
        };
        let start = self.mark.map(|m| m.min(self.dot)).unwrap_or(self.dot);
        let a = self.pos_to_offset(start);
        let mut text = self.text();
        if a <= text.len() && a + bytes.len() <= text.len() {
            text.drain(a..a + bytes.len());
            self.lines = text.split(|&c| c == b'\n').map(|s| s.to_vec()).collect();
            if self.lines.is_empty() {
                self.lines.push(Vec::new());
            }
            self.dot = start;
            self.mark = None;
            self.flags |= BFCHG;
            self.original_bytes = None;
        }
        bytes
    }

    /// C 版互換の弱暗号キーを設定する (setcrkey 相当)。
    pub fn set_key(&mut self, plaintext: &[u8]) {
        self.crypt = CryptKind::Classic;
        self.key = Some(Cipher::prepare_key(plaintext));
        self.password = None;
        self.decrypt_failed = false;
        self.mode |= MDCRYPT;
        self.original_bytes = None;
    }

    /// 認証付き強暗号のパスワードを設定する。
    pub fn set_strong_key(&mut self, password: &[u8]) {
        self.crypt = CryptKind::Strong;
        self.password = Some(password.to_vec());
        self.key = None;
        self.decrypt_failed = false;
        self.mode |= MDCRYPT;
        self.original_bytes = None;
    }

    pub fn disable_crypt(&mut self) {
        self.crypt = CryptKind::None;
        if let Some(mut k) = self.key.take() {
            k.fill(0);
        }
        if let Some(mut p) = self.password.take() {
            p.fill(0);
        }
        self.decrypt_failed = false;
        self.mode &= !MDCRYPT;
        self.original_bytes = None;
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            dot: self.dot,
            mark: self.mark,
            fname: self.fname.clone(),
            mode: self.mode,
            flags: self.flags,
            key: self.key.clone(),
            crypt: self.crypt,
            password: self.password.clone(),
        }
    }

    pub fn restore(&mut self, s: &Snapshot) {
        self.lines = s.lines.clone();
        self.dot = s.dot;
        self.mark = s.mark;
        self.fname = s.fname.clone();
        self.mode = s.mode;
        self.flags = s.flags;
        self.key = s.key.clone();
        self.crypt = s.crypt;
        self.password = s.password.clone();
        self.clamp_dot();
    }

    pub fn same_text(&self, s: &Snapshot) -> bool {
        self.lines == s.lines
    }

    /// 変更前に取得したスナップショット `before` を差分へ変換して履歴に積む。
    /// 呼び出しは必ず変更「後」に行うこと (差分は現在状態との比較で決まる)。
    pub fn record_undo(&mut self, before: Snapshot) {
        let entry = self.make_undo_entry(&before);
        if self.undo_limit > 0 && self.undo.len() >= self.undo_limit {
            self.undo.pop_front();
        }
        self.undo.push_back(entry);
        self.redo.clear();
    }

    /// undo / redo 履歴が実際に保持している差分バイト数の合計。
    pub fn undo_bytes(&self) -> usize {
        let sum = |q: &VecDeque<UndoEntry>| -> usize {
            q.iter().map(|e| e.old_mid.len() + e.new_mid.len()).sum()
        };
        sum(&self.undo) + sum(&self.redo)
    }

    fn make_undo_entry(&self, before: &Snapshot) -> UndoEntry {
        let a = &before.lines;
        let b = &self.lines;
        // 先頭と末尾の共通行を除く
        let mut head = 0usize;
        let max = a.len().min(b.len());
        while head < max && a[head] == b[head] {
            head += 1;
        }
        let mut tail = 0usize;
        while tail < a.len() - head
            && tail < b.len() - head
            && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
        {
            tail += 1;
        }
        let a_region = &a[head..a.len() - tail];
        let b_region = &b[head..b.len() - tail];
        let a_bytes = join_lines(a_region);
        let b_bytes = join_lines(b_region);
        // 行内でも先頭・末尾の共通バイトを除き、増減した部分だけ残す
        let mut prefix = 0usize;
        let m = a_bytes.len().min(b_bytes.len());
        while prefix < m && a_bytes[prefix] == b_bytes[prefix] {
            prefix += 1;
        }
        let mut suffix = 0usize;
        while suffix < a_bytes.len() - prefix
            && suffix < b_bytes.len() - prefix
            && a_bytes[a_bytes.len() - 1 - suffix] == b_bytes[b_bytes.len() - 1 - suffix]
        {
            suffix += 1;
        }
        UndoEntry {
            head,
            tail,
            prefix,
            suffix,
            old_mid: a_bytes[prefix..a_bytes.len() - suffix].to_vec(),
            new_mid: b_bytes[prefix..b_bytes.len() - suffix].to_vec(),
            old_region_lines: a_region.len(),
            new_region_lines: b_region.len(),
            meta: UndoMeta::from_snapshot(before),
        }
    }

    fn apply_entry(&mut self, e: &UndoEntry, to_old: bool) {
        let cur_len = self.lines.len();
        let start = e.head.min(cur_len);
        let end = cur_len.saturating_sub(e.tail).max(start);
        // 共通部分のバイトは現在の行から借りる (履歴には持たない)
        let region = join_lines(&self.lines[start..end]);
        let prefix_end = e.prefix.min(region.len());
        let suffix_start = region.len().saturating_sub(e.suffix).max(prefix_end);
        let mid = if to_old { &e.old_mid } else { &e.new_mid };
        let mut bytes = Vec::with_capacity(prefix_end + mid.len() + region.len() - suffix_start);
        bytes.extend_from_slice(&region[..prefix_end]);
        bytes.extend_from_slice(mid);
        bytes.extend_from_slice(&region[suffix_start..]);
        let count = if to_old {
            e.old_region_lines
        } else {
            e.new_region_lines
        };
        let replacement = split_region(&bytes, count);
        self.lines.splice(start..end, replacement);
        e.meta.apply_to(self);
        self.clamp_dot();
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop_back() else {
            return false;
        };
        let cur = UndoMeta::from_buffer(self);
        self.apply_entry(&prev, true);
        if self.undo_limit == 0 || self.redo.len() < self.undo_limit {
            self.redo.push_back(prev.with_meta(cur));
        }
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop_back() else {
            return false;
        };
        let cur = UndoMeta::from_buffer(self);
        self.apply_entry(&next, false);
        self.undo.push_back(next.with_meta(cur));
        true
    }

    /// 1 文字進む。成功したら true。
    pub fn forward_char(&mut self) -> bool {
        if self.eolp() {
            if self.is_header(self.dot) {
                return false;
            }
            self.dot.line += 1;
            self.dot.off = 0;
            true
        } else {
            let n = self.char_len_at(self.dot.line, self.dot.off);
            self.dot.off += n.max(1);
            true
        }
    }

    pub fn backward_char(&mut self) -> bool {
        if self.dot.off > 0 {
            let n = self.prev_char_len(self.dot.line, self.dot.off);
            self.dot.off -= n.max(1);
            true
        } else if self.dot.line > 0 {
            self.dot.line -= 1;
            self.dot.off = self.line_len(self.dot.line);
            true
        } else {
            false
        }
    }

    pub fn goto_bob(&mut self) {
        self.dot = Pos::default();
    }

    pub fn goto_eob(&mut self) {
        self.dot.line = self.lines.len().saturating_sub(1);
        self.dot.off = self.line_len(self.dot.line);
    }

    pub fn goto_line(&mut self, n: usize) -> bool {
        if n == 0 || n > self.line_count() {
            return false;
        }
        self.dot = Pos::new(n - 1, 0);
        true
    }

    fn is_word_byte(c: u8) -> bool {
        c.is_ascii_alphanumeric() || c >= 0x80 || c == b'_'
    }

    pub fn forward_word(&mut self) -> bool {
        let mut li = self.dot.line;
        let mut off = self.dot.off;
        loop {
            if li >= self.lines.len() {
                return false;
            }
            let l = &self.lines[li];
            while off < l.len() && !Self::is_word_byte(l[off]) {
                off += 1;
            }
            if off < l.len() {
                break;
            }
            if li + 1 >= self.lines.len() {
                return false;
            }
            li += 1;
            off = 0;
        }
        let l = &self.lines[li];
        while off < l.len() && Self::is_word_byte(l[off]) {
            off += 1;
        }
        self.dot = Pos::new(li, off);
        true
    }

    pub fn backward_word(&mut self) -> bool {
        let mut li = self.dot.line;
        let mut off = self.dot.off;
        loop {
            let l = self.lines.get(li).cloned().unwrap_or_default();
            while off > 0 && !Self::is_word_byte(l[off - 1]) {
                off -= 1;
            }
            if off > 0 {
                break;
            }
            if li == 0 {
                return false;
            }
            li -= 1;
            off = self.line_len(li);
        }
        let l = self.lines[li].clone();
        while off > 0 && Self::is_word_byte(l[off - 1]) {
            off -= 1;
        }
        self.dot = Pos::new(li, off);
        true
    }

    /// 两端の空行を除いた実効行数 (C 版の b_nlines 相当)。
    pub fn effective_lines(&self) -> usize {
        self.line_count()
    }
}

impl Status {
    /// Buffer 由来の操作結果を Status に変換する。
    pub fn from_ok(ok: bool) -> Self {
        if ok {
            Status::True
        } else {
            Status::False
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_visible_and_quit_can_see_it() {
        let b = Buffer::new("main");
        assert_eq!(b.flags & BFINVS, 0);
        let mut b = b;
        b.insert_bytes(b"x");
        assert_ne!(b.flags & BFCHG, 0);
        assert_eq!(b.flags & BFINVS, 0);
    }

    #[test]
    fn encoded_roundtrip() {
        let sjis = encoding::from_utf8("日本語".as_bytes(), Code::Sjis);
        let b = Buffer::from_encoded("t", None, &sjis, Code::Sjis, Eol::Lf);
        assert_eq!(b.text(), "日本語".as_bytes());
        assert_eq!(b.encoded_text(), sjis);
        let mut b2 = b.clone();
        b2.goto_eob();
        b2.insert_bytes(b"!");
        assert_eq!(
            b2.encoded_text(),
            encoding::from_utf8("日本語!".as_bytes(), Code::Sjis)
        );
    }

    #[test]
    fn terminal_line_sentinel() {
        let mut b = Buffer::new("test");
        b.insert_bytes(b"a\nb\n");
        assert_eq!(b.lines.len(), 3);
        assert_eq!(b.b_linep_index(), 2);
        assert_eq!(b.real_line_count(), 2);
        assert_eq!(b.line_or_sentinel(0), b"a");
        assert_eq!(b.line_or_sentinel(1), b"b");
        assert_eq!(b.line_or_sentinel(2), b.b_linep());
        assert_eq!(b.line_or_sentinel(999), b.b_linep());
        assert_eq!(b.text(), b"a\nb\n");
    }

    #[test]
    fn crlf_roundtrip() {
        let raw = b"one\r\ntwo\r\n";
        let b = Buffer::from_encoded("t", None, raw, Code::Utf8, Eol::Crlf);
        assert_eq!(b.text(), b"one\ntwo\n");
        assert_eq!(b.encoded_text(), raw);
    }

    #[test]
    fn from_bytes_roundtrip() {
        for src in [
            &b""[..],
            &b"abc"[..],
            &b"abc\n"[..],
            &b"a\nbb\nccc\n"[..],
            "日本語\n😀て스트\n".as_bytes(),
        ] {
            let b = Buffer::from_bytes("t", None, src);
            assert_eq!(b.text(), src, "roundtrip failed for {:?}", src);
        }
    }

    #[test]
    fn insert_and_newline() {
        let mut b = Buffer::new("t");
        b.insert_bytes(b"abc");
        assert_eq!(b.dot, Pos::new(0, 3));
        b.newline();
        assert_eq!(b.dot, Pos::new(1, 0));
        b.insert_bytes(b"xy");
        assert_eq!(b.text(), b"abc\nxy");
        b.backward_char();
        b.insert_bytes(b"Z");
        assert_eq!(b.text(), b"abc\nxZy");
    }

    #[test]
    fn delete_across_lines_and_kill() {
        let mut b = Buffer::from_bytes("t", None, b"abc\ndef");
        b.goto_bob();
        let (ok, killed) = b.delete(5, true);
        assert!(ok);
        assert_eq!(killed, b"abc\nd");
        assert_eq!(b.text(), b"ef");
    }

    #[test]
    fn utf8_char_navigation() {
        let mut b = Buffer::from_bytes("t", None, "あA😀B".as_bytes());
        b.goto_bob();
        assert_eq!(b.char_len_at(0, 0), 3);
        b.forward_char();
        assert_eq!(b.dot.off, 3);
        b.forward_char();
        assert_eq!(b.dot.off, 4);
        b.forward_char();
        assert_eq!(b.dot.off, 8);
        b.backward_char();
        assert_eq!(b.dot.off, 4);
    }

    #[test]
    fn undo_redo_restore_text_and_cursor() {
        let mut b = Buffer::new("t");
        b.insert_bytes(b"one");
        let before = b.snapshot();
        b.insert_bytes(b"two");
        b.record_undo(before);
        assert_eq!(b.text(), b"onetwo");
        assert!(b.undo());
        assert_eq!(b.text(), b"one");
        assert!(b.redo());
        assert_eq!(b.text(), b"onetwo");
    }

    #[test]
    fn region_copy_and_replace() {
        let mut b = Buffer::from_bytes("t", None, b"0123456789");
        b.goto_bob();
        b.dot = Pos::new(0, 2);
        b.mark = Some(Pos::new(0, 6));
        assert_eq!(b.region_bytes().unwrap(), b"2345");
        b.replace_region(b"XX");
        assert_eq!(b.text(), b"01XX6789");
    }
}
