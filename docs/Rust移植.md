# kemacs Rust 版 (移植作業の現状)

リポジトリ直下の `Cargo.toml` / `src/` / `tests/rust_core.rs` / `tests/rust_parity.rs` が Rust 版の実装です。
元の C 版は別リポジトリで管理しています。このリポジトリには Rust 版のみが含まれます。

## 基本方針

- キーコード (`CTLFLG` / `META` / `CTLX` / `SPEC`) と `ebind.H_` のキー割当てを同じ値で再現。
- コマンド名は `efunc.H_` の名前をそのまま使用 (`M-x` / マクロから呼べるようにした)。
- バッファ内部は UTF-8 バイト列、行は改行境界で表現。読み込み時に符号化方式を UTF-8 へ
  変換し、書き出し時に逆変換する (未編集なら元バイト列をそのまま書き戻す)。
- undo は「1 コマンド = 1 スナップショット」方式。C 版のコマンド境界モデルと同じ粒度。

## ビルド・実行・テスト

```sh
cargo build                       # target/debug/kemacs を生成
cargo build --release             # target/release/kemacs (最適化ビルド)
cargo install --path .            # ~/.cargo/bin/kemacs へインストール
cargo install --locked --path .   # Cargo.lock を固定してインストール
cargo run -- file.txt             # ファイルを開いて編集 (符号化方式は自動判定)
cargo run -- --code SJIS new.txt  # SJIS の新規ファイルとして開く
cargo test                        # Rust の単体・結合・差分テスト
cargo clippy --all-targets -- -D warnings
```

C 版互換の起動オプションも使えます (例: `cargo run -- -v -g 50 file.txt`)。

`cargo install` の配置先は `~/.cargo/bin` で、`--root` で変更できます。更新は `cargo install --force --path .`、削除は `cargo uninstall kemacs` です。GitHub 登録後は `cargo install --git https://github.com/NIWA-Hideyuki/kemacs_rust.git --locked` でも導入できます。crates.io への publish は非商用ライセンスのため行いません。

## 実装済み

- キー入力: `C-` / `M-` (ESC) / `C-X` プレフィックス / `C-U` 数値引数、矢印・PageUp/Down 等
- 移動: `C-A/C-E/C-B/C-F/C-N/C-P/C-V/C-Z`、`M-</M->`、`M-b/M-f`、`M-G`、段落移動
- 編集: 自己挿入 (UTF-8 複数バイト対応)、`C-M/C-J/C-O/C-I`、`C-D/C-H/DEL`、
  `C-K`、`C-T`、`M-D/M-H`、`C-Y`、`C-W/M-W`、`M-C/M-L/M-U`、行操作、空白整理
- 検索: `C-S` / `C-R` / `C-X S` (インクリメンタル)、置換 (`M-R`)
- **MAGIC 正規表現** (`src/regex.rs`): `.` `[...]` `[^...]` `^` `$` `*` `\`、
  ASCII 大小文字無視 (MDEXACT で完全一致)、MDMAGIC モードのときだけメタ解釈
- **キーボードマクロ**: `C-X (` 記録開始 / `C-X )` 終了 / `C-X E` 再生 (引数で repetition)
- **名前付きプロシージャ / スクリプト言語** (`src/script.rs`):
  `storeproc` / `storemac` / `execproc` / `execbuf` / `execfile` / `execcmd`、
  `!if` / `!else` / `!endif` / `!goto` / `!return` / `!force` / ラベル / コメント、
  `&add` 等の関数、ユーザー変数 `%name`、組み込み変数 `$fill-column` 等、
  `#buffer` によるバッファ内容参照
- **キー再バインド**: `M-K` (`bindtokey`) / `M-C-K` (`unbindkey`) / `C-X ?` (describe-key)
- **エンコーディング変換** (`src/encoding.rs`): UTF-8 / UJIS (EUC-JP) / SJIS / JIS
  (ISO-2022-JP) の iconv 変換と改行種別 (LF / CR / CRLF)。`$bcode` / `$fcode` で設定。
  未編集ファイルは元バイト列をそのまま保存し、変換できないバイトは素通しする。
- **暗号化** (`src/crypt.rs`): DLH-POLY-86-B (Beaufort) を C 版と同じアルゴリズムで実装。
  `M-E` (`setcrkey`) でキーを設定すると CRYPT モードになり、保存時に暗号化・読込時に復号。
- **自動保存**: ASAVE モードで `$asave` 文字ごとに `#ファイル名#` へ書き出す
  (`$acount` で残りカウントを参照)。
- **シェル / OS アクセス** (`src/system.rs`): `C-X !` (spawn)、`C-X C` (spawncli)、
  `C-X @` (pipecmd)、`C-X #` (filter)、`M-S` (bktoshell)、`&escape` 関数。
- **非同期プレビュー**: `M-Y` (md-preview) で外部コマンドをバックグラウンド実行し、
  pipe の nonblock read で出力を専用バッファへ追記。プレビュー中に `C-C` で中止。
- **マウスホイール**: 端末の SGR 1006 キャプチャは有効なままにし、ホイールの上下を
  カーソル移動の疑似キー (`WHEELUP` / `WHEELDOWN`, `MOUSEFLG` フラグ) として受け取る。
  1 ノッチの移動行数は `$wheel-lines` (既定 3) で変更できる。クリック / ドラッグ /
  横スクロールは機能に含めず破棄する。ホイールはプロンプト・`C-Q`・`&getkey` の
  文字入力には混ぜず、キーボードマクロにも記録しない。
- **カラー**: `$fcolor` / `$bcolor` と `set-fcolor` / `set-bcolor` (M-x) /
  `toggle-color`。8/256 色 (Ansi) と truecolor (RGB) に対応。
- **C Mode / Python Mode / Markdown Mode** (`src/modes.rs`):
  - C: `cinsert` (`{` で +1 レベル、`}` で対応する `{` に整列、制御キーワード、
    括弧継続、`#if` 系、brace style 0/1/2/4/6/7)、`cindent` (M-J)、
    `c-indent-region`、`c-detect-style`、`c-follow-source`。
  - Python: `pyinsert` (`:` で +`$py-indent-width`、`\` 継続は同レベル)、
    `pyindent` (`else/elif/except/finally` を 1 レベル戻す)、`py-backspace`、
    `py-format-region`。
  - Markdown: `mdinsert` (箇条書き継続・番号列表の採番・タスクリスト・引用)、
    `md_backspace` (マーカー 1 段削除)、`md_tab` (表セル移動 / リスト インデント)、
    `mdindent`、`smart_indent` (M-J のモード別振り分け)、見出し/LIST/強調/
    リンク/フェンス/アウトライン各コマンド、`$md-auto-pair` の自動対挿入。
  - Stroustrup (style 6) は `$c-brace-func=1` で関数定義の `{` を独立行に
    配置し、制御の `else` を対応する `if` に揃える (C 版は変数のみで
    未使用だったが Rust 版で完全実装)。
  - 設定変数: `$c-indent-width`, `$c-use-spaces`, `$c-brace-style`,
    `$c-cont-indent`, `$c-case-indent`, `$c-pp-indent`, `$c-follow-source`,
    `$py-indent-width`, `$py-use-spaces`, `$md-indent-width`, `$md-use-spaces`,
    `$md-auto-list`, `$md-auto-pair` など。
  - 拡張子による自動モード (`.c/.h`, `.py`, `.md/.markdown`)。
- **正規表現の大小文字無視**: `search.C_ ctab[]` の 221 項目 (全角記号・数字・
  英字・かな) を `src/casefold.rs` へ移植。パターンと行を Unicode 正規形に
  変換して照合し、元バイト位置へ変換して返す。`+ / ? / | / ()` は C 版に
  存在しないためメタ文字ではなくリテラルとして扱う。
- **Markdown conceal**: `C-X Y` (`md_conceal_toggle`) または `$md-conceal=1` で
  強調記号 (`**` / `*` / `_` / `~~` / `` ` ``) とリンクの URL 部分を描画時に
  非表示化。元バイト位置→表示桁の対応表を維持するためカーソル位置は崩れない。
- **リサイズ全描画**: crossterm の Resize イベントで `input::apply_resize()` が
  ウィンドウ高さを再配分し、前フレームを破棄して全行を再描画。入力待ち中も
  50ms ポーリングでリサイズを検知する。PTY で 41 回のカーソル移動を確認。
- **差分描画**: `display::{changed_rows,needs_full_redraw}` と `input::draw_diff` により、
  変更行・モード行・メッセージ行・カーソルのみを端末へ送出する (C 版 `update()` 相当)。
  色設定や行数変化時は全体再描画。1 キーあたりのカーソル移動シーケンスが
  全画面描画の約 25 個から 3 個に削減されることを PTY で確認。
- **強暗号 (追加)**: `M-x set-strong-key` で XChaCha20-Poly1305 + PBKDF2-SHA256
  (100,000 回) による認証付き暗号化を有効化。保存形式は
  `KEMACS-STRONG-V1\0 | salt(16) | nonce(24) | ciphertext+tag(16)`。
  強暗号ファイルを `C-X C-F` で開くとパスワードを要求する。
  `M-x crypt-off` で解除、M-E は従来どおり C 互換の弱暗号。
  `$crypt` で `NONE`/`CLASSIC`/`STRONG` を参照可能。
- スクリプト: `$script-interactive=1` または `M-x toggle-script-interactive` で、
  スクリプト中のコマンドをミニバッファへ対話的にプロンプトできる (再帰的な
  プロンプトも `Editor::prompt()` / `yesno()` のネスト深さで追跡)。
  `&getkey` / `get-key` で端末キーを対話的に読み取り、`!if` /
  `!else` / `!endif` はスタック方式で深く入れ子にできる (プロシージャ呼び出し
  ごとにスタックを保存/復元)。引数式 `!if &sequal &getkey a` も評価可能。
- ウィンドウ: 同じバッファを複数ウィンドウで表示しても、dot/mark は
  ウィンドウ×バッファ単位で独立 (C 版の `w_dotp` / `w_doto` 相当)。
- エンコーディング: 表現不能文字 (例: SJIS の絵文字) は `KEMACS-U+XXXX;` 形式の
  ASCII エスケープで保存し、読み込み時に復元するため往復が可逆。未編集ファイルの
  バイト列はそのまま保持。`--code` はファイル引数より前に置く (起動時適用順)。
- 終端行: `Buffer::b_linep()` / `b_linep_index()` / `line_or_sentinel()` で C 版の
  `b_linep` を明示的に表現。末尾改行は終端行直前の空行として扱い、表示も
  終端行経由で行を参照する。
- ファイル: `C-X C-F` / `C-X C-S` / `C-X C-W` / `C-X C-R` / `C-X I` / `M-Z`
- バッファ / ウィンドウ: 一覧・切替・作成・改名・削除、分割・移動・リサイズ
- 表示: 行番号ガーター、モードライン、UTF-8 表示幅、タブ展開、制御文字の `^X` 表示
- モードライン: C 版と同じ書式コード (`%p`=プログラム名 `日本語μEMACS`,
  `%v`=バージョン `2.1k`, `%m/%M`=モード, `%c`=改行/文字コード, `%-`=右端まで
  `=`) を使用。既定書式は
  `== %p %v (%m) == %b == File: %F == %c %-` で、CRLF/UTF8 の `main`
  バッファでは `== 日本語μEMACS 2.1k () == main == File:  == CRLF/UTF8 ===...`
  になる。起動時のバッファ名は `main` (C 版と同じ)。
  モード行は常に SGR 7 (反転) で描画し、直後に SGR 0 で解除するため、
  `%*=%c` 等に変更がないキー入力では再描画されず反転状態が保持される。
- その他: undo/redo、`M-X`、ヘルプ、`C-X =`、変数・モード設定

## テスト対応

C 版 `make test` の 18 バイナリと照合し、不足していた項目を
`tests/rust_parity.rs` に 25 件追加した (評価関数の 32bit 境界、JIS/SJIS の
半角カナ往復、行・undo のストレス、各モードのエッジケース、検索境界、
preview の部分行/CRLF、長行・日本語ファイル名、ガーター描画)。
追加テストで見つかった次の不具合も修正済み:
- `&add` / `&sub` / `&times` / `&div` / `&mod` / `&neg` / `&abs` / `&rnd`
  の 32bit オーバーフローと 0 除算を C 版と同じく `ERROR` に統一。
- `ldelete` が最終行末尾で残回数分だけ改行を kill バッファへ積んでいた問題。
- Markdown タスク一覧のチェック文字位置が 1 桁ずれていた問題。
- preview の CRLF 行に `\r` が残っていた問題。

## コードレビューで修正した項目

- `region_bytes()` が終端行 (`dot.line == lines.len()`) を指すと
  配列外参照でパニックしていた。`pos_to_offset()` も終端行のバイト位置を
  1 多く計算していた。
- 長い数字列の Markdown 順序リスト番号が `usize` オーバーフローで
  パニックしていた (saturating 演算に変更)。
- `filter` が予測可能な一時ファイル名を使い、シンボリックリンク /
  TOCTOU 攻撃に対して脆弱だった。stdin/stdout パイプ方式に変更。
- 正規表現の長いパターンが `match_here()` の再帰でスタック枯渇する
  可能性があった (1024 バイト上限を追加)。
- `intern_command()` が未知のコマンド名を `Box::leak` していた
  (静的 "noop" にフォールバック)。
- 全角数字の大小文字無視テーブルが `\0` に変換されていた (C 版 `ctab[]`
  生成時の文字リテラル解釈ミス)。全角０〜９が ASCII 数字へ正しく畳み込まれる。
- 終端行で `trim` / `detab` / `entab` を実行すると `dot.line == lines.len()`
  となり配列外参照でパニックしていた。
- `$md-heading-max=0` のとき `md_insert_heading` の `clamp(1, 0)` がパニックしていた。
- 空文字列のモード名を指定したとき `set_mode` がパニックしていた。
- Cargo clippy の警告 35 件を解消 (`-D warnings` で通過)。
- 評価関数の符号付き境界、スレッド poisoned mutex、32bit 吸入警告も修正。
- 後方検索がカウント付きで同じ一致を繰り返していた。
- `left` / `right` / `mid` がマルチバイト文字の途中でスライスしてパニック
  していた (文字境界に丸め、範囲外は空文字列扱い)。
- スクリプトの `dobuf` / `execproc` に再帰深さ上限 (32) を追加。
- ユニバーサル引数の自己挿入を 100 万バイトで頭打ちにし、1 回の
  `Vec::splice` に変更。
- モードラインの `=` 埋めを 4096 桁で頭打ちにし、巨大な画面幅で
  大量確保しないようにした。
- パスワード入力 (`set-strong-key` / `setcrkey` / ファイル読込) を
  エコーなしプロンプトに変更。
- `Buffer` / `Snapshot` の `Debug` 実装で鍵とパスワードを `<redacted>` に。
  `disable_crypt()` で鍵・パスワードをゼロ埋めしてから破棄。
- 強暗号の復号失敗時に `decrypt_failed` を立て、ファイル読込時は
  読み取り専用で開き警告を表示 (誤パスワードで上書き保存する危険を防止)。
- Classic (Beaufort) は「難読化であり安全な暗号ではない」と警告表示。

## セキュリティ上の前提 (設計として許容している事項)

- ファイルコマンド (`filefind` / `viewfile` / `fileread` / `filewrite` /
  `execfile`) は `~/` 展開以外のパス制限を行わない。エディタとして
  プロセス権限内の全ファイルへアクセスできる必要があるため。
- シェル実行系 (`spawn` / `pipecmd` / `filter` / `preview_start` /
  `md-preview-command`) はユーザーが指定したコマンドをそのまま実行する。
  信頼できないスクリプト / ファイルから呼び出さないこと。
- スクリプトは信頼済みとして扱う。`get_bvar()` は未知の変数名を環境変数へ
  フォールバックするため、スクリプトから環境変数を読める。
- `system.rs` の raw FFI は Linux の定数値に依存する (他 Unix での
  ポータビリティは未対応)。
- 暗号化は研究用 / 難読化であり、機密情報の保護には使わない。

## C 版に合わせた項目

- キー割当て: C 版 `ebind.H_` との機械比較で欠落していた `C-X @` (pipecmd)、
  `C-X #` (filter)、`C-X D` / `M-S` (bktoshell)、`M-C-E` (execproc) を追加。
- 起動オプション: `-v` / `-V`、`-e` / `-E`、`-b`、`-g<line>`、`-x<command>`、
  `-f<code>` / `-t<code>`、`-r` / `-R`、`@file` を解釈。
- `~/.kemacsrc` を起動時に自動実行 (C 版の startup() 相当)。
- モードライン書式 `%t` / `%T`、`%<` / `%>` を実装。`$gmodeline` /
  `$bmodeline` は `$mode-line-format` の別名として読み書き可能。
- query-replace の `!` (以降すべて置換) を実装。

## 未移植 (C 版との差分)

`-s` / `-k` の起動オプション (対話プロンプトを伴うため raw mode 開始前に実行不可)、
BINARY モードの変換停止、制限モードのコマンド制限、query-replace の `^`、
対話的変数、`M-X` 補完、`auto-mode-for-file`、`^C` シェル一時脱出、
`JIS-NEW-ASCII` / `JIS-OLD-ROMAJI` などが残っています。
各項目の理由と代替手段は [`rust版とC言語版の違い.md`](rust版とC言語版の違い.md) を参照してください。

## ソース対応表

C 版のファイル名は移植元の参照です (C 版は別リポジトリにあります)。

| Rust | C 版 |
|------|------|
| `src/keycode.rs` | `estruct.H_` (キーコード・定数) |
| `src/buffer.rs` | `line.C_`, `buffer.C_`, `undo.C_` |
| `src/encoding.rs` | `kanji/kgetc.c`, `kanji/kputc.c` |
| `src/crypt.rs` | `crypt.C_` |
| `src/system.rs` | `spawn.C_`, `main.C_` (ASAVE) |
| `src/modes.rs` | `cmode.C_`, `python.C_`, `mdmode.C_` |
| `src/regex.rs` | `search.C_` の `mcstr()` / `amatch()` |
| `src/casefold.rs` | `search.C_` の `ctab[]` / `adjcase()` |
| `src/script.rs` | `exec.C_`, `eval.C_` |
| `src/commands.rs` | `ebind.H_`, 移動・編集コマンド群 |
| `src/commands/extra.rs` | `file.C_`, `window.C_`, `search.C_` の一部 |
| `src/display.rs` | `display.C_` の描画部分 |
| `src/input.rs` | `termio.C_`, `tcap.C_` の入出力部分 |
| `src/editor.rs` | `main.C_` の `recedit()` / `execute()` |

このドキュメントは実装 progress に合わせて更新してください。
