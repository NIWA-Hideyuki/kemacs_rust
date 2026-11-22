//! kemacs Rust 版: C 版 (MicroEMACS 3.8i + 日本語拡張) の中核を同じ挙動で移植したもの。
//!
//! 現在の範囲: キーバインド / バッファ / 行操作 / ウィンドウ / 基本コマンド /
//! ファイル入出力 / undo-redo / 画面表示 / インクリメンタルサーチの基本。
//! C 版のソース (.C_) は参照用として残してあり、移植済みコマンドは
//! `commands.rs` の `dispatch` に集約している。

pub mod buffer;
pub mod casefold;
pub mod commands;
pub mod crypt;
pub mod display;
pub mod editor;
pub mod encoding;
pub mod input;
pub mod keycode;
pub mod modes;
pub mod regex;
pub mod script;
pub mod search;
pub mod system;

pub use buffer::{Buffer, Pos};
pub use editor::Editor;
pub use keycode::{Ckey, Key, Status};
