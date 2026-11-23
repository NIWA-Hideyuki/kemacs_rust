# kemacs Markdown モード 仕様書 (Rust 版)

> C 版のソース / C ビルドは別リポジトリに移管済み。本ドキュメントは
> Rust 版 kemacs (`Cargo.toml`, `src/`) の実装に基づく。
> C 版のファイル名は移植元の参照として併記する。

**状態**: 実装済み -- MDMODE は `src/modes.rs` に実装済み
(Phase 1/2 の編集機能、conceal、アウトライン、外部プレビュー)。
行レベルの色付けと折りたたみは意図的に未実装
(折りたたみはアウトラインで代替。`src/modes.rs` の Markdown 実装冒頭の注記参照)。
**対象**: Rust 版 kemacs (バッファ内部は UTF-8 の `Vec<Vec<u8>>`、
`src/buffer.rs` / `src/encoding.rs`)。C 版の
`HANDLE_UTF=1` / `DEF_F_CODE=KS_UTF8` に相当する設定は不要。
**既存ドキュメント**: `docs/cmode使用法.md`, `docs/Python_mode使用法.md` と同形式
(移植状況は `docs/Rust移植.md` も参照)。ユーザー向けガイドは
`docs/Markdown_mode使用法.md`。

---

## 0. この文書について

kemacs で Markdown (`.md` / `.markdown`) を扱うためのモード **MDMODE** の仕様。
表示 (Display) と編集 (Editing) の両面から、既存の CMODE / PYMODE が実証済みの
アーキテクチャに乗せる形で設計する。

新規実装は最小限のフックで既存の編集系 (`src/editor.rs::Editor::execute`、
`src/commands.rs::dispatch`、`src/modes.rs` の `newline_auto` / `backspace_auto` /
`tab_auto` / `indent_auto`) と表示系 (`src/display.rs` の `frame` /
`render_line_concealed` / `changed_rows`、`src/input.rs::draw_diff`) に接続する。
既存の動作を壊さないことが最優先され、各機能は独立に ON/OFF できる `$md-*` 変数で
制御する。

本書は「実装済みの事実」と「未移植の項目」を区別して記述する。事実には
Rust の `ファイル名:行` 相当を参照した (関数名で示す)。

---

## 1. 目的と範囲

### 1.1 対象とするもの

- Markdown 方言: **CommonMark + GFM 拡張** (フェンスドコードブロック、表、
  タスクリスト、取り消し線)。
- 文字コード: UTF-8 (`src/buffer.rs` の行バイト列、`src/encoding.rs` の iconv 変換)。
- ファイル: `.md`, `.markdown` の自動判定 (`src/modes.rs::auto_mode()`)。

### 1.2 対象外とするもの

- 完全な Markdown パーサの実装 (kemacs は行指向のエディタであり、
  Markdown のネスト・参照・footnote 解決までは扱わない)。
- WYSIWYG なプレビュー画面そのもの (外部ビューア/ブラウザへの委譲)。
- 既存の C/Python モードとの統合 (同時 ON は可能だが、優先順位は後述の
  `indent_auto()` / `newline_auto()` の判定順による)。

### 1.3 用語

| 用語 | 意味 |
|------|------|
| マーカー | `#`, `>`, `- `, `1. `, `- [ ] ` など行頭に置く Markdown の構造記号 |
| conceal | 強調記号やリンク URL を画面上だけ非表示にする機能 (`src/display.rs`) |
| 行属性 | 色・反転など、表示行全体にかかる属性。Rust 版では `Frame` の `fg` / `bg` / `color_on` に相当 (行単位の切替は未移植) |
| オートモード | ファイル拡張子でモードを自動設定する仕組み (`src/modes.rs::auto_mode()`。C 版 `pattbl` に相当) |

---

## 2. 現状調査 (設計の前提)

### 2.1 モードの仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| モードビット | `MDMDMOD = 1 << 7` | `src/buffer.rs` |
| モード表示名 | `MODE_NAMES[7] = "MDMODE"` | `src/buffer.rs` |
| モード略字 | `MODE_CODES[7] = 'D'` | `src/buffer.rs` |
| 自動判定 | `auto_mode()` が拡張子を判定 | `src/modes.rs` |
| モードごとの改行 | `newline_auto()` が `MDMDMOD` を判定し `md_insert()` へ | `src/modes.rs` |
| モードごとのバックスペース | `backspace_auto()` が `MDMDMOD` を判定し `md_backspace()` へ | `src/modes.rs` |
| モードのオンオフ | `md_mode()` がビットを OR / AND NOT | `src/modes.rs` |
| モードの一覧表示 | `expand_modeline()` が `%m` / `%M` を展開 | `src/display.rs` |

`src/buffer.rs` のモードビットは `MDWRAP(0)`, `MDCMOD(1)`, `MDPYMOD(2)`,
`MDSPELL(3)`, `MDEXACT(4)`, `MDVIEW(5)`, `MDOVER(6)` の 7 個の基本エントリの後、
`MDMDMOD(7)`, `MDMAGIC(8)`, `MDCRYPT(9)`, `MDASAVE(10)`, `MDBINARY(11)` が続く。

**ルール**: 新しい基本モードは必ず `MDMDMOD` より**前**に追加する。
末尾に追加すると、既存のモードビットの位置が変わり、保存された設定や
既存のテストとの整合性が壊れる。`MODE_NAMES` / `MODE_CODES` も同じ順序で
追加する (配列長は固定 12)。

### 2.2 表示の仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| 仮想画面 | `Frame.rows` (`Vec<String>`) | `src/display.rs` |
| 行の描画 | `frame()` が `render_line()` / `render_line_concealed()` で行を生成 | `src/display.rs` |
| 表示更新 | `changed_rows()` + `input::draw_diff()` が差分のみ送出 | `src/display.rs`, `src/input.rs` |
| 文字出力 | `render_line()` (タブ展開、制御文字の `^X` 表示、全角幅) | `src/display.rs` |
| 反転表示 | モデルインは `input::draw()` が SGR 7 / SGR 0 で描画 (行単位の反転は未移植) | `src/input.rs` |
| 色 | `Frame.fg` / `Frame.bg` / `color_on` (`ColorSpec`、画面単位) | `src/display.rs`, `src/editor.rs` |
| ガター | `show_line_numbers` / `line_number_width` | `src/display.rs::frame()` |
| 横スクロール | 未移植 (行を画面幅で切り詰める `chars().take(cols)`) | `src/display.rs::frame()` |
| 全角幅 | `column_of()` / `unicode_width::UnicodeWidthChar` | `src/display.rs` |

**重要な制約**:

1. **色は画面 (Frame) 単位までしか持てない**。行ごと、ましてや行の一部だけ
   色を変える仕組みは無い (`$md-highlight` は未移植)。
2. モデルインは常に反転 (SGR 7) で描画され、直後に SGR 0 で解除される。
   本文行の反転要求は無い。
3. カーソル桁は元バイト位置→表示桁の対応表から求める
   (`render_line_concealed()` の返り値)。conceal してもカーソルはずれない。
4. ガターは行番号専用に予約されている。Markdown 用の追加ガターは作らない。

### 2.3 編集の仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| 行挿入/分割 | `Buffer::newline()` / `Buffer::insert_bytes()` | `src/buffer.rs` |
| undo 連携 | `Editor::execute_named()` が 1 コマンド = 1 スナップショット | `src/editor.rs` |
| リージョン | `Buffer::mark` / `Pos { line, off }` | `src/buffer.rs` |
| タブ | `tab_auto()` / `do_tab()`、`ed.tabsize` | `src/modes.rs` |
| 自動折り返し | `MDWRAP` は未移植。`fillpara` コマンドで手動整形 | `src/commands/extra.rs` |
| フェンス整合 | `md_is_fence()` / `md_in_fence_at()` | `src/modes.rs` |
| インデント計算 | `set_indent()` / `indent_bytes()` / `indent_cols()` | `src/modes.rs` |

C 版の CMODE / PYMODE のインデントヘルパーは file-scope `static` だったため
Markdown モードから直接呼べなかったが、Rust 版では `src/modes.rs` の共通ヘルパー
(`indent_bytes`, `indent_cols`, `set_indent`, `skip_ws` 等) として実装され、
C / Python / Markdown の全モードから利用できる。

---

## 3. 全体設計

### 3.1 モードビット

`src/buffer.rs` に基本モードの末尾として追加する (条件付きフラグの前)。

```rust
pub const MDOVER: u32 = 1 << 6;
pub const MDMDMOD: u32 = 1 << 7;  // Markdown mode: marker-aware editing
pub const MDMAGIC: u32 = 1 << 8;  // MAGIC は次のビット
```

`MODE_NAMES` / `MODE_CODES` も同じ位置に追加する (配列長 12)。

```rust
pub const MODE_NAMES: [&str; 12] = [
    "WRAP", "CMODE", "PYMODE", "SPELL", "EXACT", "VIEW", "OVER", "MDMODE", "MAGIC", "CRYPT",
    "ASAVE", "BINARY",
];
pub const MODE_CODES: [u8; 12] = *b"WCPSEVODMYAB";
```

`MODE_CODES[7]` は `'D'` で、既存の
`'W','C','P','S','E','V','O','M','Y','A','B'` と重複しない。

### 3.2 自動判定 (automode)

`src/modes.rs::auto_mode()` がファイル名の小文字化後に拡張子を判定する
(C 版 `pattbl` / `readin()` に相当)。

```rust
let bit = if lower.ends_with(".c") || lower.ends_with(".h") {
    Some(MDCMOD)
} else if lower.ends_with(".py") {
    Some(MDPYMOD)
} else if lower.ends_with(".md") || lower.ends_with(".markdown") {
    Some(MDMDMOD)
} else {
    None
};
if let Some(b) = bit {
    ed.buffers[bi].mode |= b;
}
```

### 3.3 ファイル構成

| ファイル | 内容 |
|----------|------|
| `src/modes.rs` | Markdown モードのコマンドとヘルパー (C: `mdmode.C_`) |
| `src/buffer.rs` | モードビット、行/バッファ、undo (C: `estruct.H_` のモード定数、`buffer.C_`, `undo.C_`。キーコードは `src/keycode.rs`、全体対応は `docs/Rust移植.md`) |
| `src/editor.rs` | コマンドディスパッチ、`$md-*` 値の保持 (C: `edef.H_`, `random.C_` のディスパッチ, `main.C_` の `recedit()` / `execute()` に相当) |
| `src/commands.rs` / `src/commands/extra.rs` | キー割当て、コマンド名、`M-x` ディスパッチ (C: `ebind.H_`, `efunc.H_`) |
| `src/script.rs` | `$md-*` 変数の get/set (C: `evar.H_`) |
| `src/search.rs`, `src/regex.rs`, `src/casefold.rs` | 検索、MAGIC 正規表現、大小文字無視 (C: `search.C_`) |
| `src/display.rs` | conceal 描画、モデルイン、差分描画 (C: `display.C_`) |
| `src/input.rs` | 端末入出力、描画の ANSI 送出 (C: `termio.C_`, `tcap.C_`) |
| `src/encoding.rs` | UTF-8 / SJIS / EUC-JP / JIS 変換 (C: `kanji/kgetc.c`, `kanji/kputc.c`) |
| `tests/rust_core.rs`, `tests/rust_parity.rs` | Rust の単体・結合・差分テスト (C: `tests/test_mdmode.c`) |
| `docs/Markdown_mode使用法.md` | ユーザー向け利用ガイド (別途) |

Rust 版は `Cargo.toml` でビルドするため、C 版の `mf.c` への `OBJS` / `SRCS` /
サフィックスルールの追加は不要です。ビルドは `cargo build`、テストは `cargo test`。

### 3.4 編集フックの接続点

既存のディスパッチは引数チェック後に行われ、kill フラグ付きの特殊パスでは
モード判定を避けている (二重 kill を防ぐため、C 版と同じ意図)。
Rust 版の接続点は次のとおり。

| 関数 | 既存のディスパッチ | Markdown の接続 |
|------|--------------------|----------------|
| `newline_auto()` | `newline` → `MDPYMOD` / `MDCMOD` | `MDMDMOD` → `md_insert()` |
| `backspace_auto()` | `backdel` → `MDPYMOD` | `MDMDMOD` → `md_backspace()` |
| `tab_auto()` | `tab` → 通常のタブ | `MDMDMOD` → `md_tab()` (`n == 1` のとき) |
| `indent_auto()` | `smart_indent` (M-J) | `MDMDMOD` → `md_indent()` |

### 3.5 複数モード同時 ON の優先順位

`MDMDMOD` と `MDCMOD` / `MDPYMOD` が同時に立つことは通常ないが、
リージョンのコピーやテンプレ挿入で起こりうる。`newline_auto()` /
`backspace_auto()` / `indent_auto()` では `MDMDMOD` を**先に**判定する
(Markdown は行構造を扱うため、C / Python の文法判定より優先する)。

---

## 4. 表示の仕様

Markdown の「表示」には 3 段階の選択肢がある。実装コストとリスクは
段階的に上がる。

| 段階 | 名称 | 内容 | Rust 版 |
|------|------|------|---------|
| L0 | 素の表示 | 変更なし。マーカーはテキストとして見える | 既定 (`$md-conceal=FALSE`) |
| L1 | 行属性 | 見出し・コードフェンス等を行単位で色付け/反転 | 未移植 |
| L2 | conceal | `**` やリンク URL などを画面上だけ非表示 | 実装済み (`src/display.rs`) |
| L3 | 折りたたみ | セクション/フェンスを畳む | 未移植 |

### 4.1 L0: 素の表示 (既定)

`$md-conceal=FALSE` のときは `render_line()` がそのまま使われる。
タブ展開、制御文字の `^X` 表示、全角幅処理 (`unicode_width`) が
Markdown の行にも適用される (C 版の `vtputc()` に相当する処理は
`src/display.rs::render_line()`)。

### 4.2 L1: 行レベルの色付け (未移植)

Rust 版の `Frame` は `fg` / `bg` / `color_on` を持つが、これは画面全体の
色設定であり、特定の行だけ色を変える仕組みは無い。
`$md-highlight` は未移植なので、以下の割り当ては参考 (C 版の設計案) にとどまる。

| 行の種類 | 色 (8 色モード) | 判定 |
|----------|----------------|------|
| 見出し H1 | 白 (7) | 行頭が `# ` 1 個 |
| 見出し H2 | 黄 (3) | `## ` |
| 見出し H3-H6 | シアン (6) | `### ` 〜 `###### ` |
| コードフェンス内 | 青 (4) | ` ``` ` で囲まれた区間 |
| 引用 | 緑 (2) | 行頭が `> ` |
| 水平線 | 赤 (1) | `---` / `***` / `___` |
| その他 | 画面既定 | |

**制限**: 行単位のため、見出しの中の `**強調**` だけを変色することはできない。
Rust 版ではこの色付け自体が未移植である。画面全体の色は
`$fcolor` / `$bcolor` と `toggle-color` で変更できる。

### 4.3 L1: リバースビデオによる強調 (未移植)

C 版で提案されていた、色を使わず行を反転させる方式。Rust 版では
モデルインのみが `src/input.rs::draw()` で SGR 7 / SGR 0 により反転され、
本文行の反転は未移植。`$md-highlight` のフォールバック動作も未移植である。

### 4.4 L2: conceal (マークアップ非表示)

#### 4.4.1 狙う対象 (Rust 版)

| 対象 | 表示 | 備考 |
|------|------|------|
| 強調 `**`, `__`, `~~` | 非表示 | 対応する組が行内にあるとき |
| 強調 `*`, `_`, `~` | 非表示 | 同上 |
| コード `` ` `` | 非表示 | 対応する組が行内にあるとき |
| リンク `[text](url)` | `[text](url)` → `text` | 括弧と URL も消える |
| バックスラッシュ記法 `\X` | `\` を非表示 | C 版のエスケープ扱いに相当 |

Rust 版は**行頭マーカー (`#`, `>`, リスト) を conceal しない**。
これは C 版の「行頭マーカーを同幅の空白で置き換える」方針とは異なる。
`src/display.rs::conceal_flags()` が行を走査して非表示バイトを決定する。

#### 4.4.2 実装方針

`src/display.rs::render_line_concealed(bytes, tabsize, conceal)` が
`conceal_flags()` で非表示にするバイトを決め、残したバイト列を `render_line()`
で描画する。返り値は `(表示文字列, 元バイト位置 → 表示桁 の対応表)`。
`frame()` はこの対応表を使ってカーソル桁を計算するため、カーソル位置が崩れない。

```rust
// src/display.rs (要旨)
let (text, _) = render_line_concealed(b.line_or_sentinel(li), ed.tabsize, conceal);
let (_, map) = render_line_concealed(b.line_or_sentinel(dotline), ed.tabsize, conceal);
let cursor_col = (gutter + map[b.dot.off]) .min(cols.saturating_sub(1));
```

#### 4.4.3 最大の課題: カーソル列と表示列の不一致

conceal すると「見た目の列」と物理的なバイト位置がずれる。C 版では
以下の対策案が検討されたが、Rust 版では**案 2 (表示列マップを持つ)** を
採用済みである。

1. 行頭マーカー限定に絞る (C 版の推奨案)。Rust 版は行頭マーカーを隠さない点で
   近いが、加えて行内の強調も隠す。
2. **表示列マップを持つ (Rust 版の実装)**。行ごとに「実文字位置 → 表示列」の
   対応表を描画時に作り、カーソル桁計算で参照する。
3. conceal した分だけ `vtcol` を進めない (C 版のゼロ幅パス案)。
   Rust 版では `render_line_concealed()` が対応表を作るため不要。

#### 4.4.4 conceal を実装しない場合の代替

`$md-conceal=FALSE` なら全ての記号が表示されたままになる。`^X Y`
(`md_conceal_toggle`) または `C-X A` で切り替える。

### 4.5 L3: 折りたたみ (未移植)

#### 4.5.1 対象 (C 版の設計案)

- セクション: 見出し行から、次の**同レベル以上**の見出しまでの行。
- コードフェンス: ` ``` ` の開始行から終了行まで。

#### 4.5.2 データ構造 (未移植)

Rust 版の `Buffer.lines` は `Vec<Vec<u8>>` であり、行に折りたたみフラグを
持たせる仕組みが無い。C 版の `l_folded` / `l_hidden` に相当する
`src/buffer.rs` の変更は未実装である。

```
// 未移植: 行フラグを追加するには Vec<Vec<u8>> の構造を拡張する必要がある
```

#### 4.5.3 影響範囲 (難所、未移植)

| 機能 | 影響 |
|------|------|
| `frame()` / `changed_rows()` | 隠れた行を描画しない |
| カーソル桁計算 | 隠れた行にカーソルが乗らないよう移動 |
| `forwchar` / `backchar` / `forwline` / `backline` | 隠れた行を飛ばす |
| 行番号表示 | 隠れた行で番号を飛ばすか否か |
| undo | 隠れた行への undo が壊れないか |
| 検索 (`search.rs`) | 隠れた行を検索するか |
| リージョン | 隠れた行を含んだリージョンのコピー/削除 |
| `md_outline` | 折りたたみ状態の表示 |

アウトライン (4.6) によるセクション間ジャンプで代替する。

### 4.6 アウトライン (見出し一覧)

折りたたみに代わる、低コストな構造ナビゲーション。

| コマンド | 内容 |
|----------|------|
| `md-outline` | バッファ内の見出し行を `*Outline*` バッファに列挙 |
| `md-goto-heading` | `Heading: ` に入力したタイトルから元の見出しへジャンプ |

実装 (`src/modes.rs`):

1. バッファ全体を走査し、`md_heading_level(line) > 0` の行を集める。
2. `*Outline*` バッファを作り、`"-" × レベル` を `(レベル-1)*2` だけ
   インデントして見出しテキストを並べる (`find_buffer_or_create()` +
   `set_window_buffer()`)。
3. `md-goto-heading()` は現行バッファの見出しを部分一致で検索し、
   `Pos::new(i, 0)` へ移動する。

C 版の「行番号 → `gotoline()`」による逆引きは未移植。Rust 版では
行番号は `dot.line + 1` (0-based + 1) で求め、
ガター描画は `src/display.rs::frame()` が担う。

### 4.7 テーブルの表示

表示側の特別処理は**不要**。等幅フォントでは `|` の位置が自然に揃う。

- 全角文字は `column_of()` / `unicode_width` で 2 桁として数える。
- タブは `render_line()` がタブストップに展開する。
- 長い行の横スクロールは未移植 (画面幅で切り詰める)。
- 表の整列 (`md-table-align`) は未移植。

### 4.8 モードライン拡張

`src/display.rs::expand_modeline()` が書式を展開する。

| 指定子 | 表示 | 意味 |
|--------|------|------|
| `%h` | — | 未移植 (見出しレベル) |
| `%H` | — | 未移植 (セクション見出し) |
| `%l` | `12` | 行番号 (MDLIST 表示は未移植) |
| `%f` / `%F` | ファイル名 | 基底名 / フルパス |
| `%*` | `*` / `=` / `%` | 変更 / 未変更 / VIEW |
| `%m` / `%M` | `D` / `MDMODE` | モード略字 / モード名 |
| `%p` / `%v` | `日本語μEMACS` / `2.1k` | プログラム名 / バージョン |
| `%c` | `CRLF/UTF8` | 改行種別 / 文字コード |
| `%=` / `%-` | `=` / 右端まで `=` | 区切り / 埋め |

既定書式は `== %p %v (%m) == %b == File: %F == %c %-` で、
`mode-line-format` 変数 (`C-X A` で設定) から変更できる。
モード名 `MDMODE` は `MODE_NAMES` への登録で `%m` / `%M` に自動で出る
(追加作業不要)。

### 4.9 外部プレビュー

| コマンド | 内容 |
|----------|------|
| `md_preview` | `$md-preview-command` をバックグラウンドで実行し、出力を `*Preview*` バッファに表示 |

- 実装: `src/commands/extra.rs` の `md_preview` が
  `src/system.rs::preview_start()` を呼ぶ。
- 起動形式は `sh -c 'exec 2>&1; CMD'`。stdout / stderr を同じ pipe に繋ぎ、
  `preview_poll()` が非ブロック read で行を追記する。
- 終了時は `[preview finished]` を追加。`*Preview*` は VIEW モード。
- `$md-preview-command` の既定値は `grip %f` で、`%f` は現在のバッファの
  ファイル名へシェルクォートされて展開される。
- 未移植: 未保存バッファの一時ファイル書き出し、保存ごとの自動プレビュー
  (`$md-preview-on-save`)。
- `^C` は `preview_abort()` でコマンドを停止しバッファを閉じる。

### 4.10 表示のパフォーマンス

`conceal_flags()` / `render_line_concealed()` は行表示のたびに呼ばれる。
1 行あたり数文字〜数十文字の走査で収める。差分描画
(`changed_rows()` / `needs_full_redraw()` / `input::draw_diff()`) により、
変更行・モード行・メッセージ行・カーソルのみを端末へ送出する。
`md_in_fence_at()` は現在行より前のフェンス数を数える方式で、
1 キーあたりのカーソル移動を全画面描画の約 25 個から 3 個へ削減している。

---

## 5. 編集の仕様

### 5.1 自動インデント (`md_insert`)

`newline_auto()` の `MDMDMOD` ディスパッチ先。Rust 版は
`src/modes.rs::md_insert()` に実装されている (C 版 `pyinsert()` に相当する設計)。

#### 5.1.1 遷移表

カーソルより前のテキストを `prev` とする。

| `prev` の構造 | 次の行の先頭 | 条件 |
|---------------|-------------|------|
| `#` 〜 `######` で始まる見出し | (なし) | マーカーとして認識しない |
| `- ` / `* ` / `+ ` | 同じ記号 + ` ` | `$md-auto-list` が TRUE |
| `- [ ] ` / `- [x] ` 等 | `- [ ] ` | チェック状態はリセット |
| `1. ` 〜 `999. ` | 番号 + 1 の `N. ` | `$md-auto-list` が TRUE (saturating) |
| `> ` | `> ` | 空の引用行は `>` のみ |
| ` ``` ` (開始) | 開始行と同じインデント | 記法は無効化 |
| ` ``` ` (終了) | (なし) | |
| `    ` (4 スペースのコードブロック) | 同じインデント | |
| それ以外 | `prev` と同じインデント | |

- リストの連続を途中で破る空行の後は、マーカーを引き継がない。
- フェンス内では `#` / `-` / `>` を**マーカーとして扱わない**
  (`md_in_fence_at()` で判定)。

#### 5.1.2 インデントの挿入

`indent_bytes(indent, ed.md_use_spaces, ts(ed))` でスペース/タブを挿入する。
`indent` は `indent_cols(&line[..off], ts(ed))` で求めたカレント行の表示列。
`$md-indent-width` は主にデデント / 再インデント / リスト操作で使われる
(C 版 `insert_indent()` に相当する処理は `src/modes.rs::indent_bytes()`)。

### 5.2 再インデント (`md_indent`, M-J)

- 現在の行の構造を再解釈し、正しいインデントに置き換える (`set_indent()`)。
- リストのネストは `$md-indent-width` 単位の階層として再計算する。
- 引用 `> ` は `md_quote_end()` で段数を維持し、本文インデントだけを調整する。
- 空行と `md_is_fence()` の行は触らない。

C / Python モードの同等実装は `c_indent()` / `py_target()` (`src/modes.rs`)。

### 5.3 バックスペース (`md_backspace`)

`backspace_auto()` の `MDMDMOD` ディスパッチ先。Rust 版は
`src/modes.rs::md_backspace()` に実装 (C 版 `py_backspace()` に相当)。

- カーソルより前が全て空白/タブなら、`$md-indent-width` 分 (タブ設定なら 1 文字)
  を削除して 1 段デデントする。
- 行頭マーカーの直後で、マーカーとカーソルの間が空白のみなら、マーカー 1 個分
  (`m.end - q`) を削除する。
- それ以外は `Buffer::back_delete()` に委ねる。
- `n != 1` や kill フラグ付きは通常パスに委ねる
  (二重 kill を防ぐため、モード判定は kill フラグ処理の**前**に置く)。

### 5.4 TAB (`md_tab`)

`tab_auto()` に `MDMDMOD` ディスパッチを追加する (n=1 のときだけ)。

| 状態 | 動作 |
|------|------|
| テーブル行内 | 次の `|` の直前へカーソル移動 (`md_table_next_cell()`) |
| リスト項目の行頭 | `$md-indent-width` 分インデント |
| それ以外 | 通常の `do_tab()` (タブ or スペース) |

### 5.5 見出し

| コマンド | 引数 | 動作 |
|----------|------|------|
| `md-insert-heading` | n | レベル n の見出し (`#` × n + ` `) を現在位置に挿入 (上限 `$md-heading-max`) |
| `md-heading-up` | n | 見出しレベルを +1 (最大 `$md-heading-max`)。見出しでなければ `# ` を挿入 |
| `md-heading-down` | n | 見出しレベルを -1 (0 で通常行に戻す) |
| `md-heading-cycle` | - | 通常行 → H1 → H2 → ... → 通常行 |

- 見出しレベルの判定は `md_heading_level()` に一元化する (先頭 3 文字までの
  スペースを許し、`#` 1〜6 個の後が空白または行末なら見出し)。
- `$md-heading-max=0` でもパニックしない (`clamp(1, max)`)。

### 5.6 リスト

| コマンド | 動作 |
|----------|------|
| `md-list-toggle` | 行頭マーカーを付加/除去 (C 版の `$md-list-bullets` 巡回は未移植) |
| `md-list-demote` | インデント +1 段 |
| `md-list-promote` | インデント -1 段 (0 未満にはしない) |
| `md-list-renumber` | バッファ全体の連続する `N. ` 行を 1 から採番 (リージョン非対応) |
| `md-task-toggle` | `- [ ]` ↔ `- [x]` (`[X]` にも対応) |

採番の振り直しは `md_list_renumber()` がバッファ全体を走査し、
`md_parse_marker()` が `kind == 1` (番号付き) を返した行を順に書き換える。

### 5.7 強調・コード・リンク

| コマンド | キーバインド | 動作 |
|----------|----------------|------|
| `md-bold` | `M-x` (既定なし) | リージョンを `**` で囲む |
| `md-italic` | `M-I` | リージョンを `*` で囲む |
| `md-code` | `M-x` | リージョンを `` ` `` で囲む |
| `md-strike` | `M-x` | リージョンを `~~` で囲む |
| `md-insert-link` | `M-x` | `Link text:` / `URL:` をプロンプトし `[text](url)` を挿入 |

`M-I` 以外は既定キーがなく、`M-K` (`bindtokey`) で割り当てる想定
(`M-B` は `backword`、`M-L` は `lowerword`、`M-A` は `apro` が確保済み)。

Rust 版の `wrap_markup()` はリージョンの両端にマーカーを挿入する。
以下の C 版のトグル規則は未移植である。

1. リージョンの外側がすでにその記号なら、記号を**除去**する (未移植)。
2. そうでなければ、記号を**付与**する (実装済み)。
3. リージョンがなくカーソル位置 only なら、単語を対象にする (未移植)。

`md-insert_link()` は `ed.prompt()` でテキストと URL を尋ねる
(C 版の `mlreply()` / リージョン利用に相当する処理は未移植)。

### 5.8 テーブル

| コマンド | 動作 |
|----------|------|
| `md-table-next-cell` | 次の `|` へ (`TAB` から呼ばれる) — 実装済み |
| `md-table-align` | 未移植 |
| `md-table-insert-row` | 未移植 |
| `md-table-insert-column` | 未移植 |
| `md-table-delete-row` | 未移植 |
| `md-table-delete-column` | 未移植 |

整列の手順は C 版の設計案のまま参考に残す (未移植):

1. 連続する表行の範囲を特定 (前後の空行/非 `|` 行まで)。
2. 各列の最大表示幅を `column_of()` で数える。
3. セパレータ行 (`|---|:--:|`) はアライメント記号を保持する。
4. 各行のセルに空格を詰めて `|` の位置を揃える。

### 5.9 タスクリスト

`md_task_toggle()` は `[ ]` ↔ `[x]` を切り替える。`[X]` (大文字) にも対応する。
`- [ ]` の行で Enter を押すと `md_insert()` が `- [ ] ` を継続する。

### 5.10 引用

| コマンド | 動作 |
|----------|------|
| `md-quote-region` | リージョンの各行に `> ` を付加 (解除トグルは未移植) |
| `md-quote-toggle` | カレント行を引用にする/やめる |

### 5.11 フェンス

| コマンド | 動作 |
|----------|------|
| `md-insert-fence` | ` ```lang ` と ` ``` ` を対で挿入し、間にカーソルを置く (言語はプロンプト) |
| `md-goto-fence` | カレント行より後ろの次の ` ``` ` へジャンプ (前方探索のみ) |

`md_is_fence()` が先頭の空白を飛ばして 3 個以上のバッククォートを検出する。
C 版の `getfence()` は C 版の内部関数であり、Rust 版では `md_is_fence()` と
前方探索 (`md_goto_fence()`) で同等の処理を実装している
(後方探索は未移植)。

### 5.12 自動ペア

`$md-auto-pair` が TRUE のとき、`*`, `` ` ``, `[`, `_`, `~` の自己挿入を
対で挿入する (`modes::md_auto_pair_char()` / `modes::md_self_insert()`)。

**実装箇所**: Rust 版では `Editor::execute` / `commands::dispatch` の自己挿入パスで
`MDMDMOD && ed.md_auto_pair` を判定し、`md_self_insert()` を呼ぶ。
C 版の `insbrace()` / `inspound()` は C 版の内部関数であり、
Rust 版では `src/modes.rs::c_insert()` に相当処理 (波括弧の整合) がある。

```rust
// src/modes.rs (要旨)
pub fn md_self_insert(ed: &mut Editor, c: u8, n: i32) -> Status {
    let pair = if c == b'[' { b']' } else { c };
    ed.buffer_mut().insert_repeat(n.max(1) as usize, c);
    ed.buffer_mut().insert_repeat(n.max(1) as usize, pair);
    ed.buffer_mut().dot.off = ed.buffer_mut().dot.off.saturating_sub(n.max(1) as usize);
    Status::True
}
```

### 5.13 領域操作 (未移植)

| コマンド | 動作 |
|----------|------|
| `md-indent-region` | 未移植 |
| `md-dedent-region` | 未移植 |
| `md-format-region` | 未移植 |

参考: Python モードの `py_indent_region()` (`src/modes.rs`) は存在するが、
Markdown 用の「リスト階層スタック + フェンス状態」を持つ版は未実装。

### 5.14 保存時処理 (未移植)

| 機能 | 内容 |
|------|------|
| `$md-table-align-on-save` | 未移植 |
| 自動プレビュー (`$md-preview-on-save`) | 未移植 |

Rust 版の `filesave` は `src/commands/extra.rs` に実装されているが、
C 版の `write-file-hooks` に相当する保存時フックは無い。

---

## 6. 設定変数

Rust 版では `src/editor.rs` の `Editor` フィールドとして保持し、
`src/script.rs::get_bvar()` / `set_bvar()` 経由の `C-X A` (`setvar`) で設定する
(C 版 `evar.H_` の `envars[]` / hook に相当)。

| 変数 | 型 | 既定 | 意味 |
|------|----|------|------|
| `$md-indent-width` | usize | 2 | リスト/引用のインデント幅 |
| `$md-use-spaces` | bool | true | スペースを使う (false でタブ) |
| `$md-auto-list` | bool | true | 改行時にリストマーカーを継続 |
| `$md-auto-pair` | bool | false | 強調記号を自動ペア |
| `$md-conceal` | bool | false | 強調記号とリンク URL を非表示 |
| `$md-heading-max` | usize | 6 | 見出しレベルの上限 |
| `$md-preview-command` | str | `grip %f` | プレビューコマンド (`%f` はファイル名へ展開) |

**未移植の変数**: `$md-highlight`, `$md-list-bullets`, `$md-fence-lang`,
`$md-preview-on-save`, `$md-table-align-on-save`。設定しても動作に影響しない。

`C-X A` は「Variable to set: 」と「Value: 」を順にプロンプトし、
入力された名前の先頭の `$` / `%` を取り除いて `set_bvar()` に渡す。
`.kemacsrc` の自動読み込みは Rust 版には無い。

---

## 7. キーバインド

`src/commands.rs::default_bindings()` が C 版 `ebind.H_` の KEYTAB に相当する。
`Editor::execute()` は `keymap` (HashMap) で引くため、線形走査の順番は
正しさに影響しない (C 版と同じく先にマッチしたものが勝つ)。

**META キー**: `src/keycode::meta()` で `META|'I'` のように大文字で登録する
(C 版の `getcmd()` の大文字化に相当)。

**CTLX キー**: `^X l` と `^X L` は別キーとして共存する
(`toggle_line_numbers` / `set_line_number_width` がその例)。

| キー | コマンド | 備考 |
|------|----------|------|
| `C-M` (Enter) | `newline` → `mdinsert` | `newline_auto()` 経由で自動 |
| `Backspace` | `backdel` → `md-backspace` | `backspace_auto()` 経由で自動 |
| `TAB` | `tab` → `md-tab` | `tab_auto()` 経由で自動 |
| `M-J` | `smart_indent` → `mdindent` | `indent_auto()` がモードを判定 |
| `M-I` | `md_italic` | 既定キーあり |
| `M-O` | `md_outline` | 既定キーあり |
| `M-Y` | `md_preview` | 既定キーあり |
| `^X G` | `md_insert_heading` | `ctlxlctl(b'G')` |
| `^X H` | `md_heading_up` | `ctlxlctl(b'H')` |
| `^X J` | `md-table-align` | 未移植 |
| `^X Q` | `md-fold-toggle` | 未移植 |
| `^X Y` | `md_conceal_toggle` | `ctlx(b'Y')` |

**配置の制約**:

- `META` グループはアルファベット順: `M-I` (0x49), `M-J` (0x4A),
  `M-O` (0x4F), `M-Y` (0x59)。
- `CTLX` グループは大文字順: `^X G` (0x47), `^X H` (0x48) は
  `^X F` (0x46) と `^X I` (0x49) の間、`^X Y` (0x59) は
  `^X X` (0x58) と `^X Z` (0x5A) の間。
- モード固有のキーは C 版では `#if MDMDMOD` で囲まれていたが、
  Rust 版では常にコンパイルされ、`indent_auto()` / `newline_auto()` が
  実行時にモードを判定する (プリプロセスの落とし穴は無い)。
- `M-J` は C / Python / Markdown で共通の `smart_indent` に割り当て済みで、
  `indent_auto()` が `MDMDMOD` → `MDPYMOD` → `MDCMOD` の順に振り分ける。

---

## 8. コマンド一覧 (`EXTRA` 登録)

`src/commands.rs` の `EXTRA` 配列に以下を列挙し、
`src/commands/extra.rs::dispatch()` に `crate::modes::*` へのマッピングを追加する。
`M-x` (`namedcmd`) は `known_command()` で名前を検証する。

| コマンド名 | Rust の対応関数 |
|------------|----------------|
| `md_mode` | `modes::md_mode` |
| `mdinsert` | `modes::md_insert` (`newline_auto()` 経由) |
| `mdindent` | `modes::md_indent` |
| `md-backspace` | `modes::md_backspace` |
| `md-tab` | `modes::md_tab` |
| `md-insert-heading` | `modes::md_insert_heading` |
| `md-heading-up` | `modes::md_heading_up` |
| `md-heading-down` | `modes::md_heading_down` |
| `md-heading-cycle` | `modes::md_heading_cycle` |
| `md-list-toggle` | `modes::md_list_toggle` |
| `md-list-demote` | `modes::md_list_demote` |
| `md-list-promote` | `modes::md_list_promote` |
| `md-list-renumber` | `modes::md_list_renumber` |
| `md-task-toggle` | `modes::md_task_toggle` |
| `md-bold` | `modes::md_bold` |
| `md-italic` | `modes::md_italic` |
| `md-code` | `modes::md_code` |
| `md-strike` | `modes::md_strike` |
| `md-insert-link` | `modes::md_insert_link` |
| `md-quote-toggle` | `modes::md_quote_toggle` |
| `md-quote-region` | `modes::md_quote_region` |
| `md-insert-fence` | `modes::md_insert_fence` |
| `md-goto-fence` | `modes::md_goto_fence` |
| `md-table-next-cell` | `modes::md_table_next_cell` |
| `md-outline` | `modes::md_outline` |
| `md-goto-heading` | `modes::md_goto_heading` |
| `md_conceal_toggle` | `modes::md_conceal_toggle` |
| `md_preview` | `system::preview_start` (`commands/extra.rs` 経由) |
| `md_self_insert` | `modes::md_self_insert` |
| `md-dedent-region` | 未移植 |
| `md-fold-toggle` | 未移植 |
| `md-format-region` | 未移植 |
| `md-indent-region` | 未移植 |
| `md-table-align` | 未移植 |
| `md-table-delete-column` | 未移植 |
| `md-table-delete-row` | 未移植 |
| `md-table-insert-column` | 未移植 |
| `md-table-insert-row` | 未移植 |

C 版の `CSTR("md-...")` 追加や `make` による `Css.c` 再生成、kpp の処理は
Rust 版には存在しない。

---

## 9. テスト計画

Rust 版のテストは `tests/rust_core.rs` と `tests/rust_parity.rs` に追加する
(C 版の direct-include ハーネス、`#include "../mdmode.C_"`、
`#define maindef` / `#define static` は使用しない)。

| テスト項目 | 内容 |
|------------|------|
| 自動インデント | `modes::md_insert` の見出し/リスト/番号リスト/タスクリスト/引用/フェンスの各遷移 |
| 番号リスト採番 | `modes::md_list_renumber` による `1. ` → `2. ` → `3. ` |
| フェンス内でのマーカー無効化 | `modes::md_in_fence_at()` が真のとき `#` をマーカーにしない |
| 再インデント | `modes::md_indent()` によるリスト階層、引用段数の修正 |
| デデント | `modes::md_backspace()` による行頭マーカーの 1 段削除 |
| 見出しレベル | `modes::md_heading_up` / `down` / `cycle` の境界 (1-6, max=0) |
| 強調トグル | `modes::md_bold()` 等によるリージョンの囲み (解除トグルは未移植) |
| テーブル整列 | 未移植 (`modes::md_table_next_cell()` のみテスト) |
| アウトライン | `modes::md_outline()` の見出し抽出と `*Outline*` バッファ |
| 色判定 | 対象外 (行レベルの色付けは未移植) |
| conceal | `display::render_line_concealed()` の強調/URL 非表示と列補正 |
| automode | `modes::auto_mode()` で `.md` / `.markdown` のビットが立つこと |
| プレビュー | `system::preview_start()` の部分行 / CRLF / `[preview finished]` |

ビルドとテストは次のとおり。

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

既存の Rust テストは全てパスする (現在の件数は `docs/Rust移植.md` を参照)。
新規テストはこれに破壊を与えないこと。テスト手順は `cargo test` に統一する。

---

## 10. 実装フェーズ

| Phase | 内容 | Rust 版の状況 |
|-------|------|---------------|
| 1 | モード骨格: ビット、automode、`md_mode`、`md_insert`、`md_indent`、`md_backspace` | 実装済み (`src/modes.rs`) |
| 2 | 構造編集: 見出し、リスト、強調、リンク、テーブル、タスクリスト、引用、フェンス、アウトライン | 実装済み (テーブル整列・領域操作・解除トグルは未移植) |
| 3 | 表示: 行レベル色/反転 (`$md-highlight`)、conceal (`$md-conceal`)、モードライン `%h` | conceal は実装済み (強調/URL)、色/反転/`%h` は未移植 |
| 4 | 高度機能: 折りたたみ、自動ペア、保存時フック、外部プレビューの自動実行 | 自動ペア/プレビューは実装済み、折りたたみ/保存時フックは未移植 |

Phase 1 だけでも「Markdown を Structured Text として編集する」
段階の価値がある。表示系は conceal が実装済みで、行レベルの色付けは
未移植のままである。

---

## 11. 制約・リスク・未決事項

### 11.1 技術的制約

1. **色は `Frame` 単位** (`src/display.rs`)。部分着色は行ごとの属性構造を
   持つ改修が必要 (大規模、未移植)。
2. **conceal は表示列とカーソル列の整合**が最大の難所だったが、
   `render_line_concealed()` の元バイト位置→表示桁の対応表で解決済み。
3. **折りたたみに行フィルタ機構が無い**。`Buffer.lines` は `Vec<Vec<u8>>` で、
   行フラグ追加と検索 / undo / リージョンへの影響が広い (未移植)。
4. **自己挿入は `Editor::execute` / `commands::dispatch` の直接挿入パス**。
   `md_self_insert()` を `MDMDMOD && md_auto_pair` で呼ぶ形で実現している。
5. **`M-J` の競合**は C 版では課題だったが、Rust 版では `smart_indent` →
   `indent_auto()` のモード判定で解消済み。
6. **ガター列の競合**: 行番号ガター (`show_line_numbers`) が既に使っており、
   Markdown 用ガターは作らない。
7. **文字コード**: Rust 版は UTF-8 バイト列を直接扱い、表示幅は
   `unicode_width` で計算する。表現不能文字 (例: SJIS の絵文字) は
   `KEMACS-U+XXXX;` 形式のエスケープで保存し、読み込み時に復元する
   (`src/encoding.rs`)。

### 11.2 デザイン上の未決事項

- フェンス内の構文ハイライト (言語別キーワード色) は**対象外**。
- 行レベルの色付けを追加するか、conceal のままにするか
  (`$md-highlight` は未移植)。
- 折りたたみを実装するか、アウトラインで代替するか (折りたたみは未移植)。
- 表の編集を「TAB でセル移動」に限定するか、本格的な表エディタにするか。
- プレビューを外部任せにするか、バッファに HTML を出すにとどめるか
  (現在は外部コマンド任せ、既定は `printf` フォールバック)。

### 11.3 既存モードとの共存

| モード | 共通点 | 相違点 |
|--------|--------|--------|
| CMODE | `c_insert()` / `c_indent()` / `c_mode()` (`src/modes.rs`) | Markdown はブレースを構造に使わない。行頭マーカー中心 |
| PYMODE | `py_insert()` / `py_indent()` / `py_backspace()` (`src/modes.rs`)、`$py-*` 変数 | Markdown はインデントが構文でなく「整形」。マーカー継続が主 |

---

## 12. 参考: 既存モードとの対応表

C 版の関数名は移植元の参照であり、Rust 版の対応実装を併記する。
Rust に対応するものが無いものは「C 版の内部関数」と明記した。

| Markdown モード | Rust の対応実装 | 参照 |
|-----------------|------------------|------|
| `md_insert` | `src/modes.rs::md_insert` | `src/modes.rs` |
| `md_indent` | `src/modes.rs::md_indent` | `src/modes.rs` |
| `md_backspace` | `src/modes.rs::md_backspace` | `src/modes.rs` |
| `md_format_region` | 未移植 (Python 用の `src/modes.rs::py_indent_region` が参考になる) | `src/modes.rs` |
| `md_mode` | `src/modes.rs::md_mode` | `src/modes.rs` |
| `md_goto_fence` | `src/modes.rs::md_goto_fence` / `md_is_fence` (C 版の内部関数 `getfence` に相当) | `src/modes.rs` |
| `md_insert` のインデント | `src/modes.rs::indent_bytes` / `indent_cols` (C 版の内部関数に相当) | `src/modes.rs` |
| `md_indent` の再インデント | `src/modes.rs::set_indent` / `indent_cols` (C 版の内部関数に相当) | `src/modes.rs` |
| 行の色付け | 未移植 (`src/display.rs::Frame` の `fg` / `bg`) | `src/display.rs` |
| 見出しの反転 | 未移植 (モデルインは `src/input.rs::draw` が SGR 7 / SGR 0) | `src/input.rs` |
| 行番号ガター | `src/display.rs::frame()` の `show_line_numbers` | `src/display.rs` |
| 変数 | `src/script.rs::get_bvar` / `set_bvar`、`src/editor.rs::Editor` | `src/script.rs`, `src/editor.rs` |
| 自動判定 | `src/modes.rs::auto_mode` (C 版 `pattbl` に相当) | `src/modes.rs` |
| 正規表現 / 大小文字無視 | `src/search.rs` / `src/regex.rs::Regex` / `src/casefold.rs::fold_char` (C 版 `search.C_` の `mcstr` / `ctab[]` は C 版の内部関数) | `src/search.rs`, `src/regex.rs`, `src/casefold.rs` |
| エンコーディング | `src/encoding.rs` (C 版 `kanji/kgetc.c`, `kanji/kputc.c` に相当) | `src/encoding.rs` |
| キー入出力 | `src/input.rs`, `src/keycode.rs` (C 版 `termio.C_`, `tcap.C_`, `estruct.H_` に相当) | `src/input.rs`, `src/keycode.rs` |
