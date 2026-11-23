//! 端末入力 (crossterm) と画面出力、およびテスト用のスクリプト I/O。

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{self, Clear, ClearType};

use crate::display::{self, Frame};
use crate::editor::{Editor, EditorIo};
use crate::keycode::{code, ctl, meta, metac_ctl, spec, Ckey, DEL};

/// crossterm の Resize イベント受領フラグ。
static RESIZE_PENDING: AtomicBool = AtomicBool::new(false);

/// リサイズ通知用の疑似キー (編集ループが無視する)。
pub const RESIZE_KEY: Ckey = 0x0E00_0000;

pub struct TerminalIo {
    keys: Mutex<VecDeque<Ckey>>,
}

impl Default for TerminalIo {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalIo {
    pub fn new() -> Self {
        TerminalIo {
            keys: Mutex::new(VecDeque::new()),
        }
    }

    #[allow(clippy::redundant_guards)]
    fn next_key(&self) -> Ckey {
        if let Some(queued) = self
            .keys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
        {
            return queued;
        }
        loop {
            if RESIZE_PENDING.load(Ordering::Relaxed) {
                return RESIZE_KEY;
            }
            if !event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                if RESIZE_PENDING.load(Ordering::Relaxed) {
                    return RESIZE_KEY;
                }
                continue;
            }
            match event::read() {
                Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char(ch),
                    modifiers,
                    kind,
                    ..
                })) if kind != KeyEventKind::Release
                    && !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT)
                    && !ch.is_ascii() =>
                {
                    let mut buf = [0u8; 4];
                    let bytes = ch.encode_utf8(&mut buf).as_bytes().to_vec();
                    let mut q = self.keys.lock().unwrap_or_else(|e| e.into_inner());
                    for &b in bytes.iter().skip(1) {
                        q.push_back(b as Ckey);
                    }
                    return bytes[0] as Ckey;
                }
                Ok(Event::Key(KeyEvent {
                    code,
                    modifiers,
                    kind,
                    ..
                })) if kind != KeyEventKind::Release => {
                    return map_key(code, modifiers);
                }
                Ok(Event::Key(KeyEvent { code, .. })) if code == KeyCode::Null => {}
                Ok(Event::Paste(_)) => {}
                // マウス操作は提供しないが、キャプチャ自体は有効に保つ。
                // 解除すると端末がホイールをスクロールバックと解釈し、
                // エディタ描画外の領域が見えてしまうため、イベントは破棄する。
                Ok(Event::Mouse(_)) => {}
                Ok(Event::Resize(_, _)) => {
                    RESIZE_PENDING.store(true, Ordering::Relaxed);
                }
                Err(_) => return ctl(b'G'),
                _ => {}
            }
        }
    }
}

/// crossterm のキーを C 版のキーコードへ変換する (ebind.H_ の割当てに合わせる)。
pub fn map_key(code: KeyCode, modifiers: KeyModifiers) -> Ckey {
    let meta_mod = modifiers.contains(KeyModifiers::ALT);
    let ctrl_mod = modifiers.contains(KeyModifiers::CONTROL);
    let base = match code {
        KeyCode::Char(c) if ctrl_mod => ctl(c as u8),
        KeyCode::Char(c) if meta_mod => meta(c as u8),
        KeyCode::Char(c) if c.is_ascii() => c as u8 as Ckey,
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf);
            buf[0] as Ckey
        }
        KeyCode::Enter => ctl(b'M'),
        KeyCode::Tab => ctl(b'I'),
        KeyCode::Backspace => ctl(b'H'),
        KeyCode::Delete => DEL as Ckey,
        KeyCode::Esc => ctl(b'['),
        KeyCode::Up => spec(meta(b'A') as u8),
        KeyCode::Down => spec(meta(b'B') as u8),
        KeyCode::Right => spec(meta(b'C') as u8),
        KeyCode::Left => spec(meta(b'D') as u8),
        KeyCode::PageUp => metac_ctl(b'V'),
        KeyCode::PageDown => metac_ctl(b'Z'),
        KeyCode::Home => meta(b'<'),
        KeyCode::End => meta(b'>'),
        KeyCode::F(n) if n <= 12 => spec(b'0' + n),
        _ => 0,
    };
    if meta_mod && !matches!(code, KeyCode::Char(_)) {
        base | crate::keycode::META
    } else {
        base
    }
}

impl EditorIo for TerminalIo {
    fn prompt(&mut self, prompt: &str, default: Option<&str>) -> Option<String> {
        let mut buf = String::new();
        if let Some(d) = default {
            buf.push_str(d);
        }
        let mut out = io::stdout();
        let _ = crossterm::execute!(out, Show);
        loop {
            let _ = crossterm::queue!(
                out,
                MoveTo(
                    0,
                    terminal::size().map_or(24, |(_, r)| r).saturating_sub(1) as u16
                ),
                Print(format!("{}{}", prompt, buf)),
                Clear(ClearType::UntilNewLine)
            );
            let _ = out.flush();
            let k = self.next_key();
            if k == ctl(b'G') {
                return None;
            }
            if k == ctl(b'M') || k == b'\r' as Ckey || k == b'\n' as Ckey {
                break;
            }
            if k == ctl(b'H') || k == 8 || k == 127 {
                buf.pop();
                continue;
            }
            let c = code(k);
            if c >= 0x20 {
                if let Some(ch) = char::from_u32(c as u32) {
                    buf.push(ch);
                }
            }
        }
        Some(buf)
    }

    fn prompt_secret(&mut self, prompt: &str) -> Option<String> {
        let mut buf = String::new();
        let mut out = io::stdout();
        let _ = crossterm::execute!(out, Show);
        loop {
            let _ = crossterm::queue!(
                out,
                MoveTo(
                    0,
                    terminal::size().map_or(24, |(_, r)| r).saturating_sub(1) as u16
                ),
                Print(prompt),
                Clear(ClearType::UntilNewLine)
            );
            let _ = out.flush();
            let k = self.next_key();
            if k == ctl(b'G') {
                return None;
            }
            if k == ctl(b'M') || k == b'\r' as Ckey || k == b'\n' as Ckey {
                break;
            }
            if k == ctl(b'H') || k == 8 || k == 127 {
                buf.pop();
                continue;
            }
            let c = code(k);
            if c >= 0x20 {
                if let Some(ch) = char::from_u32(c as u32) {
                    buf.push(ch);
                }
            }
        }
        Some(buf)
    }

    fn yesno(&mut self, prompt: &str) -> bool {
        let mut out = io::stdout();
        let _ = crossterm::execute!(out, Show);
        let _ = crossterm::queue!(
            out,
            MoveTo(
                0,
                terminal::size().map_or(24, |(_, r)| r).saturating_sub(1) as u16
            ),
            Print(format!("{} (y or n)? ", prompt)),
            Clear(ClearType::UntilNewLine)
        );
        let _ = out.flush();
        let k = self.next_key();
        matches!(code(k), b'y' | b'Y')
    }

    fn read_key(&mut self) -> Ckey {
        self.next_key()
    }

    fn suspend_terminal(&mut self) {
        let _ = terminal::disable_raw_mode();
    }

    fn resume_terminal(&mut self) {
        let _ = terminal::enable_raw_mode();
    }
}

/// 画面を描画してカーソルを目的位置へ移動する。
/// 前フレームとの差分だけを端末へ送出する (C 版 update() 相当)。
pub fn draw_diff(prev: Option<&Frame>, frame: &Frame) -> io::Result<()> {
    let mut out = io::stdout();
    let full = display::needs_full_redraw(prev, frame);
    let fg = color_of(frame.fg);
    let bg = color_of(frame.bg);
    if full {
        crossterm::queue!(out, Hide)?;
        if frame.color_on {
            crossterm::queue!(out, SetForegroundColor(fg), SetBackgroundColor(bg))?;
        }
    }
    for r in display::changed_rows(prev, frame) {
        crossterm::queue!(
            out,
            MoveTo(0, r as u16),
            Print(frame.rows[r].clone()),
            Clear(ClearType::UntilNewLine)
        )?;
    }
    let rows = frame.rows.len();
    let modeline_changed = full || prev.is_none_or(|p| p.modeline != frame.modeline);
    if modeline_changed {
        // C 版と同じくモードラインは常に反転表示 (SGR 7)。
        // C 版の w_toprow + w_ntrows に相当する行はフレーム末尾の next 行。
        // フレーム最終行 (rows - 1) は通常のテキスト行なので、そこへ
        // モードラインを書くとスクロール時にテキストへ上書きされて消える。
        crossterm::queue!(out, SetAttribute(Attribute::Reverse))?;
        crossterm::queue!(
            out,
            MoveTo(0, rows as u16),
            Print(frame.modeline.clone()),
            Clear(ClearType::UntilNewLine)
        )?;
        // 反転を解除してからメッセージ行/カーソルを通常属性で描く。
        crossterm::queue!(out, SetAttribute(Attribute::Reset))?;
        if frame.color_on {
            crossterm::queue!(out, SetForegroundColor(fg), SetBackgroundColor(bg))?;
        }
    }
    if full || prev.is_none_or(|p| p.message != frame.message) {
        crossterm::queue!(
            out,
            MoveTo(0, (rows + 1) as u16),
            Print(frame.message.clone()),
            Clear(ClearType::UntilNewLine)
        )?;
    }
    if frame.color_on && !modeline_changed {
        crossterm::queue!(out, ResetColor)?;
    }
    if full
        || prev.is_none_or(|p| p.cursor_row != frame.cursor_row || p.cursor_col != frame.cursor_col)
    {
        crossterm::queue!(
            out,
            MoveTo(frame.cursor_col as u16, frame.cursor_row as u16)
        )?;
    }
    crossterm::queue!(out, Show)?;
    out.flush()
}

/// 互換ラッパー (全行描画)。
pub fn draw(frame: &Frame) -> io::Result<()> {
    draw_diff(None, frame)
}

pub fn enable() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    // マウス操作機能は提供しないが、キャプチャは有効にしておく。
    // これを外すと端末がホイールをスクロールバックとして扱い、
    // エディタ描画外のバッファが見えてしまう。
    crossterm::execute!(io::stdout(), EnableMouseCapture)
}

pub fn disable() -> io::Result<()> {
    let _ = crossterm::execute!(io::stdout(), DisableMouseCapture);
    terminal::disable_raw_mode()
}

pub fn terminal_size() -> (usize, usize) {
    terminal::size()
        .map(|(c, r)| (r as usize, c as usize))
        .unwrap_or((24, 80))
}

/// 端末サイズ変更をエディタ状態へ反映する (C 版 `sigwinch` 相当)。
pub fn apply_resize(ed: &mut Editor, rows: usize, cols: usize) {
    ed.screen_rows = rows.max(3);
    ed.screen_cols = cols.max(1);
    let n = ed.windows.len().max(1);
    let body = ed.screen_rows.saturating_sub(2).max(1);
    let each = body / n;
    for (i, w) in ed.windows.iter_mut().enumerate() {
        w.rows = if i + 1 == n {
            body.saturating_sub(each * (n - 1)).max(1)
        } else {
            each.max(1)
        };
        w.flag = 0xff;
    }
    ed.window_pos.resize(ed.windows.len(), Default::default());
}

/// 編集ループ: 入力を読み、プレフィックスを処理し、描画する。
pub fn run(ed: &mut Editor) -> io::Result<()> {
    let mut prev: Option<Frame> = None;
    while !ed.quit {
        if RESIZE_PENDING.swap(false, Ordering::Relaxed) {
            let (rows, cols) = terminal_size();
            apply_resize(ed, rows, cols);
            prev = None; // リサイズ時は全行を描き直す
        }
        ed.preview_poll();
        let f = display::frame(ed);
        draw_diff(prev.as_ref(), &f)?;
        prev = Some(f);
        let k = ed.read_key();
        if k == RESIZE_KEY {
            continue;
        }
        if let Some((k2, f, n)) = ed.feed_key(k) {
            ed.record_key(k2, f, n);
            let _ = ed.execute(k2, f, n);
            if k2 != ctl(b'G') {
                ed.mpresf = false;
            }
        }
    }
    Ok(())
}

/// テスト用のスクリプト I/O。プロンプトへの回答とキーを事前に設定する。
#[derive(Default)]
pub struct ScriptedIo {
    pub prompts: std::collections::VecDeque<String>,
    pub answers: std::collections::VecDeque<bool>,
    pub keys: std::collections::VecDeque<Ckey>,
}

impl ScriptedIo {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_prompts(v: &[&str]) -> Self {
        ScriptedIo {
            prompts: v.iter().map(|s| s.to_string()).collect(),
            answers: std::collections::VecDeque::new(),
            keys: std::collections::VecDeque::new(),
        }
    }
}

impl EditorIo for ScriptedIo {
    fn prompt(&mut self, _prompt: &str, _default: Option<&str>) -> Option<String> {
        self.prompts.pop_front()
    }
    fn yesno(&mut self, _prompt: &str) -> bool {
        self.answers.pop_front().unwrap_or(false)
    }
    fn read_key(&mut self) -> Ckey {
        self.keys.pop_front().unwrap_or(ctl(b'G'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_ctrl_and_meta() {
        assert_eq!(
            map_key(KeyCode::Char('a'), KeyModifiers::CONTROL),
            ctl(b'A')
        );
        assert_eq!(map_key(KeyCode::Char('f'), KeyModifiers::ALT), meta(b'f'));
        assert_eq!(map_key(KeyCode::Enter, KeyModifiers::NONE), ctl(b'M'));
        assert_eq!(map_key(KeyCode::Backspace, KeyModifiers::NONE), ctl(b'H'));
        assert_eq!(
            map_key(KeyCode::Up, KeyModifiers::NONE),
            spec(meta(b'A') as u8)
        );
    }
}

fn color_of(spec: crate::editor::ColorSpec) -> Color {
    match spec {
        crate::editor::ColorSpec::Default => Color::Reset,
        crate::editor::ColorSpec::Ansi(n) => Color::AnsiValue(n),
        crate::editor::ColorSpec::Rgb(r, g, b) => Color::Rgb { r, g, b },
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn apply_resize_updates_window_rows() {
        let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
        apply_resize(&mut ed, 40, 100);
        assert_eq!(ed.screen_rows, 40);
        assert_eq!(ed.screen_cols, 100);
        assert_eq!(ed.window().rows, 38);
    }
}
