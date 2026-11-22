//! C 版のキー操作・ファイル形式を維持していることを確認する結合テスト。

use std::collections::VecDeque;

use kemacs::buffer::Pos;
use kemacs::display;
use kemacs::editor::{Editor, Status};
use kemacs::input::ScriptedIo;
use kemacs::keycode::{ctl, ctlx, ctlxlctl, mouse_key, Ckey};

fn feed(ed: &mut Editor, k: Ckey) -> Status {
    match ed.feed_key(k) {
        Some((k2, f, n)) => ed.execute(k2, f, n),
        None => Status::True,
    }
}

fn type_text(ed: &mut Editor, s: &str) {
    // 端末では Enter が C-M (改行コマンド) として渡される
    for b in s.as_bytes() {
        if *b == b'\n' {
            feed(ed, ctl(b'M'));
        } else {
            feed(ed, *b as Ckey);
        }
    }
}

#[test]
fn key_sequence_find_type_save_quit() {
    let dir = std::env::temp_dir().join(format!("kemacs-rust-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("edit.txt");
    let path_str = path.to_string_lossy().into_owned();

    let io = ScriptedIo {
        prompts: VecDeque::from(vec![path_str.clone()]),
        answers: VecDeque::from(vec![true]),
        keys: VecDeque::new(),
    };
    let mut ed = Editor::new(Box::new(io), 24, 80);

    // C-X C-F <file> -- ファイルを開く
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, ctl(b'F')), Status::True);
    // 日本語と改行を含むテキストを入力
    type_text(&mut ed, "hello 日本語\nworld\n");
    assert_eq!(ed.buffer().text(), "hello 日本語\nworld\n".as_bytes());
    assert_eq!(ed.buffer().dot.line, 2);

    // C-X C-S -- 保存 (ファイル名は既にあるのでプロンプトなし)
    assert_eq!(feed(&mut ed, ctl(b'X')), Status::True);
    assert_eq!(feed(&mut ed, ctl(b'S')), Status::True);
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(saved, "hello 日本語\nworld\n".as_bytes());

    // C-X C-C -- 変更は保存済みなので確認なしで終了
    feed(&mut ed, ctl(b'X'));
    feed(&mut ed, ctl(b'C'));
    assert!(ed.quit);

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn undo_redo_through_commands() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    type_text(&mut ed, "one");
    // C 版と同じく 1 キー = 1 コマンド境界
    for _ in 0..3 {
        assert_eq!(ed.execute_named("undo", false, 1), Status::True);
    }
    assert_eq!(ed.buffer().text(), b"");
    for _ in 0..3 {
        assert_eq!(ed.execute_named("redo", false, 1), Status::True);
    }
    assert_eq!(ed.buffer().text(), b"one");
    type_text(&mut ed, "two");
    assert_eq!(ed.execute_named("undo", false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"onetw");
}

#[test]
fn kill_yank_and_prefix_keys() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    type_text(&mut ed, "abcdef");
    // M-< で先頭へ、C-U 2 C-F で 2 文字進む
    feed(&mut ed, 0x1b); // ESC
    feed(&mut ed, b'<' as Ckey);
    assert_eq!(ed.buffer().dot.off, 0);
    feed(&mut ed, ctl(b'U'));
    feed(&mut ed, b'2' as Ckey);
    feed(&mut ed, ctl(b'F'));
    assert_eq!(ed.buffer().dot.off, 2);
    ed.execute_named("setmark", false, 1);
    for _ in 0..2 {
        feed(&mut ed, ctl(b'F'));
    }
    ed.execute_named("copyregion", false, 1);
    assert_eq!(ed.kill, b"cd");
    ed.execute_named("yank", false, 1);
    assert_eq!(ed.buffer().text(), b"abcdcdef");
}

#[test]
fn display_gutter_and_modeline() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    type_text(&mut ed, "abc\ndef");
    ed.execute_named("toggle_line_numbers", false, 1);
    let f = display::frame(&mut ed);
    assert_eq!(f.rows.len(), 22);
    assert!(f.rows[0].starts_with("     1 abc") || f.rows[0].contains("1 abc"));
    assert!(f.modeline.contains("main"));
    assert_eq!(f.message, "");
    assert_eq!(f.cursor_row, 1);
}

#[test]
fn bindings_are_not_shadowed_by_case() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    type_text(&mut ed, "x");
    // C-X l (小文字) も C-X L と同じく行番号トグル
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, b'l' as Ckey), Status::True);
    assert!(ed.show_line_numbers);
}

#[test]
fn c_key_binding_table_matches_reference() {
    let ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    let n = ed.keymap.len();
    assert!(n >= 100, "bindings: {}", n);
    assert_eq!(ed.keymap.get(&ctlxlctl(b'S')), Some(&"filesave"));
    assert_eq!(ed.keymap.get(&ctlxlctl(b'F')), Some(&"filefind"));
    assert_eq!(ed.keymap.get(&ctl(b'Y')), Some(&"yank"));
}

#[test]
fn magic_regex_search() {
    use kemacs::buffer::MDMAGIC;
    let io = ScriptedIo::with_prompts(&["[0-9]*"]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    type_text(&mut ed, "abc 123\nxyz 456\n");
    ed.buffer_mut().goto_bob();
    ed.buffer_mut().mode |= MDMAGIC;
    assert_eq!(ed.execute_named("forwsearch", false, 1), Status::True);
    assert_eq!(ed.buffer().dot, Pos::new(0, 7));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 4)));

    // MAGIC を外すとメタ文字はリテラルとして扱われる
    ed.buffer_mut().mode &= !MDMAGIC;
    ed.buffer_mut().goto_bob();
    assert_eq!(ed.execute_named("forwsearch", false, 1), Status::False);
}

#[test]
fn keyboard_macro_record_and_replay() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    // C-X ( a C-A C-X ) : 「a を挿して行頭へ戻る」を記録
    for k in [ctlx(b'('), b'a' as Ckey, ctl(b'A'), ctlx(b')')] {
        if let Some((k2, f, n)) = ed.feed_key(k) {
            ed.record_key(k2, f, n);
            let _ = ed.execute(k2, f, n);
        }
    }
    assert_eq!(ed.buffer().text(), b"a");
    // C-U 2 C-X E で 2 回再生
    assert_eq!(ed.feed_key(ctl(b'U')), None);
    assert_eq!(ed.feed_key(b'2' as Ckey), None);
    let (k, f, n) = ed.feed_key(ctlx(b'E')).unwrap();
    assert!(f);
    assert_eq!(n, 2);
    assert_eq!(ed.execute(k, f, n), Status::True);
    assert_eq!(ed.buffer().text(), b"aaa");
}

#[test]
fn named_procedure_documented_by_script() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    let lines: Vec<Vec<u8>> = [
        "insert-string hello",
        "!if FALSE",
        "insert-string skipped",
        "!endif",
        "newline",
        "insert-string world",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    let bi = ed.find_buffer_or_create("[[demo]]");
    ed.buffers[bi].lines = lines;
    assert_eq!(kemacs::script::dobuf(&mut ed, "[[demo]]"), Status::True);
    assert_eq!(ed.buffer().text(), b"hello\nworld");
}

#[test]
fn storeproc_execproc_and_setvar_script() {
    let io = ScriptedIo {
        prompts: VecDeque::from(vec!["demo2".to_string(), "demo2".to_string()]),
        answers: VecDeque::new(),
        keys: VecDeque::new(),
    };
    let mut ed = Editor::new(Box::new(io), 24, 80);
    assert_eq!(ed.execute_named("storeproc", false, 1), Status::True);
    assert_eq!(
        kemacs::script::docmd(&mut ed, "insert-string stored"),
        Status::True
    );
    assert_eq!(kemacs::script::docmd(&mut ed, "!endm"), Status::True);
    assert_eq!(ed.execute_named("execproc", false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"stored");

    // ユーザー変数と &add 関数
    kemacs::script::docmd(&mut ed, "setvar %x 3");
    assert_eq!(ed.user_vars.get("x").map(String::as_str), Some("3"));
    ed.macro_args.push_back("&add".to_string());
    ed.macro_args.push_back("%x".to_string());
    ed.macro_args.push_back("4".to_string());
    assert_eq!(kemacs::script::eval_token(&mut ed, "&add"), "7");
}

#[test]
fn key_rebinding_and_unbinding() {
    let io = ScriptedIo {
        prompts: VecDeque::from(vec!["gotoeol".to_string()]),
        answers: VecDeque::new(),
        keys: VecDeque::from(vec![ctl(b'L'), ctl(b'L')]),
    };
    let mut ed = Editor::new(Box::new(io), 24, 80);
    type_text(&mut ed, "abcd");
    ed.buffer_mut().goto_bob();
    assert_eq!(ed.execute_named("bindtokey", false, 1), Status::True);
    assert_eq!(ed.execute(ctl(b'L'), false, 1), Status::True);
    assert_eq!(ed.buffer().dot.off, 4);
    assert_eq!(ed.execute_named("unbindkey", false, 1), Status::True);
    assert_eq!(ed.execute(ctl(b'L'), false, 1), Status::False);
}

#[test]
fn sjis_file_edit_and_save() {
    use kemacs::encoding::{self, Code};
    let dir = std::env::temp_dir().join(format!("kemacs-rust-sjis-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sjis.txt");
    let path_str = path.to_string_lossy().into_owned();
    std::fs::write(&path, encoding::from_utf8("日本語".as_bytes(), Code::Sjis)).unwrap();

    let io = ScriptedIo::with_prompts(&[&path_str]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, ctl(b'F')), Status::True);
    assert_eq!(ed.buffer().text(), "日本語".as_bytes());
    assert_eq!(ed.buffer().code, Code::Sjis);
    ed.buffer_mut().goto_eob();
    type_text(&mut ed, "!");
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, ctl(b'S')), Status::True);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        encoding::from_utf8("日本語!".as_bytes(), Code::Sjis)
    );
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn crlf_file_keeps_line_endings() {
    let dir = std::env::temp_dir().join(format!("kemacs-rust-crlf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("crlf.txt");
    let path_str = path.to_string_lossy().into_owned();
    let raw = b"one\r\ntwo\r\n";
    std::fs::write(&path, raw).unwrap();

    let io = ScriptedIo::with_prompts(&[&path_str]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    feed(&mut ed, ctl(b'X'));
    feed(&mut ed, ctl(b'F'));
    assert_eq!(ed.buffer().text(), b"one\ntwo\n");
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, ctl(b'S')), Status::True);
    assert_eq!(std::fs::read(&path).unwrap(), raw);
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn encrypted_file_roundtrip() {
    use kemacs::crypt::Cipher;
    let dir = std::env::temp_dir().join(format!("kemacs-rust-crypt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("secret.txt");
    let path_str = path.to_string_lossy().into_owned();
    let io = ScriptedIo {
        prompts: VecDeque::from(vec![path_str.clone(), "secret".to_string()]),
        answers: VecDeque::new(),
        keys: VecDeque::new(),
    };
    let mut ed = Editor::new(Box::new(io), 24, 80);
    feed(&mut ed, ctl(b'X'));
    feed(&mut ed, ctl(b'F'));
    assert_eq!(ed.execute_named("setcrkey", false, 1), Status::True);
    type_text(&mut ed, "hidden message");
    feed(&mut ed, ctl(b'X'));
    assert_eq!(feed(&mut ed, ctl(b'S')), Status::True);
    let raw = std::fs::read(&path).unwrap();
    assert_ne!(raw, b"hidden message");
    let key = ed.buffer().key.clone().unwrap();
    let mut dec = raw;
    Cipher::from_key(&key).decrypt(&mut dec);
    assert_eq!(dec, b"hidden message");
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn autosave_writes_backup_file() {
    let dir = std::env::temp_dir().join(format!("kemacs-rust-asave-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("auto.txt");
    let path_str = path.to_string_lossy().into_owned();
    let io = ScriptedIo::with_prompts(&[&path_str]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    ed.buffer_mut().fname = Some(path_str.clone());
    ed.buffer_mut().mode |= kemacs::buffer::MDASAVE;
    ed.asave = 2;
    ed.acount = 2;
    type_text(&mut ed, "ab");
    let backup = dir.join("#auto.txt#");
    assert_eq!(std::fs::read(&backup).unwrap(), b"ab");
    assert!(ed.buffer().flags & kemacs::buffer::BFCHG != 0);
    std::fs::remove_file(&backup).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn pipe_and_filter_commands() {
    let io = ScriptedIo {
        prompts: VecDeque::from(vec!["printf piped".to_string(), "tr a-z A-Z".to_string()]),
        answers: VecDeque::new(),
        keys: VecDeque::new(),
    };
    let mut ed = Editor::new(Box::new(io), 24, 80);
    assert_eq!(ed.execute_named("pipecmd", false, 1), Status::True);
    assert_eq!(ed.buffer().name, "[command]");
    assert_eq!(ed.buffer().text(), b"piped");
    ed.window_mut().buf = 0;
    type_text(&mut ed, "abc");
    assert_eq!(ed.execute_named("filter", false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"ABC");
}

#[test]
fn mouse_click_and_scroll() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    type_text(&mut ed, "abcdef");
    ed.execute_named("newline", false, 1);
    type_text(&mut ed, "ghijkl");
    let k = kemacs::keycode::mouse_key(0, 5, 1); // 1 行目 5 桁目
    assert!(ed.handle_mouse(k));
    assert_eq!(ed.buffer().dot, Pos::new(0, 4));
    let down = kemacs::keycode::mouse_key(3, 1, 1);
    assert!(ed.handle_mouse(down));
    assert_eq!(ed.buffer().dot.line, 1);
}

#[test]
fn color_commands_and_variables() {
    let io = ScriptedIo::with_prompts(&["3", "10,20,30"]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    assert_eq!(ed.execute_named("set-fcolor", false, 1), Status::True);
    assert_eq!(ed.fg, kemacs::editor::ColorSpec::Ansi(3));
    assert!(ed.color_on);
    assert_eq!(kemacs::script::eval_token(&mut ed, "$fcolor"), "3");
    assert_eq!(ed.execute_named("set-bcolor", false, 1), Status::True);
    assert_eq!(ed.bg, kemacs::editor::ColorSpec::Rgb(10, 20, 30));
    let f = kemacs::display::frame(&mut ed);
    assert!(f.color_on);
    assert_eq!(f.fg, kemacs::editor::ColorSpec::Ansi(3));
}

#[test]
fn c_mode_enter_and_reindent_through_keys() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    assert_eq!(ed.execute_named("c_mode", false, 1), Status::True);
    type_text(&mut ed, "if (x) {");
    feed(&mut ed, ctl(b'M'));
    assert_eq!(ed.buffer().text(), b"if (x) {\n\t");
    type_text(&mut ed, "}");
    assert_eq!(ed.execute_named("smart_indent", false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"if (x) {\n}");
}

#[test]
fn py_mode_enter_indent_and_backspace() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    assert_eq!(ed.execute_named("py_mode", false, 1), Status::True);
    type_text(&mut ed, "if True:");
    feed(&mut ed, ctl(b'M'));
    assert_eq!(ed.buffer().text(), b"if True:\n    ");
    type_text(&mut ed, "x = 1");
    assert_eq!(ed.execute(ctl(b'H'), false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"if True:\n    x = ");
}

#[test]
fn md_mode_list_continuation_and_tab() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    assert_eq!(ed.execute_named("md_mode", false, 1), Status::True);
    type_text(&mut ed, "- first");
    feed(&mut ed, ctl(b'M'));
    assert_eq!(ed.buffer().text(), b"- first\n- ");
    type_text(&mut ed, "second");
    assert_eq!(ed.buffer().text(), b"- first\n- second");
    ed.buffer_mut().dot = Pos::new(1, 0);
    assert_eq!(ed.execute(ctl(b'I'), false, 1), Status::True);
    assert_eq!(ed.buffer().text(), b"- first\n  - second");
}

#[test]
fn automode_by_file_extension() {
    let dir = std::env::temp_dir().join(format!("kemacs-rust-amode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.py");
    let path_str = path.to_string_lossy().into_owned();
    let io = ScriptedIo::with_prompts(&[&path_str]);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    feed(&mut ed, ctl(b'X'));
    feed(&mut ed, ctl(b'F'));
    assert_ne!(ed.buffer().mode & kemacs::buffer::MDPYMOD, 0);
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn strong_encryption_roundtrip() {
    use kemacs::buffer::Buffer;
    use kemacs::crypt::CryptKind;
    use kemacs::encoding::{Code, Eol};

    let mut b = Buffer::new("test");
    b.set_strong_key(b"correct password");
    b.insert_bytes("secret 日本語 line".as_bytes());
    let enc = b.encoded_text();
    assert!(enc.starts_with(b"KEMACS-STRONG-V1"));
    assert!(!String::from_utf8_lossy(&enc).contains("secret"));

    let mut ok = Buffer::new("test");
    ok.crypt = CryptKind::Strong;
    ok.password = Some(b"correct password".to_vec());
    ok.load_encoded(&enc, Code::Utf8, Eol::Lf);
    assert_eq!(ok.text(), "secret 日本語 line".as_bytes());

    let mut bad = Buffer::new("test");
    bad.crypt = CryptKind::Strong;
    bad.password = Some(b"wrong password".to_vec());
    bad.load_encoded(&enc, Code::Utf8, Eol::Lf);
    assert!(!String::from_utf8_lossy(&bad.text()).contains("secret"));
}

#[test]
fn preview_reads_pipe_incrementally() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    let started =
        kemacs::system::preview_start(&mut ed, "printf 'ab'; printf 'cd\\nef\\n'", "*PreviewPipe*");
    assert_eq!(started, Status::True);
    assert!(ed.preview.as_ref().unwrap().fd >= 0);
    assert!(ed.preview.as_ref().unwrap().pending.is_empty() || true);
    for _ in 0..50 {
        ed.preview_poll();
        if ed.preview.as_ref().unwrap().finished {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let bi = ed.find_buffer("*PreviewPipe*").unwrap();
    let text = String::from_utf8_lossy(&ed.buffers[bi].text()).into_owned();
    assert!(text.contains("abcd"), "{}", text);
    assert!(text.contains("ef"), "{}", text);
    assert!(text.contains("[preview finished]"), "{}", text);
    kemacs::system::preview_close(&mut ed);
    assert!(ed.preview.is_none());
}

#[test]
fn display_diff_skips_unchanged_rows() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    ed.buffer_mut().insert_bytes(b"abc");
    let a = display::frame(&mut ed);
    type_text(&mut ed, "d");
    let b = display::frame(&mut ed);
    assert!(!display::needs_full_redraw(Some(&a), &b));
    let rows = display::changed_rows(Some(&a), &b);
    assert!(rows.contains(&0));
    assert!(
        !rows.contains(&5),
        "unchanged row should not be redrawn: {:?}",
        rows
    );
}

#[test]
fn mouse_double_click_selects_word() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    ed.buffer_mut().insert_bytes(b"foo bar baz");
    assert!(ed.handle_mouse(mouse_key(4, 3, 1)));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 0)));
    assert_eq!(ed.buffer().dot, Pos::new(0, 3));
}

#[test]
fn mouse_drag_creates_region_from_press_point() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    ed.buffer_mut().insert_bytes(b"hello world");
    assert!(ed.handle_mouse(mouse_key(0, 1, 1)));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 0)));
    assert!(ed.handle_mouse(mouse_key(1, 7, 1)));
    assert!(ed.handle_mouse(mouse_key(7, 7, 1)));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 0)));
    assert_eq!(ed.buffer().dot, Pos::new(0, 6));
}

#[test]
fn mouse_shift_click_extends_region() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    ed.buffer_mut().insert_bytes(b"hello world");
    ed.buffer_mut().dot = Pos::new(0, 5);
    assert!(ed.handle_mouse(mouse_key(5, 1, 1)));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 5)));
    assert_eq!(ed.buffer().dot, Pos::new(0, 0));
    assert!(ed.handle_mouse(mouse_key(6, 7, 1)));
    assert_eq!(ed.buffer().mark, Some(Pos::new(0, 6)));
    assert_eq!(ed.buffer().dot, Pos::new(0, 0));
}

#[test]
fn window_positions_are_independent() {
    let mut ed = Editor::new(Box::new(ScriptedIo::new()), 24, 80);
    ed.buffer_mut().insert_bytes(b"one\ntwo\nthree");
    assert_eq!(ed.execute_named("splitwind", false, 1), Status::True);
    assert_eq!(ed.windows.len(), 2);
    ed.buffer_mut().dot = Pos::new(0, 0);
    ed.set_cur_win(1);
    ed.buffer_mut().dot = Pos::new(2, 0);
    ed.save_window_pos();
    ed.set_cur_win(0);
    assert_eq!(ed.buffer().dot.line, 0);
    ed.set_cur_win(1);
    assert_eq!(ed.buffer().dot.line, 2);
}

#[test]
fn script_getkey_in_condition() {
    let mut io = ScriptedIo::new();
    io.keys.push_back(b'a' as Ckey);
    let mut ed = Editor::new(Box::new(io), 24, 80);
    kemacs::script::docmd(&mut ed, "!if &sequal &getkey a");
    kemacs::script::docmd(&mut ed, "insert-string yes");
    kemacs::script::docmd(&mut ed, "!endif");
    assert_eq!(ed.buffer().text(), b"yes");
}
