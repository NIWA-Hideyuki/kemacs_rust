//! マウス操作: クリック / ドラッグによる範囲選択と、ホイールによるカーソル移動。
//!
//! 端末から届くマウスイベントは `MOUSEFLG` 付きの疑似キーへ変換され
//! (`TerminalIo::map_mouse`)、`input::run()` が `decode()` で元に戻して
//! `Editor::mouse_event()` へ渡す。キーボードマクロには記録しない。
//!
//! 座標のエンコード (32bit):
//!
//! ```text
//! bit 31..28  MOUSEFLG (0x1)
//! bit 27..24  種別タグ (1=ホイール 2=押下 3=ドラッグ 4=解放)
//! bit 23..12  行 (0 起点・12bit)
//! bit 11.. 0  列 (0 起点・12bit)
//! ```
//!
//! ホイールは座標を持たず、下位ビットに方向 (0=上, 1=下) を入れる。

use crate::buffer::{Pos, MDMDMOD};
use crate::editor::Editor;
use crate::keycode::{is_mouse, Ckey, MOUSEFLG};

const TAG_WHEEL: Ckey = 0x0100_0000;
const TAG_PRESS: Ckey = 0x0200_0000;
const TAG_DRAG: Ckey = 0x0300_0000;
const TAG_RELEASE: Ckey = 0x0400_0000;
const TAG_RPRESS: Ckey = 0x0500_0000;
const TAG_RDRAG: Ckey = 0x0600_0000;
const TAG_RRELEASE: Ckey = 0x0700_0000;
const TAG_MASK: Ckey = 0x0F00_0000;
const COORD_SHIFT: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    WheelUp,
    WheelDown,
    /// 左ボタン押下 (範囲選択の始点)。
    Press,
    /// 左ボタンのドラッグ。
    Drag,
    /// 左ボタン解放。
    Release,
    /// 右ボタン押下。
    RightPress,
    /// 右ボタンのドラッグ (解放時にコピー)。
    RightDrag,
    /// 右ボタン解放。ドラッグしていなければペースト。
    RightRelease,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub kind: MouseKind,
    /// テキスト領域の上端からの行 (0 起点)。
    pub row: u16,
    /// テキスト領域の左端からの表示桁 (0 起点)。
    pub col: u16,
}

/// 疑似キーへ変換する (const: `WHEELUP` などの定義にも使う)。
pub const fn encode(kind: MouseKind, row: u16, col: u16) -> Ckey {
    let tag = match kind {
        MouseKind::WheelUp | MouseKind::WheelDown => TAG_WHEEL,
        MouseKind::Press => TAG_PRESS,
        MouseKind::Drag => TAG_DRAG,
        MouseKind::Release => TAG_RELEASE,
        MouseKind::RightPress => TAG_RPRESS,
        MouseKind::RightDrag => TAG_RDRAG,
        MouseKind::RightRelease => TAG_RRELEASE,
    };
    let dir = if matches!(kind, MouseKind::WheelDown) {
        1
    } else {
        0
    };
    let (r, c) = if tag == TAG_WHEEL { (0, 0) } else { (row, col) };
    MOUSEFLG | tag | (((r as Ckey) & 0xFFF) << COORD_SHIFT) | ((c as Ckey) & 0xFFF) | dir
}

/// 疑似キーからマウスイベントへ復号する。MOUSEFLG が無ければ None。
pub fn decode(k: Ckey) -> Option<MouseEvent> {
    if !is_mouse(k) {
        return None;
    }
    let kind = match k & TAG_MASK {
        TAG_WHEEL => {
            if k & 1 == 0 {
                MouseKind::WheelUp
            } else {
                MouseKind::WheelDown
            }
        }
        TAG_PRESS => MouseKind::Press,
        TAG_DRAG => MouseKind::Drag,
        TAG_RELEASE => MouseKind::Release,
        TAG_RPRESS => MouseKind::RightPress,
        TAG_RDRAG => MouseKind::RightDrag,
        TAG_RRELEASE => MouseKind::RightRelease,
        _ => return None,
    };
    // ホイールは座標を持たず、下位ビットが方向。
    let (row, col) = if is_wheel(k) {
        (0, 0)
    } else {
        (((k >> COORD_SHIFT) & 0xFFF) as u16, (k & 0xFFF) as u16)
    };
    Some(MouseEvent { kind, row, col })
}

/// ホイールの疑似キーか。
pub fn is_wheel(k: Ckey) -> bool {
    is_mouse(k) && (k & TAG_MASK) == TAG_WHEEL
}

/// マウスイベントをエディタへ適用する。
pub fn handle(ed: &mut Editor, ev: MouseEvent) {
    match ev.kind {
        MouseKind::WheelUp | MouseKind::WheelDown => wheel(ed, ev.kind),
        MouseKind::Press => press(ed, ev.row, ev.col),
        MouseKind::Drag => drag(ed, ev.row, ev.col),
        MouseKind::Release => release(ed),
        MouseKind::RightPress => right_press(ed, ev.row, ev.col),
        MouseKind::RightDrag => right_drag(ed, ev.row, ev.col),
        MouseKind::RightRelease => right_release(ed),
    }
}

fn wheel(ed: &mut Editor, kind: MouseKind) {
    let lines = ed.wheel_lines.clamp(1, 50) as i32;
    let name = match kind {
        MouseKind::WheelUp => "wheel-up",
        _ => "wheel-down",
    };
    let _ = ed.execute_named(name, false, lines);
}

/// 左ボタン押下: カーソルを移動し、そこをマークにする。
fn press(ed: &mut Editor, row: u16, col: u16) {
    let Some(p) = pos_at(ed, row, col) else {
        return;
    };
    ed.buffer_mut().dot = p;
    ed.buffer_mut().mark = Some(p);
    ed.mouse_drag = true;
    ed.window_mut().flag = 0xff;
}

/// ドラッグ中: マークはanchoredしたまま、カーソルだけを追従させる。
fn drag(ed: &mut Editor, row: u16, col: u16) {
    if !ed.mouse_drag {
        return;
    }
    if let Some(p) = pos_at(ed, row, col) {
        ed.buffer_mut().dot = p;
        ed.window_mut().flag = 0xff;
    }
}

/// 左ボタン解放: ドラッグなら選択 (mark + dot) を残して ^C-W / M-W / ^Y で使う。
///
/// クリック (押下=解放で動いていない) では選択を作らない。ここで mark を
/// 残すと、位置決めのクリック直後のキー入力が選択範囲に取り込まれて
/// 反転表示されてしまうため。
fn release(ed: &mut Editor) {
    ed.mouse_drag = false;
    let (mark, dot) = {
        let b = ed.buffer();
        (b.mark, b.dot)
    };
    if mark == Some(dot) {
        ed.buffer_mut().mark = None;
    }
}

/// 右ボタン押下: ここを起点にする。直前の選択は右クリック=コピーに使うため控える。
fn right_press(ed: &mut Editor, row: u16, col: u16) {
    ed.mouse_rclip = ed.buffer().region_bytes();
    let Some(p) = pos_at(ed, row, col) else {
        return;
    };
    ed.buffer_mut().dot = p;
    ed.buffer_mut().mark = Some(p);
    ed.mouse_rdrag = true;
    ed.window_mut().flag = 0xff;
}

fn right_drag(ed: &mut Editor, row: u16, col: u16) {
    if !ed.mouse_rdrag {
        return;
    }
    if let Some(p) = pos_at(ed, row, col) {
        ed.buffer_mut().dot = p;
        ed.window_mut().flag = 0xff;
    }
}

/// 右ボタン解放。
///
/// - ドラッグした (押下位置と異なる) → その範囲をコピー。
/// - クリックで、直前に選択 (左ドラッグ等) があれば → その選択をコピーして選択を解除。
/// - クリックで選択がなければ → クリップボードの内容をドットへ貼り付け。
fn right_release(ed: &mut Editor) {
    ed.mouse_rdrag = false;
    let prior = ed.mouse_rclip.take().filter(|v| !v.is_empty());
    let b = ed.buffer();
    let anchor = match b.mark {
        Some(m) => m,
        None => return,
    };
    let dot = b.dot;
    if anchor != dot {
        // 右ドラッグ = コピー (端末クリップボードへも OSC 52 で転送される)。
        // コピーしたら選択は解除する (次の右クリックがペーストになるように)。
        let Some(bytes) = b.region_bytes() else {
            return;
        };
        copy_clipboard(ed, bytes);
        ed.buffer_mut().mark = None;
        return;
    }
    if let Some(bytes) = prior {
        // 既存の選択の上での右クリック = その選択をコピーし、選択を解除する。
        // 解除しないと次の右クリックもコピー扱いになり、ペーストに到達できない。
        copy_clipboard(ed, bytes);
        ed.buffer_mut().mark = None;
        return;
    }
    // 右クリックだけ = ペースト
    let text = crate::clipboard::paste();
    if text.is_empty() {
        ed.mlwrite("Clipboard is empty");
        return;
    }
    let n = text.len();
    if ed.insert_snippet(&text) {
        ed.buffer_mut().mark = None;
        ed.mlwrite(format!("[Pasted {} bytes]", n));
    }
}

fn copy_clipboard(ed: &mut Editor, bytes: Vec<u8>) {
    let n = bytes.len();
    crate::clipboard::copy(&bytes);
    ed.mlwrite(format!("[Copied {} bytes]", n));
}

/// 画面座標 (行, 表示桁) をバッファ位置へ変換する。
///
/// ガーター (行番号) 部分は 0 桁に丸め、画面下部の余白は最後の実データ行末に丸める。
/// モードライン / メッセージ行をクリックしたときは None。
pub fn pos_at(ed: &Editor, row: u16, col: u16) -> Option<Pos> {
    let w = ed.window();
    let rows = w.rows;
    if row as usize >= rows {
        return None;
    }
    let b = ed.buffer();
    if b.lines.is_empty() {
        return None;
    }
    let gutter = if ed.show_line_numbers {
        ed.line_number_width.min(8)
    } else {
        0
    };
    let mut li = w.top + row as usize;
    let last = b.b_linep_index().saturating_sub(1);
    if li > last {
        li = last;
    }
    let target = (col as isize - gutter as isize).max(0) as usize;
    let conceal = ed.md_conceal && b.mode & MDMDMOD != 0;
    let (_, map) =
        crate::display::render_line_concealed(b.line_or_sentinel(li), ed.tabsize, conceal);
    // map はバイト位置ごとの表示桁。クリックされたセルを「含む」文字の先頭
    // (最も若いバイト位置) を選ぶ。幅の途中にカーソルが乗ることを防ぐ。
    let mut off = map.len().saturating_sub(1);
    for (i, &c) in map.iter().enumerate() {
        if c >= target {
            off = i;
            break;
        }
    }
    Some(Pos::new(li, off))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ScriptedIo;

    #[test]
    fn encode_decode_roundtrip_with_coordinates() {
        for (kind, row, col) in [
            (MouseKind::Press, 0u16, 0u16),
            (MouseKind::Press, 3, 7),
            (MouseKind::Drag, 23, 45),
            (MouseKind::Release, 9, 9),
        ] {
            let ev = decode(encode(kind, row, col)).unwrap();
            assert_eq!(ev.kind, kind);
            assert_eq!((ev.row, ev.col), (row, col));
        }
        assert!(decode(encode(MouseKind::Press, 5000, 3)).is_some());
        // 12bit 分以上は切り捨てる。
        let ev = decode(encode(MouseKind::Drag, 4096, 2)).unwrap();
        assert_eq!((ev.row, ev.col), (0, 2));
        assert_eq!(decode(ctl_key()), None);
    }

    fn ctl_key() -> Ckey {
        crate::keycode::ctl(b'X')
    }

    /// クリップボードはプロセス全体で共有されるため、扱うテストを直列化する。
    static CLIP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn wheel_keys_carry_no_coordinates() {
        assert_eq!(
            decode(WHEELUP_ENC),
            Some(MouseEvent {
                kind: MouseKind::WheelUp,
                row: 0,
                col: 0
            })
        );
        assert_eq!(
            decode(WHEELDOWN_ENC),
            Some(MouseEvent {
                kind: MouseKind::WheelDown,
                row: 0,
                col: 0
            })
        );
        assert!(is_wheel(WHEELUP_ENC));
        assert!(!is_wheel(encode(MouseKind::Press, 0, 0)));
    }

    const WHEELUP_ENC: Ckey = crate::keycode::WHEELUP;
    const WHEELDOWN_ENC: Ckey = crate::keycode::WHEELDOWN;

    fn ed() -> Editor {
        let mut e = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        let mut text = Vec::new();
        for i in 0..20 {
            text.extend_from_slice(format!("line {:02} abcdefgh\n", i + 1).as_bytes());
        }
        e.buffer_mut().insert_bytes(&text);
        e.buffer_mut().goto_bob();
        e
    }

    #[test]
    fn pos_at_maps_screen_to_buffer() {
        let mut e = ed();
        e.show_line_numbers = true;
        e.line_number_width = 6; // ガーター 6 桁
                                 // ガター 6 + "line 01 " 8 => 表示桁 14 は 'a'。
        assert_eq!(pos_at(&e, 0, 14), Some(Pos::new(0, 8)));
        assert_eq!(pos_at(&e, 1, 14), Some(Pos::new(1, 8)));
        // ガター内は 0 桁目になる。
        assert_eq!(pos_at(&e, 0, 2), Some(Pos::new(0, 0)));
        // 行末より右は行末 ("line 01 abcdefgh" = 16 文字)。
        assert_eq!(pos_at(&e, 0, 79), Some(Pos::new(0, 16)));
        // モードライン / メッセージ行は無視。
        assert_eq!(pos_at(&e, 22, 5), None);
        assert_eq!(pos_at(&e, 23, 5), None);
        // 画面下端の余白は最終実データ行末。
        assert_eq!(pos_at(&e, 21, 79), Some(Pos::new(19, 16)));
    }

    #[test]
    fn pos_at_never_lands_mid_char_with_wide_chars() {
        let mut e = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        e.buffer_mut().insert_bytes("あいうえお".as_bytes());
        e.buffer_mut().goto_bob();
        // 「あいうえお」のバイト境界は 0,3,6,9,12,15。
        for col in 0..=10u16 {
            let p = pos_at(&e, 0, col).unwrap();
            assert!(
                [0usize, 3, 6, 9, 12, 15].contains(&p.off),
                "click col {} landed at byte offset {}",
                col,
                p.off
            );
        }
        // クリック位置に文字を挿入しても既存の文字が割れない。
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Press,
                row: 0,
                col: 5,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Release,
                row: 0,
                col: 5,
            },
        );
        e.insert_snippet(b"X");
        assert_eq!(e.buffer().text(), "あいうXえお".as_bytes());
    }

    #[test]
    fn press_drag_release_sets_mark_and_dot() {
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Press,
                row: 0,
                col: 3,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(0, 3));
        assert_eq!(e.buffer().mark, Some(Pos::new(0, 3)));
        assert!(e.mouse_drag);
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Drag,
                row: 2,
                col: 5,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(2, 5));
        assert_eq!(e.buffer().mark, Some(Pos::new(0, 3)));
        // 選択内容は mark..dot のバイト列として取れる。
        // line0 "line 01 abcdefgh"(0..15), \n(16), line1(17..32), \n(33), line2(34..)
        // => 3..(34+5) = "e 01 abcdefgh\nline 02 abcdefgh\nline "
        assert_eq!(
            e.buffer().region_bytes().unwrap(),
            b"e 01 abcdefgh\nline 02 abcdefgh\nline "
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Release,
                row: 2,
                col: 5,
            },
        );
        assert!(!e.mouse_drag);
        assert_eq!(e.buffer().mark, Some(Pos::new(0, 3)));
    }

    #[test]
    fn drag_without_press_is_ignored() {
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Drag,
                row: 4,
                col: 4,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(0, 0));
        assert_eq!(e.buffer().mark, None);
    }

    #[test]
    fn right_drag_copies_region_to_clipboard() {
        let _guard = CLIP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightPress,
                row: 0,
                col: 3,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightDrag,
                row: 2,
                col: 5,
            },
        );
        assert!(e.mouse_rdrag);
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightRelease,
                row: 2,
                col: 5,
            },
        );
        assert!(!e.mouse_rdrag);
        assert_eq!(
            crate::clipboard::paste(),
            b"e 01 abcdefgh\nline 02 abcdefgh\nline "
        );
    }

    #[test]
    fn right_click_on_left_selection_copies_then_next_right_click_pastes() {
        let _guard = CLIP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut e = ed();
        // 左ドラッグで 0 行目の 3 文字目から 2 行目の 5 文字目までを選択。
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Press,
                row: 0,
                col: 3,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Drag,
                row: 2,
                col: 5,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Release,
                row: 2,
                col: 5,
            },
        );
        assert!(e.buffer().mark.is_some());

        // 右クリック (動かさず) = 既存の選択をコピーし、選択を解除する。
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightPress,
                row: 4,
                col: 1,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightRelease,
                row: 4,
                col: 1,
            },
        );
        assert_eq!(
            crate::clipboard::paste(),
            b"e 01 abcdefgh\nline 02 abcdefgh\nline "
        );
        assert_eq!(e.buffer().mark, None);

        // もう一度右クリック = 今度はペースト。
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightPress,
                row: 8,
                col: 0,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightRelease,
                row: 8,
                col: 0,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(10, 5));
        let text = e.buffer().text();
        let line = String::from_utf8_lossy(&text)
            .lines()
            .nth(8)
            .unwrap()
            .to_string();
        assert!(line.starts_with("e 01 abcdefgh"), "pasted line: {:?}", line);
    }

    #[test]
    fn right_drag_still_copies_its_own_region() {
        let _guard = CLIP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightPress,
                row: 0,
                col: 3,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightDrag,
                row: 2,
                col: 5,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightRelease,
                row: 2,
                col: 5,
            },
        );
        assert_eq!(
            crate::clipboard::paste(),
            b"e 01 abcdefgh\nline 02 abcdefgh\nline "
        );
        // コピー後は選択が解除され、次の右クリックはペーストになる。
        assert_eq!(e.buffer().mark, None);
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightPress,
                row: 9,
                col: 1,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightRelease,
                row: 9,
                col: 1,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(11, 5));
    }

    #[test]
    fn right_drag_without_press_is_ignored() {
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::RightDrag,
                row: 1,
                col: 1,
            },
        );
        assert_eq!(e.buffer().dot, Pos::new(0, 0));
    }

    #[test]
    fn click_without_drag_clears_mark() {
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Press,
                row: 2,
                col: 3,
            },
        );
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::Release,
                row: 2,
                col: 3,
            },
        );
        // 位置決めのクリックだけで選択 (mark) を残さない。
        assert_eq!(e.buffer().dot, Pos::new(2, 3));
        assert_eq!(e.buffer().mark, None);
        // そのためキー入力しても反転表示されない。
        e.insert_snippet(b"Q");
        assert_eq!(e.buffer().text().iter().filter(|&&c| c == b'Q').count(), 1);
        assert_eq!(e.buffer().mark, None);
    }

    #[test]
    fn wheel_moves_cursor_by_wheel_lines() {
        let mut e = ed();
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::WheelDown,
                row: 0,
                col: 0,
            },
        );
        assert_eq!(e.buffer().dot.line, 3);
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::WheelUp,
                row: 0,
                col: 0,
            },
        );
        assert_eq!(e.buffer().dot.line, 0);
        // 先頭ではそれ以上上がらない。
        handle(
            &mut e,
            MouseEvent {
                kind: MouseKind::WheelUp,
                row: 0,
                col: 0,
            },
        );
        assert_eq!(e.buffer().dot.line, 0);
    }
}
