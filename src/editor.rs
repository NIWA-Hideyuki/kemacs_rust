//! エディタ全体の状態とコマンドディスパッチ。C 版 `main.C_` の `recedit()` /
//! `execute()` と `edef.H_` のグローバル変数に相当する。

use std::collections::{HashMap, VecDeque};

use crate::buffer::{Buffer, Snapshot};
use crate::commands;
use crate::encoding::{Code, Eol};
pub use crate::keycode::Status;
use crate::keycode::{ctl, ctlx, ctlxlctl, Ckey, CTLFLG, META};
use crate::system::PreviewState;

/// 画面 I/O の抽象化。端末実装 (`input.rs`) とテスト用スクリプト実装を差し替える。
pub trait EditorIo {
    /// ミニバッファから 1 行入力する。None は C-G による中止。
    fn prompt(&mut self, prompt: &str, default: Option<&str>) -> Option<String>;
    /// パスワードなど、入力文字を画面に表示しないプロンプト。
    fn prompt_secret(&mut self, prompt: &str) -> Option<String> {
        self.prompt(prompt, None)
    }
    /// yes/no を尋ねる。
    fn yesno(&mut self, prompt: &str) -> bool;
    /// 生のキーストロークを 1 つ読む (プレフィックス処理は Editor 側)。
    fn read_key(&mut self) -> Ckey;
    /// ESC も確定キーとして受け付けるプロンプト (検索・置換用)。
    fn prompt_accept_esc(&mut self, prompt: &str, default: Option<&str>) -> Option<String> {
        self.prompt(prompt, default)
    }
    /// 指定時間だけ待ってキーを読む。無ければ None。
    /// プレビュー出力を待つ間も描画を更新するために使う。
    fn read_key_timeout(&mut self, _timeout: std::time::Duration) -> Option<Ckey> {
        Some(self.read_key())
    }
    /// 外部プロセス実行中は raw mode を解除する。
    fn suspend_terminal(&mut self) {}
    fn resume_terminal(&mut self) {}
}

#[derive(Clone, Debug)]
pub struct Window {
    pub buf: usize,
    /// 表示の先頭行 (0 起点)。
    pub top: usize,
    pub rows: usize,
    pub flag: u8,
    pub force: u8,
}

/// 端末色 (8/256 色の Ansi 値、または truecolor の RGB)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpec {
    Default,
    Ansi(u8),
    Rgb(u8, u8, u8),
}

impl ColorSpec {
    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("default") || s == "0" {
            return ColorSpec::Default;
        }
        let parts: Vec<u8> = s
            .split(',')
            .filter_map(|v| v.trim().parse::<u8>().ok())
            .collect();
        match parts.as_slice() {
            [r, g, b] => ColorSpec::Rgb(*r, *g, *b),
            [n] => ColorSpec::Ansi(*n),
            _ => ColorSpec::Default,
        }
    }

    pub fn as_var(self) -> String {
        match self {
            ColorSpec::Default => "default".to_string(),
            ColorSpec::Ansi(n) => n.to_string(),
            ColorSpec::Rgb(r, g, b) => format!("{},{},{}", r, g, b),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KbdMode {
    #[default]
    Stop,
    Record,
    Play,
}

#[derive(Clone, Debug, Default)]
pub struct ArgState {
    value: i32,
    digits: String,
    negative: bool,
}

/// スクリプトの !if / !else の入れ子状態。
#[derive(Clone, Copy, Debug)]
pub struct IfFrame {
    pub parent_active: bool,
    pub cond: bool,
    pub in_else: bool,
}

impl IfFrame {
    pub fn active(&self) -> bool {
        self.parent_active && if self.in_else { !self.cond } else { self.cond }
    }
}

pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub windows: Vec<Window>,
    pub cur_win: usize,
    pub io: Box<dyn EditorIo>,
    pub keymap: HashMap<Ckey, &'static str>,
    pub kill: Vec<u8>,
    pub message: String,
    pub mpresf: bool,
    pub thisflag: i32,
    pub lastflag: i32,
    pub lastchar: Ckey,
    pub quit: bool,
    pub prefix_ctlx: bool,
    pub prefix_meta: bool,
    pub arg_state: Option<ArgState>,
    pub show_line_numbers: bool,
    pub line_number_width: usize,
    pub fillcol: usize,
    pub screen_rows: usize,
    pub screen_cols: usize,
    pub search_pattern: Vec<u8>,
    pub search_reverse: bool,
    pub mode_line_format: String,
    /// C-P / C-N の縦移動で保持する目的列。
    pub curgoal: usize,
    /// ウィンドウ×バッファごとのカーソル位置 (C 版 w_dotp/w_doto 相当)。
    pub window_pos: Vec<HashMap<usize, (crate::buffer::Pos, Option<crate::buffer::Pos>)>>,
    /// 最後に位置を保存した (ウィンドウ, バッファ)。
    pub last_saved_win_buf: Option<(usize, usize)>,
    /// グローバルモード (C 版 gmode 相当、全バッファに適用)。
    pub global_mode: u32,
    /// TAB の幅。0 はタブ文字を挿入する C 版の動作。
    pub tabsize: usize,
    /// マウスホイール 1 ノッチで移動する行数 ($wheel-lines)。
    pub wheel_lines: usize,
    /// 記録中のキーボードマクロ (キー, 引数フラグ, 回数)。
    pub kbdm: Vec<(Ckey, bool, i32)>,
    pub kbdmode: KbdMode,
    pub kbdrep: usize,
    /// スクリプトからコマンドへ渡す引数トークン。
    pub macro_args: VecDeque<String>,
    pub user_vars: HashMap<String, String>,
    /// 実行中のマクロを書き込むバッファ名 (Some の間は docmd が記録する)。
    pub mstore: Option<String>,
    pub execlevel: i32,
    pub if_stack: Vec<IfFrame>,
    /// スクリプト実行中もミニバッファへ対話的に尋ねる (再帰プロンプト)。
    pub script_interactive: bool,
    /// 現在のプロンプト入れ子深さ (デバッグ/再帰制御用)。
    pub prompt_depth: usize,
    /// dobuf / execproc の再帰深さ (スタック枯渇防止)。
    pub script_depth: usize,
    pub golabel: Option<String>,
    pub in_macro_play: bool,
    pub rng_seed: i64,
    /// 新規バッファの既定コード ($fcode 相当)。
    pub default_code: Code,
    pub default_eol: Eol,
    /// $asave / $acount 相当 (文字数と残りカウント)。
    pub asave: usize,
    pub acount: usize,
    pub preview: Option<PreviewState>,
    pub fg: ColorSpec,
    pub bg: ColorSpec,
    pub color_on: bool,
    // C Mode 設定 (edef.H_ の c_* 相当)
    pub c_indent_width: usize,
    pub c_use_spaces: bool,
    pub c_brace_style: i32,
    pub c_auto_detect: bool,
    pub c_follow_source: bool,
    pub c_gnu_indent: usize,
    pub c_brace_func: bool,
    pub c_cont_indent: usize,
    pub c_case_indent: usize,
    pub c_pp_indent: usize,
    pub c_auto_close: bool,
    // Python Mode 設定
    pub py_indent_width: usize,
    pub py_use_spaces: bool,
    // Markdown Mode 設定
    pub md_indent_width: usize,
    pub md_use_spaces: bool,
    pub md_auto_list: bool,
    pub md_heading_max: usize,
    pub md_conceal: bool,
    pub md_auto_pair: bool,
}

impl Editor {
    pub fn new(io: Box<dyn EditorIo>, rows: usize, cols: usize) -> Self {
        let mut buffers = Vec::new();
        let mut scratch = Buffer::new("main");
        scratch.undo_limit = 0;
        buffers.push(scratch);
        let mut keymap = HashMap::new();
        for (k, name) in commands::default_bindings() {
            keymap.insert(k, name);
        }
        let rows = rows.max(3);
        let cols = cols.max(20);
        Editor {
            buffers,
            windows: vec![Window {
                buf: 0,
                top: 0,
                rows: rows - 2,
                flag: 0xff,
                force: 0,
            }],
            cur_win: 0,
            io,
            keymap,
            kill: Vec::new(),
            message: String::new(),
            mpresf: false,
            thisflag: 0,
            lastflag: 0,
            lastchar: 0,
            quit: false,
            prefix_ctlx: false,
            prefix_meta: false,
            arg_state: None,
            show_line_numbers: false,
            line_number_width: 6,
            fillcol: 72,
            screen_rows: rows,
            screen_cols: cols,
            search_pattern: Vec::new(),
            search_reverse: false,
            mode_line_format: "== %p %v (%m) == %b == File: %F == %c %-".to_string(),
            curgoal: 0,
            window_pos: vec![HashMap::new()],
            last_saved_win_buf: Some((0, 0)),
            global_mode: 0,
            tabsize: 8,
            wheel_lines: 3,
            kbdm: Vec::new(),
            kbdmode: KbdMode::Stop,
            kbdrep: 0,
            macro_args: VecDeque::new(),
            user_vars: HashMap::new(),
            mstore: None,
            execlevel: 0,
            if_stack: Vec::new(),
            script_interactive: false,
            prompt_depth: 0,
            script_depth: 0,
            golabel: None,
            in_macro_play: false,
            rng_seed: 0,
            default_code: Code::Utf8,
            default_eol: Eol::Lf,
            asave: 256,
            acount: 256,
            preview: None,
            fg: ColorSpec::Default,
            bg: ColorSpec::Default,
            color_on: false,
            c_indent_width: 4,
            c_use_spaces: false,
            c_brace_style: 0,
            c_auto_detect: false,
            c_follow_source: false,
            c_gnu_indent: 2,
            c_brace_func: false,
            c_cont_indent: 2,
            c_case_indent: 0,
            c_pp_indent: 0,
            c_auto_close: true,
            py_indent_width: 4,
            py_use_spaces: true,
            md_indent_width: 2,
            md_use_spaces: true,
            md_auto_list: true,
            md_heading_max: 6,
            md_conceal: false,
            md_auto_pair: false,
        }
    }

    pub fn buffer_index(&self) -> usize {
        self.windows[self.cur_win].buf
    }
    pub fn buffer(&self) -> &Buffer {
        &self.buffers[self.buffer_index()]
    }
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        let i = self.buffer_index();
        &mut self.buffers[i]
    }
    pub fn window(&self) -> &Window {
        &self.windows[self.cur_win]
    }
    pub fn window_mut(&mut self) -> &mut Window {
        &mut self.windows[self.cur_win]
    }

    pub fn find_buffer(&self, name: &str) -> Option<usize> {
        self.buffers.iter().position(|b| b.name == name)
    }

    /// C 版 `bfind()` に相当。存在しなければ作成する。
    pub fn find_buffer_or_create(&mut self, name: &str) -> usize {
        if let Some(i) = self.find_buffer(name) {
            return i;
        }
        let mut b = Buffer::new(name);
        b.flags &= !1; // BFINVS を外す
        b.undo_limit = self.buffers[0].undo_limit;
        self.buffers.push(b);
        self.buffers.len() - 1
    }

    /// 現在のバッファの dot/mark を現在ウィンドウの位置として保存する。
    pub fn save_window_pos(&mut self) {
        self.window_pos.resize(self.windows.len(), HashMap::new());
        let wi = self.cur_win.min(self.windows.len().saturating_sub(1));
        let bi = self.windows[wi].buf;
        let b = &self.buffers[bi];
        self.window_pos[wi].insert(bi, (b.dot, b.mark));
        self.last_saved_win_buf = Some((wi, bi));
    }

    /// 現在ウィンドウ×バッファの保存位置をバッファへ反映する。
    pub fn restore_window_pos(&mut self) {
        if self.windows.is_empty() {
            return;
        }
        let wi = self.cur_win.min(self.windows.len() - 1);
        let bi = self.windows[wi].buf;
        if self.last_saved_win_buf == Some((wi, bi)) {
            return;
        }
        if let Some(&(dot, mark)) = self.window_pos.get(wi).and_then(|m| m.get(&bi)) {
            let b = &mut self.buffers[bi];
            b.dot = dot;
            b.mark = mark;
        }
        self.last_saved_win_buf = Some((wi, bi));
    }

    /// ウィンドウを切り替える (旧ウィンドウの位置を保存し、新ウィンドウの位置を復元)。
    pub fn set_cur_win(&mut self, wi: usize) {
        if wi >= self.windows.len() {
            return;
        }
        self.save_window_pos();
        self.cur_win = wi;
        self.restore_window_pos();
    }

    /// 現在ウィンドウが表示するバッファを切り替える。
    pub fn set_window_buffer(&mut self, bi: usize) {
        if bi >= self.buffers.len() {
            return;
        }
        self.save_window_pos();
        self.windows[self.cur_win].buf = bi;
        self.windows[self.cur_win].top = 0;
        self.restore_window_pos();
    }

    pub fn mlwrite(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.mpresf = true;
    }

    /// メッセージ行を消す (C 版 mlerase() 相当)。
    pub fn clear_message(&mut self) {
        self.message.clear();
        self.mpresf = false;
    }

    /// 外部コマンド実行の前後で端末モードを切り替える。
    pub fn suspend_terminal(&mut self) {
        self.io.suspend_terminal();
    }
    pub fn resume_terminal(&mut self) {
        self.io.resume_terminal();
    }

    /// 1 文字挿入ごとの自動保存チェック (C 版 execute() の ASAVE 部分)。
    pub fn autosave_tick(&mut self) {
        if self.acount == 0 {
            return;
        }
        self.acount -= 1;
        if self.acount == 0 {
            crate::system::autosave_current(self);
            self.acount = self.asave;
        }
    }

    /// メインループから呼ぶ非同期プレビューのポール。
    pub fn preview_poll(&mut self) {
        crate::system::preview_poll(self);
    }

    /// I/O 実装から生キーを 1 つ読む。
    pub fn read_key(&mut self) -> Ckey {
        self.io.read_key()
    }

    /// マウスホイールの疑似キーを (コマンド名, 移動行数) に変換する。
    /// ホイールでなければ None。`run` ループからプレフィックス処理や
    /// マクロ記録を挟まず直接 `execute_named` する。
    pub fn wheel_command(&self, k: Ckey) -> Option<(&'static str, i32)> {
        if !crate::keycode::is_wheel(k) {
            return None;
        }
        let lines = self.wheel_lines.clamp(1, 50) as i32;
        if k == crate::keycode::WHEELUP {
            Some(("wheel-up", -lines))
        } else {
            Some(("wheel-down", lines))
        }
    }

    /// コマンドからの入力要求。スクリプト実行中は引数トークンを返す。
    pub fn prompt(&mut self, prompt: &str, default: Option<&str>) -> Option<String> {
        self.prompt_depth += 1;
        let answer = if let Some(a) = self.macro_args.pop_front() {
            Some(a)
        } else {
            self.io.prompt(prompt, default)
        };
        self.prompt_depth -= 1;
        answer
    }

    /// 検索・置換のキーワード入力。ESC でも確定できる。
    pub fn prompt_accept_esc(&mut self, prompt: &str, default: Option<&str>) -> Option<String> {
        self.prompt_depth += 1;
        let answer = if let Some(a) = self.macro_args.pop_front() {
            Some(a)
        } else {
            self.io.prompt_accept_esc(prompt, default)
        };
        self.prompt_depth -= 1;
        answer
    }

    /// パスワード入力。端末では文字をエコーしない。
    pub fn prompt_secret(&mut self, prompt: &str) -> Option<String> {
        self.prompt_depth += 1;
        let answer = if let Some(a) = self.macro_args.pop_front() {
            Some(a)
        } else {
            self.io.prompt_secret(prompt)
        };
        self.prompt_depth -= 1;
        answer
    }

    /// 現在の !if ネストが実行可能か。
    pub fn script_active(&self) -> bool {
        self.if_stack.last().is_none_or(|f| f.active())
    }

    /// yes/no。スクリプト中は引数トークンを y/n として解釈する。
    pub fn yesno(&mut self, prompt: &str) -> bool {
        self.prompt_depth += 1;
        let answer = if let Some(a) = self.macro_args.pop_front() {
            matches!(a.to_ascii_lowercase().as_str(), "y" | "yes" | "true" | "1")
        } else {
            self.io.yesno(prompt)
        };
        self.prompt_depth -= 1;
        answer
    }

    /// 簡易乱数 (&rnd 関数用)。
    pub fn seed(&mut self) -> i64 {
        self.rng_seed = self
            .rng_seed
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12345);
        self.rng_seed
    }

    /// "C-A" / "^X C-S" / "M-f" 形式的なキー名からバインド先コマンド名を返す。
    pub fn command_for_key_name(&self, name: &str) -> Option<String> {
        let k = parse_key_name(name)?;
        self.keymap.get(&k).copied().map(|s| s.to_string())
    }

    /// 実行されたコマンドをキーボードマクロに記録する。
    pub fn record_key(&mut self, k: Ckey, f: bool, n: i32) {
        if self.kbdmode != KbdMode::Record || self.in_macro_play || self.kbdm.len() >= 256 {
            return;
        }
        if k == ctlx(b'(')
            || k == ctlx(b')')
            || k == ctlxlctl(b'(')
            || k == ctlxlctl(b')')
            || k == ctl(b'G')
        {
            return;
        }
        self.kbdm.push((k, f, n));
    }

    /// キーを入力として受け取り、フラグ/引数を処理してコマンドを実行する。
    /// C 版 `recedit()` のキー処理部分に相当。
    pub fn feed_key(&mut self, k: Ckey) -> Option<(Ckey, bool, i32)> {
        if self.prefix_ctlx {
            self.prefix_ctlx = false;
            let k2 = k | crate::keycode::CTLX;
            return self.feed_with_arg(k2);
        }
        if self.prefix_meta {
            self.prefix_meta = false;
            return self.feed_with_arg(k | META);
        }
        if k == ctl(b'X') {
            self.prefix_ctlx = true;
            self.mlwrite("C-X");
            return None;
        }
        if k == ctl(b'[') || k == 0x1b {
            self.prefix_meta = true;
            return None;
        }
        if k == ctl(b'U') && self.arg_state.is_none() {
            self.arg_state = Some(ArgState {
                value: 4,
                digits: String::new(),
                negative: false,
            });
            self.mlwrite("Arg: 4");
            return None;
        }
        self.feed_with_arg(k)
    }

    fn feed_with_arg(&mut self, k: Ckey) -> Option<(Ckey, bool, i32)> {
        if self.arg_state.is_none() {
            return Some((k, false, 1));
        }
        let c = (k & !crate::keycode::CK_FLAGS) as u8;
        let is_digit = c.is_ascii_digit();
        let is_repeat = k == ctl(b'U');
        let is_minus = c == b'-' && k & CTLFLG == 0;
        if is_digit || is_repeat || is_minus {
            let a = self.arg_state.as_mut().unwrap();
            if is_repeat {
                a.value = a.value.saturating_mul(4);
            } else if is_minus {
                if a.digits.is_empty() && !a.negative {
                    a.negative = true;
                    a.value = 1;
                }
            } else {
                a.digits.push(c as char);
                a.value = a.digits.parse::<i32>().unwrap_or(1);
            }
            let shown = if a.negative {
                a.value.saturating_neg()
            } else {
                a.value
            };
            self.mlwrite(format!("Arg: {}", shown));
            return None;
        }
        let a = self.arg_state.take().unwrap();
        let n = if a.negative {
            a.value.saturating_neg()
        } else {
            a.value
        };
        Some((k, true, n))
    }

    /// バインドを引いてコマンドを実行する。C 版 `execute()` に相当。
    pub fn execute(&mut self, k: Ckey, f: bool, n: i32) -> Status {
        let name = self.keymap.get(&k).copied().or_else(|| {
            // C 版と同じく ^X / M- + 英字は大小文字の別名を許す
            let has_ctlx = k & crate::keycode::CTLX != 0 && k & CTLFLG == 0;
            let has_meta = k & META != 0 && k & crate::keycode::CTLX == 0 && k & CTLFLG == 0;
            if has_ctlx || has_meta {
                let c = crate::keycode::code(k);
                if c.is_ascii_alphabetic() {
                    let other = (k & !0xff) | (c ^ 0x20) as Ckey;
                    return self.keymap.get(&other).copied();
                }
            }
            None
        });
        self.lastchar = k;
        let Some(name) = name else {
            // C 版 execute() と同じく、修飾子のない文字は自己挿入。
            if k & crate::keycode::CK_FLAGS == 0 && n > 0 {
                return self.self_insert(k, n);
            }
            self.mlwrite("[Key not bound]");
            self.lastflag = 0;
            return Status::False;
        };
        self.execute_named(name, f, n)
    }

    /// 印刷可能文字の自己挿入 (C 版 execute() の末尾に相当)。
    fn self_insert(&mut self, k: Ckey, n: i32) -> Status {
        let c = crate::keycode::code(k);
        if c < 0x20 && c != b'\t' {
            return Status::False;
        }
        let before = self.buffer().snapshot();
        let count = n.clamp(1, 1_000_000) as usize;
        let ok = self.buffer_mut().insert_repeat(count, c);
        if ok && self.buffer().mode & crate::buffer::MDASAVE != 0 {
            for _ in 0..count.min(1000) {
                self.autosave_tick();
            }
        }
        self.thisflag = 0;
        self.lastflag = 0;
        let bi = self.buffer_index();
        if self.buffers[bi].lines != before.lines {
            self.buffers[bi].record_undo(before);
        }
        Status::from_ok(ok)
    }

    pub fn execute_named(&mut self, name: &str, f: bool, n: i32) -> Status {
        self.thisflag = 0;
        let before: Option<Snapshot> = if matches!(name, "undo" | "redo") {
            None
        } else {
            Some(self.buffer().snapshot())
        };
        self.restore_window_pos();
        let st = commands::dispatch(self, name, f, n);
        self.save_window_pos();
        self.lastflag = self.thisflag;
        if let Some(b) = before {
            let bi = self.buffer_index();
            let cur = &self.buffers[bi];
            if cur.lines != b.lines || cur.fname != b.fname {
                self.buffers[bi].record_undo(b);
            }
        }
        st
    }

    /// 引数なし・1 回のコマンド実行 (テストや内部呼び出し用)。
    pub fn run(&mut self, name: &str) -> Status {
        self.execute_named(name, false, 1)
    }
}

impl Editor {}

/// `cmdstr()` 形式のキー名を Ckey に変換する。
fn parse_key_name(name: &str) -> Option<Ckey> {
    let mut k: Ckey = 0;
    let mut rest = name.trim();
    loop {
        if let Some(r) = rest.strip_prefix("M-") {
            k |= crate::keycode::META;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("^X ") {
            k |= crate::keycode::CTLX;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("C-") {
            k |= crate::keycode::CTLFLG;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("SPEC ") {
            k |= crate::keycode::SPEC;
            rest = r;
        } else {
            break;
        }
    }
    if let Some(r) = rest.strip_prefix('<') {
        let digits: String = r.chars().take_while(|c| c.is_ascii_digit()).collect();
        return digits.parse::<u8>().ok().map(|b| k | b as Ckey);
    }
    let c = rest.chars().next()?;
    let b = if c.is_ascii() { c as u8 } else { return None };
    Some(
        k | if k & crate::keycode::CTLFLG != 0 {
            b.to_ascii_uppercase()
        } else {
            b
        } as Ckey,
    )
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    #[test]
    fn parse_key_names() {
        assert_eq!(parse_key_name("C-A"), Some(crate::keycode::ctl(b'A')));
        assert_eq!(
            parse_key_name("^X C-S"),
            Some(crate::keycode::ctlxlctl(b'S'))
        );
        assert_eq!(parse_key_name("M-f"), Some(crate::keycode::meta(b'f')));
    }

    #[test]
    fn mlwrite_then_clear_message() {
        let mut e = Editor::new(Box::new(crate::input::ScriptedIo::new()), 24, 80);
        e.mlwrite("a message");
        assert_eq!(e.message, "a message");
        assert!(e.mpresf);
        e.clear_message();
        assert!(e.message.is_empty());
        assert!(!e.mpresf);
    }
}
