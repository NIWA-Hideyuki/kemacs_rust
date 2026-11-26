//! C 版 `make test` にあるが `cargo test` に欠けていた項目の回帰テスト。
//! tests/test_batch.c / test_eval.c / test_kanji.c / test_line.c /
//! test_undo.c / test_memory_safety.c / test_search.c / test_spawn.c /
//! test_fileio.c / test_cmode.c / test_python.c / test_mdmode.c に対応。

use kemacs::buffer::{Buffer, Pos};
use kemacs::editor::{Editor, Status};
use kemacs::encoding::{self, Code, Eol};
use kemacs::input::ScriptedIo;
use kemacs::script::eval_expr;

fn ed() -> Editor {
    Editor::new(Box::new(ScriptedIo::new()), 24, 80)
}

// --- test_batch.c / test_eval.c -------------------------------------------

#[test]
fn eval_division_and_modulo_by_zero_return_error() {
    let mut e = ed();
    for expr in ["&div 10 0", "&mod 10 0", "&rnd 0"] {
        assert_eq!(eval_expr(&mut e, expr), "ERROR", "{}", expr);
    }
}

#[test]
fn eval_signed_overflow_returns_error() {
    let mut e = ed();
    let cases = [
        "&neg -2147483648",
        "&abs -2147483648",
        "&rnd -2147483648",
        "&div -2147483648 -1",
        "&mod -2147483648 -1",
        "&add 2147483647 1",
        "&sub -2147483648 1",
        "&times 2147483647 2",
        "&times -2147483648 -1",
    ];
    for expr in cases {
        assert_eq!(eval_expr(&mut e, expr), "ERROR", "{}", expr);
    }
}

#[test]
fn eval_int_min_roundtrip_and_ordinary_values() {
    let mut e = ed();
    assert_eq!(eval_expr(&mut e, "&add -2147483648 0"), "-2147483648");
    assert_eq!(eval_expr(&mut e, "&div 10 3"), "3");
    assert_eq!(eval_expr(&mut e, "&abs -5"), "5");
    assert_eq!(eval_expr(&mut e, "&add 2 3"), "5");
    assert_eq!(eval_expr(&mut e, "&sub 9 4"), "5");
    assert_eq!(eval_expr(&mut e, "&times 3 4"), "12");
}

#[test]
fn eval_stoi_saturates_at_32bit_bounds() {
    let mut e = ed();
    assert_eq!(eval_expr(&mut e, "&add 99999999999 0"), "2147483647");
    assert_eq!(eval_expr(&mut e, "&add -99999999999 0"), "-2147483648");
}

#[test]
fn eval_random_sequence_from_seed_one() {
    let mut e = ed();
    e.rng_seed = 1;
    let a = eval_expr(&mut e, "&rnd 100");
    let b = eval_expr(&mut e, "&rnd 100");
    assert!(a.parse::<i64>().is_ok() && b.parse::<i64>().is_ok());
    assert_ne!(a, b);
}

// --- test_kanji.c ----------------------------------------------------------

#[test]
fn sjis_halfwidth_kana_roundtrip() {
    let src = "ｱｲ日本語".as_bytes();
    let enc = encoding::from_utf8(src, Code::Sjis);
    assert_eq!(encoding::to_utf8(&enc, Code::Sjis), src);
}

#[test]
fn ujis_halfwidth_kana_and_fullwidth_space_roundtrip() {
    let src = "　ｱｲ漢字".as_bytes();
    let enc = encoding::from_utf8(src, Code::Ujis);
    assert_eq!(encoding::to_utf8(&enc, Code::Ujis), src);
}

#[test]
fn jis_escape_and_halfwidth_kana_roundtrip() {
    let src = "日本語ｱ".as_bytes();
    let enc = encoding::from_utf8(src, Code::Jis);
    assert!(enc.windows(3).any(|w| w == b"\x1b$B"));
    assert_eq!(encoding::to_utf8(&enc, Code::Jis), src);
}

#[test]
fn code_detection_sjis_kanji_is_unambiguous() {
    assert_eq!(Code::detect(&[0x82, 0xa0]), Code::Sjis);
    assert_eq!(Code::detect("日本語".as_bytes()), Code::Utf8);
}

// --- test_line.c / test_memory_safety.c / test_undo.c ----------------------

#[test]
fn buffer_large_insert_and_delete_past_end() {
    let mut b = Buffer::new("stress");
    let text = "あ".repeat(1000);
    assert!(b.insert_bytes(text.as_bytes()));
    assert_eq!(b.text(), text.as_bytes());
    let (ok, killed) = b.delete(b.text().len() + 100, true);
    assert!(ok);
    // C 版 ldelete と同様、最終行の末尾を越える削除は何もしない
    assert_eq!(b.text(), text.as_bytes());
    assert!(killed.is_empty());
}

#[test]
fn buffer_newline_split_and_join_integrity() {
    let mut b = Buffer::new("lines");
    b.insert_bytes(b"abcdef");
    b.dot = Pos::new(0, 3);
    assert!(b.newline());
    assert_eq!(b.text(), b"abc\ndef");
    b.dot = Pos::new(0, 3);
    let (ok, killed) = b.delete(1, true);
    assert!(ok);
    assert_eq!(killed, b"\n");
    assert_eq!(b.text(), b"abcdef");
}

#[test]
fn buffer_undo_redo_stress_with_limit() {
    let mut b = Buffer::new("undo-stress");
    b.undo_limit = 600;
    for i in 0..500 {
        let before = b.snapshot();
        assert!(b.insert_bytes(format!("{}\n", i).as_bytes()));
        b.record_undo(before);
    }
    assert_eq!(b.lines.len(), 501);
    assert!(b.undo());
    assert_eq!(b.lines.len(), 500);
    assert!(b.redo());
    assert_eq!(b.lines.len(), 501);
}

#[test]
fn undo_history_stores_deltas_not_full_buffers() {
    let mut b = Buffer::new("delta");
    let base = "あ".repeat(20_000); // 60 KB の 1 行
    assert!(b.insert_bytes(base.as_bytes()));
    b.undo.clear();
    b.redo.clear();
    for i in 0..200 {
        let before = b.snapshot();
        assert!(b.insert_bytes(format!("{}", i % 10).as_bytes()));
        b.record_undo(before);
    }
    assert_eq!(b.undo.len(), 200);
    // 全コピー方式なら 200 * 60KB = 12MB 超。差分は数 KB に収まる。
    assert!(b.undo_bytes() < 8_000, "undo_bytes = {}", b.undo_bytes());
    for _ in 0..200 {
        assert!(b.undo());
    }
    assert_eq!(b.text(), base.as_bytes());
    let digits: String = (0..200)
        .map(|i| char::from(b'0' + (i % 10) as u8))
        .collect();
    for _ in 0..200 {
        assert!(b.redo());
    }
    assert_eq!(b.text(), format!("{}{}", base, digits).as_bytes());
}

#[test]
fn undo_redo_restores_multiline_delta() {
    let mut b = Buffer::new("ml");
    b.insert_bytes(b"alpha\nbeta\ngamma");
    let before = b.snapshot();
    b.dot = Pos::new(0, 5);
    assert!(b.insert_bytes(b"\nINSERT"));
    b.record_undo(before);
    assert_eq!(b.text(), b"alpha\nINSERT\nbeta\ngamma");
    assert!(b.undo());
    assert_eq!(b.text(), b"alpha\nbeta\ngamma");
    assert!(b.redo());
    assert_eq!(b.text(), b"alpha\nINSERT\nbeta\ngamma");

    let before = b.snapshot();
    b.lines.remove(1);
    b.clamp_dot();
    b.record_undo(before);
    assert_eq!(b.text(), b"alpha\nbeta\ngamma");
    assert!(b.undo());
    assert_eq!(b.text(), b"alpha\nINSERT\nbeta\ngamma");
}

#[test]
fn buffer_multiline_region_integrity() {
    let mut b = Buffer::new("region");
    b.insert_bytes(b"one\ntwo\nthree");
    b.mark = Some(Pos::new(0, 0));
    b.dot = Pos::new(2, 5);
    assert_eq!(b.region_bytes().unwrap(), b"one\ntwo\nthree");
    assert!(b.replace_region(b"X"));
    assert_eq!(b.text(), b"X");
}

// --- test_cmode.c / test_python.c / test_mdmode.c --------------------------

#[test]
fn c_mode_comment_and_preprocessor_indent() {
    let mut e = ed();
    e.buffer_mut().mode |= kemacs::buffer::MDCMOD;
    e.buffer_mut().insert_bytes(b"/* { */ if (x)");
    assert_eq!(kemacs::modes::c_insert(&mut e, 1), Status::True);
    assert_eq!(e.buffer().text(), b"/* { */ if (x)\n");
    e.buffer_mut().dot = Pos::new(0, 14);
    e.buffer_mut().insert_bytes(b"#if X");
}

#[test]
fn c_mode_preprocessor_and_try_catch() {
    let mut e = ed();
    e.buffer_mut().mode |= kemacs::buffer::MDCMOD;
    e.buffer_mut().insert_bytes(b"#if X");
    assert_eq!(kemacs::modes::c_insert(&mut e, 1), Status::True);
    assert_eq!(e.buffer().text(), b"#if X\n\t");
    let mut e2 = ed();
    e2.buffer_mut().mode |= kemacs::buffer::MDCMOD;
    e2.buffer_mut().insert_bytes(b"try {");
    kemacs::modes::c_insert(&mut e2, 1);
    assert_eq!(e2.buffer().text(), b"try {\n\t");
    let mut e3 = ed();
    e3.buffer_mut().mode |= kemacs::buffer::MDCMOD;
    e3.buffer_mut().insert_bytes(b"} catch (...) {");
    kemacs::modes::c_insert(&mut e3, 1);
    assert_eq!(e3.buffer().text(), b"} catch (...) {\n\t");
}

#[test]
fn py_mode_partial_dedent_and_blank_prev() {
    let mut e = ed();
    e.buffer_mut().mode |= kemacs::buffer::MDPYMOD;
    e.buffer_mut().insert_bytes(b"    x = 1");
    e.buffer_mut().dot = Pos::new(0, 2);
    kemacs::modes::py_backspace(&mut e);
    assert_eq!(e.buffer().text(), b"  x = 1");
    let mut e2 = ed();
    e2.buffer_mut().mode |= kemacs::buffer::MDPYMOD;
    e2.buffer_mut().insert_bytes(b"\nx = 1");
    e2.buffer_mut().dot = Pos::new(1, 0);
    kemacs::modes::py_indent(&mut e2, 1);
    assert_eq!(e2.buffer().text(), b"\nx = 1");
}

#[test]
fn md_mode_quote_nesting_and_task_toggle() {
    let mut e = ed();
    e.buffer_mut().mode |= kemacs::buffer::MDMDMOD;
    e.buffer_mut().insert_bytes(b">> quote");
    kemacs::modes::md_insert(&mut e, 1);
    assert_eq!(e.buffer().text(), b">> quote\n>> ");
    let mut e2 = ed();
    e2.buffer_mut().mode |= kemacs::buffer::MDMDMOD;
    e2.buffer_mut().insert_bytes(b"- [x] done");
    assert_eq!(kemacs::modes::md_task_toggle(&mut e2), Status::True);
    assert_eq!(e2.buffer().text(), b"- [ ] done");
    assert_eq!(kemacs::modes::md_task_toggle(&mut e2), Status::True);
    assert_eq!(e2.buffer().text(), b"- [x] done");
}

#[test]
fn md_mode_heading_cycle_max_and_fence_visible() {
    let mut e = ed();
    e.buffer_mut().mode |= kemacs::buffer::MDMDMOD;
    e.buffer_mut().insert_bytes(b"###### h");
    kemacs::modes::md_heading_cycle(&mut e);
    assert_eq!(e.buffer().text(), b"h");
    e.md_conceal = true;
    e.buffer_mut().lines = vec![b"```rust".to_vec(), b"**bold**".to_vec()];
    let f = kemacs::display::frame(&mut e);
    assert!(f.rows[0].contains("```rust"));
    assert!(f.rows[1].contains("bold"));
    assert!(!f.rows[1].contains('*'));
}

// --- test_search.c --------------------------------------------------------

#[test]
fn search_boundaries_and_empty_pattern() {
    let lines = vec![b"abc".to_vec(), b"xyz".to_vec()];
    use kemacs::search::Direction;
    assert_eq!(
        kemacs::search::find_mode(&lines, Pos::new(0, 0), b"", Direction::Forward, true, false),
        None
    );
    assert_eq!(
        kemacs::search::find_mode(
            &lines,
            Pos::new(0, 0),
            b"a",
            Direction::Forward,
            true,
            false
        ),
        Some((Pos::new(0, 0), Pos::new(0, 1)))
    );
    assert_eq!(
        kemacs::search::find_mode(
            &lines,
            Pos::new(1, 3),
            b"abc",
            Direction::Reverse,
            true,
            false
        ),
        Some((Pos::new(0, 0), Pos::new(0, 3)))
    );
}

#[test]
fn search_exact_disables_fullwidth_fold() {
    let lines = vec!["a。b".as_bytes().to_vec()];
    use kemacs::search::Direction;
    assert_eq!(
        kemacs::search::find_mode(
            &lines,
            Pos::new(0, 0),
            b"a.b",
            Direction::Forward,
            true,
            false
        ),
        None
    );
    assert_eq!(
        kemacs::search::find_mode(
            &lines,
            Pos::new(0, 0),
            b"a.b",
            Direction::Forward,
            false,
            false
        ),
        Some((Pos::new(0, 0), Pos::new(0, 5)))
    );
}

// --- test_spawn.c / test_fileio.c / display gutter ------------------------

#[test]
fn shell_escape_with_empty_and_quote() {
    assert_eq!(kemacs::system::shell_escape(""), "''");
    assert_eq!(kemacs::system::shell_escape("a b"), "'a b'");
    assert_eq!(kemacs::system::shell_escape("it's"), "'it'\\''s'");
}

#[test]
fn preview_ingests_partial_and_crlf_lines() {
    let mut e = ed();
    assert_eq!(
        kemacs::system::preview_start(
            &mut e,
            "printf 'ab'; printf 'cd\\r\\nef\\n'",
            "*ParityPreview*",
        ),
        Status::True
    );
    for _ in 0..50 {
        e.preview_poll();
        if e.preview.as_ref().unwrap().finished {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let bi = e.find_buffer("*ParityPreview*").unwrap();
    let text = String::from_utf8_lossy(&e.buffers[bi].text()).into_owned();
    assert!(text.contains("abcd"), "{}", text);
    assert!(text.contains("ef"), "{}", text);
    assert!(!text.contains('\r'), "{}", text);
    kemacs::system::preview_close(&mut e);
}

#[test]
fn long_line_file_roundtrip() {
    let data = format!("{}\n", "あ".repeat(1000));
    let mut b = Buffer::new("long");
    b.load_encoded(data.as_bytes(), Code::Utf8, Eol::Lf);
    assert_eq!(b.text(), data.as_bytes());
    assert_eq!(b.encoded_text(), data.as_bytes());
}

#[test]
fn japanese_filename_save_roundtrip() {
    let dir = std::env::temp_dir().join(format!("kemacs-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("日本語ファイル.txt");
    let ps = path.to_string_lossy().into_owned();
    let io = ScriptedIo::with_prompts(&[&ps]);
    let mut e = Editor::new(Box::new(io), 24, 80);
    assert_eq!(e.execute_named("filefind", false, 1), Status::True);
    e.buffer_mut().insert_bytes("日本語の内容".as_bytes());
    assert_eq!(e.execute_named("filesave", false, 1), Status::True);
    assert_eq!(std::fs::read(&path).unwrap(), "日本語の内容".as_bytes());
    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[test]
fn display_gutter_and_long_line_truncation() {
    let mut e = ed();
    e.show_line_numbers = true;
    e.line_number_width = 3;
    e.buffer_mut().insert_bytes(b"one\ntwo\nthree");
    let f = kemacs::display::frame(&mut e);
    assert!(f.rows[0].starts_with(" 1 one"));
    assert!(f.rows[2].starts_with(" 3 three"));
    e.buffer_mut().lines[0] = vec![b'x'; 200];
    let f2 = kemacs::display::frame(&mut e);
    assert_eq!(f2.rows[0].chars().count(), 80);
}

// --- レビューで見つかった回帰 ---------------------------------------------

#[test]
fn simple_text_commands_at_eob_do_not_panic() {
    let mut e = ed();
    e.buffer_mut().insert_bytes(b"abc\n");
    // forward_char で終端行へ移動した状態 (dot.line == lines.len()) を再現
    e.buffer_mut().dot = Pos::new(1, 0);
    for cmd in ["trim", "detab", "entab"] {
        assert_eq!(e.execute_named(cmd, false, 1), Status::True, "{}", cmd);
    }
}

#[test]
fn md_heading_max_zero_does_not_panic() {
    let mut e = ed();
    e.md_heading_max = 0;
    assert_eq!(
        kemacs::modes::md_insert_heading(&mut e, false, 1),
        Status::True
    );
    assert_eq!(e.buffer().text(), b"# ");
}

#[test]
fn fullwidth_digits_fold_to_ascii_for_search() {
    let lines = vec![b"x0y".to_vec()];
    use kemacs::search::Direction;
    assert_eq!(
        kemacs::search::find_mode(
            &lines,
            Pos::new(0, 0),
            "０".as_bytes(),
            Direction::Forward,
            false,
            false,
        ),
        Some((Pos::new(0, 1), Pos::new(0, 2)))
    );
}

#[test]
fn empty_mode_prompt_does_not_panic() {
    let io = ScriptedIo::with_prompts(&[""]);
    let mut e = Editor::new(Box::new(io), 24, 80);
    assert_eq!(e.execute_named("set_mode", false, 1), Status::False);
}

#[test]
fn filter_uses_pipe_instead_of_predictable_temp_files() {
    let dir = std::env::temp_dir();
    let before: usize = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("kemacs-filter-")
        })
        .count();
    let io = ScriptedIo::with_prompts(&["tr a-z A-Z"]);
    let mut e = Editor::new(Box::new(io), 24, 80);
    e.buffer_mut().insert_bytes(b"abc");
    assert_eq!(e.execute_named("filter", false, 1), Status::True);
    assert_eq!(e.buffer().text(), b"ABC");
    let after: usize = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("kemacs-filter-")
        })
        .count();
    assert_eq!(after, before);
}

#[test]
fn oversized_regex_pattern_is_rejected() {
    let pat = vec![b'a'; 5000];
    assert!(kemacs::regex::Regex::compile(&pat).is_err());
}

#[test]
fn intern_command_reuses_static_names() {
    let a = kemacs::commands::intern_command("set-strong-key");
    let b = kemacs::commands::intern_command("set-strong-key");
    assert_eq!(a, "set-strong-key");
    assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
    assert_eq!(kemacs::commands::intern_command("no-such-command"), "noop");
}

#[test]
fn region_to_terminal_line_does_not_panic() {
    let mut e = ed();
    e.buffer_mut().insert_bytes(b"abc\ndef");
    e.buffer_mut().mark = Some(Pos::new(0, 0));
    e.buffer_mut().dot = Pos::new(2, 0);
    assert_eq!(e.buffer().region_bytes().unwrap(), b"abc\ndef");
    e.buffer_mut().dot = Pos::new(2, 0);
    e.buffer_mut().mark = Some(Pos::new(2, 0));
    assert_eq!(e.buffer().region_bytes().unwrap(), b"");
}

#[test]
fn very_long_ordered_marker_does_not_overflow() {
    let mut e = ed();
    let num = "9".repeat(40);
    e.buffer_mut()
        .insert_bytes(format!("{}999. item", num).as_bytes());
    assert_eq!(kemacs::modes::md_insert(&mut e, 1), Status::True);
}

#[test]
fn reverse_search_with_count_moves_to_earlier_match() {
    let io = ScriptedIo::with_prompts(&["bc"]);
    let mut e = Editor::new(Box::new(io), 24, 80);
    e.buffer_mut().insert_bytes(b"bc xx bc");
    e.buffer_mut().goto_eob();
    assert_eq!(e.execute_named("backsearch", false, 2), Status::True);
    assert_eq!(e.buffer().dot, Pos::new(0, 0));
}

#[test]
fn universal_argument_insert_is_bounded() {
    let mut e = ed();
    assert!(e.buffer_mut().insert_repeat(usize::MAX, b'x'));
    assert_eq!(e.buffer().text().len(), 1_000_000);
}

#[test]
fn strong_key_uses_echo_free_prompt_and_redacts_debug() {
    let io = ScriptedIo::with_prompts(&["hunter2"]);
    let mut e = Editor::new(Box::new(io), 24, 80);
    assert_eq!(e.execute_named("set-strong-key", false, 1), Status::True);
    assert_eq!(e.buffer().password.as_deref(), Some(&b"hunter2"[..]));
    let dbg = format!("{:?}", e.buffer());
    assert!(!dbg.contains("hunter2"), "{}", dbg);
    assert!(dbg.contains("<redacted>"));
    assert!(e.message.contains("XChaCha20-Poly1305"));
}

#[test]
fn wrong_password_sets_decrypt_failed() {
    use kemacs::buffer::Buffer;
    use kemacs::crypt::CryptKind;
    use kemacs::encoding::{Code, Eol};

    let mut good = Buffer::new("enc");
    good.set_strong_key(b"right");
    good.insert_bytes(b"secret");
    let enc = good.encoded_text();

    let mut bad = Buffer::new("enc");
    bad.crypt = CryptKind::Strong;
    bad.password = Some(b"wrong".to_vec());
    bad.load_encoded(&enc, Code::Utf8, Eol::Lf);
    assert!(bad.decrypt_failed);
}
