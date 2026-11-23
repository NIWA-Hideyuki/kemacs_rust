//! キーコード定義。C 版 `estruct.H_` の CTLFLG/META/CTLX/SPEC と同じ値を使う。

/// キーストローク (Char + フラグ)。C 版の `Ckey` (unsigned long) と同じ 32bit。
pub type Ckey = u32;

pub const CTLFLG: Ckey = 0x0100_0000;
pub const META: Ckey = 0x0200_0000;
pub const CTLX: Ckey = 0x0400_0000;
pub const SPEC: Ckey = 0x0800_0000;
pub const CK_FLAGS: Ckey = CTLFLG | META | CTLX | SPEC;

pub const BELL: u8 = 0x07;
pub const TAB: u8 = 0x09;
pub const ESC: u8 = 0x1b;
pub const DEL: u8 = 0x7f;

pub const TRUE: Status = Status::True;
pub const FALSE: Status = Status::False;
pub const ABORT: Status = Status::Abort;

/// C 版の戻り値 (TRUE/FALSE/ABORT/FAILED/RET/GOLINE) に対応。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    True,
    False,
    Abort,
    Failed,
    Ret,
    Goline,
}

impl Status {
    pub fn is_true(self) -> bool {
        self == Status::True
    }
    pub fn as_int(self) -> i32 {
        match self {
            Status::True => 1,
            Status::False => 0,
            Status::Abort => 2,
            Status::Failed => 3,
            Status::Ret => 4,
            Status::Goline => 5,
        }
    }
}

/// CTLFLG | 'A' のようなキーを作る (大文字化は C 版の ctoec() と同じ)。
pub const fn ctl(c: u8) -> Ckey {
    CTLFLG | (c.to_ascii_uppercase() as Ckey)
}
pub const fn meta(c: u8) -> Ckey {
    META | (c as Ckey)
}
pub const fn ctlx(c: u8) -> Ckey {
    CTLX | (c as Ckey)
}
pub const fn ctlxctl(c: u8) -> Ckey {
    CTLX | CTLFLG | (c.to_ascii_uppercase() as Ckey)
}
pub const fn ctlxlctl(c: u8) -> Ckey {
    CTLX | CTLFLG | (c.to_ascii_uppercase() as Ckey)
}
pub const fn metac_ctl(c: u8) -> Ckey {
    META | CTLFLG | (c.to_ascii_uppercase() as Ckey)
}
pub const fn spec(c: u8) -> Ckey {
    SPEC | (c as Ckey)
}

/// キーに付いているフラグだけを返す。
pub fn flags(k: Ckey) -> Ckey {
    k & CK_FLAGS
}
/// フラグを除いた文字コード。
pub fn code(k: Ckey) -> u8 {
    (k & !CK_FLAGS) as u8
}
/// 文字部分だけ比較する (C 版の `ectoc()` 相当)。
pub fn bare(k: Ckey) -> Ckey {
    k & !CK_FLAGS
}

/// C 版 `cmdstr()` と同じ形式 ("C-A", "M-f", "^X C-S", "SPEC M-A" 等) に変換する。
pub fn cmdstr(k: Ckey) -> String {
    let mut s = String::new();
    if k & META != 0 {
        s.push_str("M-");
    }
    if k & CTLX != 0 {
        s.push_str("^X ");
    }
    if k & CTLFLG != 0 {
        s.push_str("C-");
    }
    if k & SPEC != 0 {
        s.push_str("SPEC ");
    }
    let c = code(k);
    if c == 0 {
        return s.trim_end().to_string();
    }
    if (0x20..0x7f).contains(&c) {
        let ch = c as char;
        if k & CTLFLG != 0 {
            s.push(ch.to_ascii_uppercase());
        } else {
            s.push(ch);
        }
    } else {
        s.push_str(&format!("<{}>", c));
    }
    s
}

/// ユーザー入力を表す補助型 (テストやキーシーケンス入力で使用)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Alt(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Esc,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    F(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_codes_match_c_version() {
        assert_eq!(ctl(b'A'), CTLFLG | b'A' as Ckey);
        assert_eq!(ctlxctl(b'S'), CTLX | CTLFLG | b'S' as Ckey);
        assert_eq!(ctl(b'a'), ctl(b'A'));
        assert_eq!(code(ctl(b'X')), b'X');
        assert_eq!(flags(ctlx(b'l')), CTLX);
    }

    #[test]
    fn cmdstr_format() {
        assert_eq!(cmdstr(ctl(b'A')), "C-A");
        assert_eq!(cmdstr(ctlxctl(b'S')), "^X C-S");
        assert_eq!(cmdstr(meta(b'f')), "M-f");
        assert_eq!(cmdstr(DEL as Ckey), "<127>");
    }
}
