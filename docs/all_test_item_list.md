# `cargo test` — テスト項目一覧 (all_test_item_list)

最終更新: kemacs Rust 版 (crossterm ベースの端末入出力)

`cargo test` は Cargo のテストランナーで、`src/` の単体テスト (`#[test]`)、
`tests/rust_core.rs` の結合テスト、`tests/rust_parity.rs` の差分テストを
まとめてビルドして順次実行する。各テストは Rust の `assert!` / `assert_eq!` で
結果を検証し、1 件でも失敗すると終了コードが 0 以外になる。

## 実行結果サマリー

| テスト種別 | 置き場所 | テスト数 | 結果 | 対象 |
|---|---|---|---|---|
| 単体テスト | `src/*.rs` (14 ファイル) | 74 | 74 成功 / 0 失敗 | バッファ・エンコーディング・検索・正規表現・大小文字畳み込み・スクリプト・各モード・描画・入力・キーコード・コマンド・暗号・システム |
| 結合テスト | `tests/rust_core.rs` | 30 | 30 成功 / 0 失敗 | キー操作シーケンス、ファイル形式 (SJIS / CRLF / 暗号)、マウス、ウィンドウ、キーボードマクロ |
| 差分テスト | `tests/rust_parity.rs` | 38 | 38 成功 / 0 失敗 | C 版との互換性 (32bit 境界、文字コード往復、行・undo ストレス、各モードの端数、検索境界、レビュー回帰) |

> **合計: 142 件のテスト — 全テスト合格 (exit code 0)**
> 実行コマンド: **`cargo test`**

---

## 1. 単体テスト — `src/*.rs` (74 件)

各モジュールに `#[cfg(test)]` と `#[test]` で定義されたテスト。カテゴリ別の内訳は次のとおり。

| # | カテゴリ (ソース) | 件数 | 検証内容 |
|---|---|---|---|
| 1 | `src/buffer.rs` | 9 | バッファ行の挿入・削除・改行分割、UTF-8 文字境界、終端行、undo/redo、リージョン |
| 2 | `src/encoding.rs` | 6 | UTF-8 / SJIS / UJIS / JIS の往復、表現不能文字のエスケープ、改行種別 |
| 3 | `src/search.rs` | 4 | 前方・後方検索、MAGIC 自動判定、全角大小文字無視 |
| 4 | `src/regex.rs` | 6 | MAGIC 正規表現 (`.` `[]` `^` `$` `*`)、全角畳み込み、メタ文字の扱い |
| 5 | `src/casefold.rs` | 3 | 全角記号・かな・数字の畳み込みとバイト位置マッピング |
| 6 | `src/script.rs` | 8 | トークン化、変数・関数、`!if` 入れ子、ラベル、再帰上限、`&getkey` |
| 7 | `src/modes.rs` | 13 | C / Python / Markdown 各モードの挿入・インデント・構造編集 |
| 8 | `src/display.rs` | 6 | 差分描画、conceal、全角クリック位置、モードライン、巨大画面幅 |
| 9 | `src/input.rs` | 3 | `C-` / `M-` マッピング、リサイズ、SGR 1006 マウスイベント |
| 10 | `src/keycode.rs` | 3 | キーコード定数、キー列名書式、マウスキーエンコード |
| 11 | `src/commands.rs` | 4 | キー割当て、kill / yank、リージョン、`C-U` 数値引数 |
| 12 | `src/editor.rs` | 2 | 単語境界、キー名パース |
| 13 | `src/crypt.rs` | 3 | クラシック暗号の対合性、強暗号の往復・改ざん検出 |
| 14 | `src/system.rs` | 4 | シェルエスケープ、自動保存、出力取り込み、プレビュー寿命 |
| | **合計** | **74** | |

### 1.1. `src/buffer.rs` — バッファ行操作 (9 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `encoded_roundtrip` | エンコード済みバイト列の読み込みと書き出しの往復 |
| 2 | `terminal_line_sentinel` | 終端行 (センチネル) の明示的な表現 |
| 3 | `crlf_roundtrip` | CRLF 改行の往復 |
| 4 | `from_bytes_roundtrip` | バイト列からの読み込みと再出力 |
| 5 | `insert_and_newline` | 挿入と改行による行分割 |
| 6 | `delete_across_lines_and_kill` | 行をまたぐ削除と kill バッファへの格納 |
| 7 | `utf8_char_navigation` | UTF-8 の文字境界をまたぐカーソル移動 |
| 8 | `undo_redo_restore_text_and_cursor` | undo / redo でテキストとカーソル位置を復元 |
| 9 | `region_copy_and_replace` | リージョンのコピーと置換 |

### 1.2. `src/encoding.rs` — 文字コード変換 (6 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `utf8_sjis_roundtrip` | UTF-8 ↔ SJIS の往復 |
| 2 | `utf8_ujis_roundtrip` | UTF-8 ↔ UJIS (EUC-JP) の往復 |
| 3 | `jis_escape_sequences` | JIS (ISO-2022-JP) のエスケープ列 |
| 4 | `unrepresentable_emoji_roundtrip` | 表現不能文字の `KEMACS-U+XXXX;` エスケープ往復 |
| 5 | `eol_roundtrip` | 改行種別 (LF / CR / CRLF) の往復 |
| 6 | `parse_code_names` | 符号化方式名のパース |

### 1.3. `src/search.rs` — 検索 (4 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `fullwidth_casefold_search` | 全角文字の大小文字無視検索 |
| 2 | `automatic_magic_search` | MDMAGIC によるメタ文字の自動解釈 |
| 3 | `forward_search` | 前方検索 |
| 4 | `reverse_search` | 後方検索 |

### 1.4. `src/regex.rs` — MAGIC 正規表現 (6 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `literal_and_any` | リテラル一致と `.` のワイルドカード |
| 2 | `character_classes` | `[...]` / `[^...]` 文字クラス |
| 3 | `anchors_and_closure` | `^` / `$` アンカーと `*` 繰り返し |
| 4 | `fullwidth_casefold` | 全角文字の大小文字無視 |
| 5 | `non_c_metacharacters_are_literals` | C 版に存在しない `+ / ? / | / ()` はリテラルとして扱う |
| 6 | `case_insensitive_by_default` | 既定で大小文字を無視し、MDEXACT で完全一致に切り替わる |

### 1.5. `src/casefold.rs` — 大小文字畳み込み (3 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `fullwidth_punctuation_and_kana_fold` | 全角記号・ひらがな・カタカナの畳み込み |
| 2 | `fullwidth_digits_fold_to_ascii` | 全角０〜９が ASCII 数字へ畳み込まれる |
| 3 | `canonicalize_maps_byte_offsets` | Unicode 正規化後の元バイト位置への変換 |

### 1.6. `src/script.rs` — スクリプト言語 (8 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `tokenize_escapes_and_quotes` | エスケープとクォートのトークン化 |
| 2 | `variables_and_functions` | ユーザー変数 `%name` と `&` 関数 |
| 3 | `nested_if_stack` | `!if` / `!else` / `!endif` の入れ子スタック |
| 4 | `interactive_script_prompts_minibuffer` | 対話的スクリプトのミニバッファプロンプト |
| 5 | `string_functions_respect_char_boundaries` | 文字列関数が UTF-8 の文字境界を尊重 |
| 6 | `dobuf_rejects_excessive_recursion` | `dobuf` / `execproc` の再帰深さ上限 |
| 7 | `getkey_reads_terminal_input` | `&getkey` による端末キーの読み取り |
| 8 | `docmd_with_directives_and_labels` | ディレクティブとラベルを含むコマンド実行 |

### 1.7. `src/modes.rs` — C / Python / Markdown モード (13 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `bullet_continuation` | Markdown 箇条書きの継続 |
| 2 | `ordered_numbering_and_task` | 番号リストの採番とタスク一覧 |
| 3 | `quote_and_fence_behavior` | 引用・フェンスの挙動 |
| 4 | `backspace_removes_marker` | 行頭マーカーの 1 段削除 |
| 5 | `heading_commands` | 見出しの挿入・昇格・降格・循環 |
| 6 | `auto_pair_and_markup` | `$md-auto-pair` の自動対挿入と強調・リンク |
| 7 | `smart_indent_dispatch` | `M-J` のモード別振り分け |
| 8 | `c_insert_after_brace` | C モード: `{` の次の行のインデント |
| 9 | `c_insert_no_brace_and_close` | C モード: `{` なしの行と閉じ括弧 |
| 10 | `c_indent_after_open_brace` | C モード: 開き括弧後のインデント |
| 11 | `stroustrup_function_brace` | Stroustrup (style 6) の関数定義の `{` 配置 |
| 12 | `py_insert_and_indent` | Python モード: 挿入とコロンによるインデント |
| 13 | `py_backspace_dedents_one_level` | Python モード: バックスペースで 1 段デデント |

### 1.8. `src/display.rs` — 画面描画 (6 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `click_offset_with_wide_chars` | 全角文字を含む行のクリック桁 → バイト位置 |
| 2 | `markdown_conceal_hides_emphasis` | Markdown conceal で強調記号を非表示化 |
| 3 | `only_modified_rows_change` | 変更行だけが差分描画の対象になる |
| 4 | `color_change_forces_full_redraw` | 色変更時に全体再描画へ切り替わる |
| 5 | `huge_screen_width_does_not_allocate` | 巨大な画面幅でも過大なメモリを確保しない |
| 6 | `c_style_default_modeline` | 既定のモードライン書式 |

### 1.9. `src/input.rs` — キー入力 (3 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `map_ctrl_and_meta` | `C-` / `M-` (ESC) のキーマッピング |
| 2 | `apply_resize_updates_window_rows` | リサイズイベントでウィンドウ行数を再配分 |
| 3 | `mouse_event_mapping` | SGR 1006 マウスイベントのマッピング |

### 1.10. `src/keycode.rs` — キーコード (3 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `key_codes_match_c_version` | キーコード定数が C 版と同じ値 |
| 2 | `cmdstr_format` | キー列名の書式 |
| 3 | `mouse_encoding` | マウスキーのエンコード |

### 1.11. `src/commands.rs` — コマンド (4 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `bindings_match_ebind` | キー割当てが C 版のキー表と一致 |
| 2 | `kill_and_yank_roundtrip` | kill → yank の往復 |
| 3 | `mark_region_copy_and_kill` | マーク・リージョン・コピー・kill |
| 4 | `universal_argument_flow` | `C-U` 数値引数の流れ |

### 1.12. `src/editor.rs` — エディタ状態 (2 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `mouse_word_bounds_ascii_and_multibyte` | ASCII とマルチバイト文字の単語境界 |
| 2 | `parse_key_names` | キー名文字列のパース |

### 1.13. `src/crypt.rs` — 暗号化 (3 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `isomorphic_cipher` | クラシック (Beaufort) 暗号が対合であること |
| 2 | `strong_roundtrip_and_tamper_detection` | 強暗号の往復と改ざん検出 |
| 3 | `key_changes_ciphertext` | 鍵を変えると暗号文が変わること |

### 1.14. `src/system.rs` — シェル / OS アクセス (4 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `shell_escape_quotes` | シェル引数のクォートエスケープ |
| 2 | `autosave_path_convention` | 自動保存ファイル名 (`#ファイル名#`) の規則 |
| 3 | `capture_and_filter` | 外部コマンド出力の取り込みと filter |
| 4 | `preview_lifecycle` | プレビューの開始・ポーリング・終了 |

---

## 2. 結合テスト — `tests/rust_core.rs` (30 件)

`ScriptedIo` でキー入力とプロンプト応答を差し込んだ `Editor` を組み立て、キー操作から
ファイル保存・終了までを通しで検証する。C 版のキー操作・ファイル形式を維持していることを確認する。

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `key_sequence_find_type_save_quit` | `C-X C-F` → 日本語入力 → `C-X C-S` → `C-X C-C` の一連の流れ |
| 2 | `undo_redo_through_commands` | 1 キー = 1 コマンド境界の undo / redo |
| 3 | `kill_yank_and_prefix_keys` | `M-<` / `C-U 2 C-F` / `setmark` / `copyregion` / `yank` |
| 4 | `display_gutter_and_modeline` | 行番号ガーターとモードラインの描画 |
| 5 | `bindings_are_not_shadowed_by_case` | `C-X l` (小文字) と `C-X L` が同じコマンド |
| 6 | `c_key_binding_table_matches_reference` | キー割当てテーブル (100 件以上、`C-X C-S` / `C-X C-F` / `C-Y`) |
| 7 | `magic_regex_search` | MAGIC 正規表現の検索と、MAGIC 解除時のリテラル化 |
| 8 | `keyboard_macro_record_and_replay` | `C-X (` / `C-X )` で記録し `C-U 2 C-X E` で 2 回再生 |
| 9 | `named_procedure_documented_by_script` | 名前付きプロシージャの `!if` / `!endif` 実行 |
| 10 | `storeproc_execproc_and_setvar_script` | `storeproc` / `execproc` / `setvar` / `&add` |
| 11 | `key_rebinding_and_unbinding` | `bindtokey` で再割当て、`unbindkey` で解除 |
| 12 | `sjis_file_edit_and_save` | SJIS ファイルの読込・追記・保存 |
| 13 | `crlf_file_keeps_line_endings` | CRLF 改行が保存後も保持される |
| 14 | `encrypted_file_roundtrip` | 暗号化ファイルの保存と `Cipher` による復号 |
| 15 | `autosave_writes_backup_file` | 自動保存で `#ファイル名#` が書き出される |
| 16 | `pipe_and_filter_commands` | `pipecmd` の出力取り込みと `filter` の変換 |
| 17 | `mouse_click_and_scroll` | 左クリックでカーソル移動、ホイールで行スクロール |
| 18 | `color_commands_and_variables` | `set-fcolor` / `set-bcolor` と `$fcolor` / `$bcolor` |
| 19 | `c_mode_enter_and_reindent_through_keys` | C モードの `{` インデントと `M-J` 再インデント |
| 20 | `py_mode_enter_indent_and_backspace` | Python モードの `:` インデントとバックスペース |
| 21 | `md_mode_list_continuation_and_tab` | Markdown 箇条書きの継続とタブによるインデント |
| 22 | `automode_by_file_extension` | `.py` ファイルで PYMODE が自動有効になる |
| 23 | `strong_encryption_roundtrip` | XChaCha20-Poly1305 の往復と誤パスワード時の非復号 |
| 24 | `preview_reads_pipe_incrementally` | プレビューのパイプ逐次読み取りと完了メッセージ |
| 25 | `display_diff_skips_unchanged_rows` | 差分描画が変更行だけを対象にする |
| 26 | `mouse_double_click_selects_word` | ダブルクリックで単語を選択 |
| 27 | `mouse_drag_creates_region_from_press_point` | ドラッグで押下点からのリージョンを作成 |
| 28 | `mouse_shift_click_extends_region` | Shift+クリックでリージョンを延長 |
| 29 | `window_positions_are_independent` | 同じバッファを複数ウィンドウで表示しても dot が独立 |
| 30 | `script_getkey_in_condition` | `!if &sequal &getkey a` の条件評価 |

---

## 3. 差分テスト — `tests/rust_parity.rs` (38 件)

C 版の各テスト項目を Rust 版に照合して追加した回帰テスト。32bit 境界、文字コード往復、
行・undo のストレス、各モードの端数、検索境界に加え、レビューで見つかった不具合の
再発防止を兼ねる。

### 3.1. 式評価器 (5 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `eval_division_and_modulo_by_zero_return_error` | `&div` / `&mod` / `&rnd` の 0 除算が `ERROR` |
| 2 | `eval_signed_overflow_returns_error` | 符号付き 32bit オーバーフロー (`&neg` / `&abs` / `&add` / `&sub` / `&times`) が `ERROR` |
| 3 | `eval_int_min_roundtrip_and_ordinary_values` | INT_MIN の往復と通常値の除算・絶対値・四則演算 |
| 4 | `eval_stoi_saturates_at_32bit_bounds` | 巨大数値が 32bit 境界で saturation |
| 5 | `eval_random_sequence_from_seed_one` | `$seed` 指定時の RNG シーケンスが毎回異なる |

### 3.2. 文字コード (4 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `sjis_halfwidth_kana_roundtrip` | SJIS の半角カナ往復 |
| 2 | `ujis_halfwidth_kana_and_fullwidth_space_roundtrip` | UJIS の半角カナ・全角スペース往復 |
| 3 | `jis_escape_and_halfwidth_kana_roundtrip` | JIS エスケープ列と半角カナの往復 |
| 4 | `code_detection_sjis_kanji_is_unambiguous` | SJIS 漢字・UTF-8 の文字コード自動判定 |

### 3.3. バッファ / undo (4 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `buffer_large_insert_and_delete_past_end` | 1000 文字の挿入と末尾を越える削除 (何もしない) |
| 2 | `buffer_newline_split_and_join_integrity` | 改行分割と削除による行結合のデータ整合性 |
| 3 | `buffer_undo_redo_stress_with_limit` | `undo_limit` 付き 500 レコードの undo / redo |
| 4 | `buffer_multiline_region_integrity` | 複数行リージョンの取得と置換 |

### 3.4. C / Python / Markdown モード (5 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `c_mode_comment_and_preprocessor_indent` | C モード: コメント内の `{` をインデント要因にしない |
| 2 | `c_mode_preprocessor_and_try_catch` | C モード: `#if` / `try {` / `} catch (...)` |
| 3 | `py_mode_partial_dedent_and_blank_prev` | Python モード: 部分デデントと空行のスキップ |
| 4 | `md_mode_quote_nesting_and_task_toggle` | Markdown: 入れ子引用の継続とタスクチェックの切替 |
| 5 | `md_mode_heading_cycle_max_and_fence_visible` | Markdown: H6 → 通常行の循環と conceal 下のフェンス表示 |

### 3.5. 検索 (2 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `search_boundaries_and_empty_pattern` | 空パターン・先頭一致・末尾からの後方検索 |
| 2 | `search_exact_disables_fullwidth_fold` | MDEXACT で全角 `。` の畳み込みを無効化 |

### 3.6. シェル / ファイル I/O / 表示 (5 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `shell_escape_with_empty_and_quote` | 空文字・空白・引用符のシェルエスケープ |
| 2 | `preview_ingests_partial_and_crlf_lines` | 部分行と CRLF を含むプレビュー出力の取り込み (`\r` 残りなし) |
| 3 | `long_line_file_roundtrip` | 1000 文字の長行を含むファイルの往復 |
| 4 | `japanese_filename_save_roundtrip` | 日本語ファイル名の保存と読込 |
| 5 | `display_gutter_and_long_line_truncation` | 行番号ガーターの幅と 80 列での長行切り詰め |

### 3.7. レビューで見つかった回帰 (13 件)

| # | テスト関数 | 検証内容 |
|---|---|---|
| 1 | `simple_text_commands_at_eob_do_not_panic` | 終端行で `trim` / `detab` / `entab` がパニックしない |
| 2 | `md_heading_max_zero_does_not_panic` | `$md-heading-max=0` で見出し挿入がパニックしない |
| 3 | `fullwidth_digits_fold_to_ascii_for_search` | 全角数字が検索時に ASCII 数字へ畳み込まれる |
| 4 | `empty_mode_prompt_does_not_panic` | 空文字列のモード名でパニックしない |
| 5 | `filter_uses_pipe_instead_of_predictable_temp_files` | `filter` が予測可能な一時ファイルを残さない |
| 6 | `oversized_regex_pattern_is_rejected` | 5000 バイトの正規表現パターンを拒否 |
| 7 | `intern_command_reuses_static_names` | コマンド名の静的文字列再利用と未知名の `noop` フォールバック |
| 8 | `region_to_terminal_line_does_not_panic` | 終端行を指すリージョン取得がパニックしない |
| 9 | `very_long_ordered_marker_does_not_overflow` | 40 桁の順序リスト番号でオーバーフローしない |
| 10 | `reverse_search_with_count_moves_to_earlier_match` | カウント付き後方検索が同じ一致を繰り返さない |
| 11 | `universal_argument_insert_is_bounded` | ユニバーサル引数の挿入が 100 万バイトで頭打ち |
| 12 | `strong_key_uses_echo_free_prompt_and_redacts_debug` | パスワードをエコーなしで読み、Debug 表示では `<redacted>` にする |
| 13 | `wrong_password_sets_decrypt_failed` | 誤パスワード時に `decrypt_failed` が立つ |

---

## 4. 構成 / 実行フロー

1. `cargo test` を実行すると、まず単体テスト (`src/lib.rs` = 74 件、`src/main.rs` = 0 件) が動く
2. 次に `tests/rust_core.rs` (30 件) が動く
3. 次に `tests/rust_parity.rs` (38 件) が動く
4. 最後に doc テスト (`Doc-tests kemacs`, 0 件) が動く

```sh
cargo test                       # 全 142 件を実行
cargo test -- --nocapture        # 失敗時に出力を表示
cargo test key_sequence          # 名前を指定して絞り込み
```

## 補足

- テストフレームワーク: Rust 標準の `#[test]` / `assert!` / `assert_eq!` (外部のテストフレームワークは不使用)
- キー入力のスタブ: `src/input.rs` の `ScriptedIo` (プロンプト・回答・キューを事前に設定)
- 一時ファイル: `std::env::temp_dir()` 配下にプロセス ID 付きで作成し、テスト後に削除する
- `cargo test` はテストを並列実行するため、一時ディレクトリ名に `std::process::id()` を使って衝突を避けている
- 静的解析も併せて実行できる: `cargo clippy --all-targets -- -D warnings`
- `docs/Rust移植.md`: 実装状況と C 版とのソース対応表
