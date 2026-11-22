# kemacs Rust 版 CMODE 使用方法

kemacs Rust 版に組み込まれた **CMODE (`MDCMOD`)** は、C 言語 (C89/C90/C99/C11/C17/C23) の編集において、インデントと入力補助を自動的に行う機能です。C の構文を解析して適切なインデントを計算します。

Rust 版の実装は `src/modes.rs` にあります (`c_insert` / `c_indent` / `c_indent_region` / `c_detect_style` / `c_mode` / `c_follow_source` など)。C モード関連のコードはこの 1 ファイルに統合されています。ビルド・テストは `cargo build` / `cargo test` を使用します。

---

## 1. 機能概要

| 機能 | コマンド名 | キー | 説明 |
|------|-----------|------|------|
| 自動インデント (改行時) | `cinsert` (Rust: `c_insert`) | `Enter` | 前の行のブレース・括弧・キーワードに基づいて新しい行のインデントを自動計算 |
| 行の再インデント | `cindent` (Rust: `c_indent`) | `M-J` | カレント行のインデントを前行の文脈に基づいて修正 |
| 領域の再インデント | `cindent-region` (Rust: `c_indent_region`) | `M-x` | 選択リージョン内の各行を C 構文に基づいて再インデント |
| スタイル再検出 | `c-detect-style` (Rust: `c_detect_style`) | `M-x` | ファイル内の既存コードからブレーススタイルを自動検出 |
| モード切り替え | `c_mode` (Rust: `c_mode`) | `M-x` | CMODE の ON/OFF を切り替える |
| ソース追随切り替え | `c-follow-source` (Rust: `c_follow_source`) | `M-x` | 前行のインデントをそのまま尊重するモードを切り替える |

`M-x` で呼び出すコマンド名は `src/commands.rs` / `src/commands/extra.rs` のディスパッチに登録されています。Rust 版では `M-J` に `smart_indent` がバインドされ、`indent_auto()` がモードを見て CMODE なら `c_indent()`、Python Mode なら `py_indent()`、Markdown Mode なら `md_indent()` へ振り分けます。`Enter` も同様に `newline_auto()` 経由で `c_insert()` が呼ばれます。

---

## 2. CMODE の有効化

CMODE は **バッファローカル** なモードです。以下の方法で有効にできます。

### 2.1 ファイル拡張子による自動判定

`.c` / `.h` 拡張子のファイルを開くと、`src/modes.rs` の `auto_mode()` が自動的に CMODE を ON にします。C 版の `AUTOMODE` / `pattbl[]` に相当する拡張子判定が Rust 版の `auto_mode()` に組み込まれているため、追加の設定は不要です。判定はファイル名を小文字化して末尾を見るため、`.C` / `.H` も対象です。

Rust 版の `auto_mode()` が自動判定するのは `.c` / `.h` のみです。元の C 版ドキュメントにあった `.cpp` / `.hpp` / `.cc` / `.cxx` の自動適用 (`auto-mode-for-file`) は **未移植** です。これらのファイルでは後述の `M-x c_mode` で手動切り替えしてください。

ファイルを開く際、`$c-auto-detect` が TRUE に設定されている場合、CMODE はファイル内の既存コードから以下を自動検出します (Rust 版の自動検出は限定的です。詳細は 2.3 と 4.8 を参照):

1. **ブレーススタイル**: 独立行の `{` と行末の `{` の出現回数から K&R (スタイル 0) か Allman (スタイル 1) を判定
2. **タブ vs スペース、インデント単位 (2/4/8 スペース) の検出**: **未移植** (`$c-use-spaces` / `$c-indent-width` は既定値のまま)
3. **case ラベルのインデント (switch と同じか +1 レベルか) の検出**: **未移植**

### 2.2 手動で有効化

```
M-x c_mode    (c_mode と入力)
```

`M-x` は「名前を指定してコマンドを実行する」コマンドです。`c_mode` と入力すると、`src/modes.rs` の `c_mode()` がカレントバッファの CMODE (`MDCMOD` ビット) を ON/OFF 切り替えます。

引数を指定することもできます:

- `M-x c_mode` (引数なし): ON/OFF トグル
- `C-U 1 M-x c_mode`: CMODE を強制的に ON
- `C-U -1 M-x c_mode`: CMODE を強制的に OFF

`c_mode(ed, f, n)` は、数値引数が与えられた (`f == true`) ときは `n > 0` で ON/OFF を決め、引数なし (`f == false`) のときは現在のビットを反転します。切り替え result は `[C mode ON]` / `[C mode OFF]` として表示されます。

### 2.3 スタイルの自動検出

ファイルを開いた後、またはいつでも以下のコマンドでスタイルを再検出できます:

```
M-x c-detect-style
```

このコマンドは `src/modes.rs` の `c_detect_style()` を呼び出し、バッファ内の全行をスキャンして以下のアルゴリズムでブレーススタイルを推測します:

1. 各行の先頭空白をスキップした位置が `{` の行を数える (`own_line`)
2. 行末の非空白文字が `{` で、かつ行頭が `{` でない行を数える (`same_line`)
3. `own_line > same_line` なら `$c-brace-style = 1` (Allman)、そうでなければ `$c-brace-style = 0` (K&R) を設定
4. メッセージ領域に `[C style: N]` を表示

C 版にあった「ファイルの最初の 200 行をスキャンし、タブ/スペースのリーダーとインデント幅 (2/4/8) まで推定する」アルゴリズム、および Google スタイル (タブ + スペース) の判定は Rust 版では **未移植** です。Rust 版が検出できるのはスタイル 0 / 1 のみです。

---

## 3. キー操作

### 3.1 `Enter` (cinsert) -- 自動インデント

CMODE が ON のバッファで `Enter` を押すと、`newline_auto()` 経由で `c_insert()` が呼ばれ、前の行の内容に基づいて新しい行のインデントが自動計算されます。

**ルール:**

| 前の行の末尾 | アクション |
|-------------|-----------|
| `{` | 1 レベル深くインデント (`c-indent-width` スペースまたは 1 タブ) |
| `}` | 対応する `{` のインデントレベルに戻る (行内で対応が見つからなければ `scan_brace_level()` で後方スキャン) |
| `(` / `[` (未閉) | 継続行インデント (`c-cont-indent` 追加) |
| `if` / `for` / `while` 等のキーワード (ブレースなし) | 1 レベル深くインデント |
| `else` / `case` / `default` | `M-J` (`c_indent`) で対応する `if` / `switch` と同じレベルに揃えます (`case` は `+c-case-indent`)。`c_insert()` 自体に専用の分岐はありません |
| `#if` / `#ifdef` / `#ifndef` | `c-pp-indent + c-indent-width` だけ深くインデント |
| `#else` / `#elif` / `#endif` | `scan_brace_level()` で対応する `#if` のレベルに戻る |
| `do` | 次の行は 1 レベル深くインデント。`do-while` の `while ...;` は `M-J` (`c_indent`) で `do` と同じ列に揃えます |
| `#` (その他のプリプロセッサ) | Rust 版では専用の分岐は **未移植** (既定の「前行と同じインデント」として扱われます) |
| 上記以外 | 前の行と同じインデント |

`$c-follow-source` が TRUE のときは、上記のルールをすべて飛ばして直前の非空行のインデントをそのまま使用します (`c_insert()` の `c_follow_source` 分岐)。

`$c-brace-style = 7` (none) で末尾が `}` の場合は `scan_brace_level()` の結果を採用します。`$c-brace-style = 6` かつ `$c-brace-func` が有効で、関数定義の `)` の直後 (`is_function_definition()` が真) の場合は、開き `{` を次行の同じ列に置くために現在のインデントを維持します (Stroustrup スタイル)。

`scan_brace_level()` は C 版の内部関数 `c_scan_for_brace_level()` に相当し、`is_function_definition()` は C 版の内部関数 `c_is_function_definition()` に相当します。

```
if (x) {          ──Enter──▶
	do_something();      │
                    ▼    ← 1 レベル自動インデント (タブ)

for (int i = 0;   ──Enter──▶
     i < n;       │  ← 継続行インデント (c-cont-indent)
     i++) {       │
	process(i);   │
                ▼    ← 1 レベルインデント

}               ──Enter──▶
            ▼    ← } は if と同じレベル (0 列)
```

### 3.2 `M-J` (cindent) -- 行の再インデント

Rust 版では `M-J` に `smart_indent` がバインドされ、`indent_auto()` が CMODE を検出して `c_indent()` を呼び出します。コマンド名として `M-x cindent` から直接呼ぶこともできます。

カーソル行のインデントを、前の非空行の文脈に基づいて再計算します。

```
	if (x) {        ──M-J──▶
	do_something();          │
                         ▼  ← インデント修正
	do_something();
```

`c_indent()` は `else` / `case` / `default` / `}` のデデント、`$c-brace-style = 7` の `else`、`$c-brace-style = 6` + `$c-brace-func` の関数定義の `{` と `else`、`do-while` の `while ...;`、`#else` / `#elif` / `#endif` を処理します。

数値引数 `n` を付けると、現在行から下方へ `n` 行分まとめて再インデントします。

### 3.3 `M-x cindent-region` (cindent-region) -- 領域の再インデント

リージョン (範囲選択) 内の各行を、C のブロック構造に基づいて再インデントします。

Rust 版の `c_indent_region()` は、マークとカーソルの範囲を求めて各行に `c_indent()` を 1 行ずつ適用します。C 版の内部関数がインデントスタックを使って前方スキャンする方式は Rust 版では使われていません (**未移植**)。

**使用方法:**

1. `M-space` でマークを設定
2. カーソルを移動して再インデントしたい範囲を選択
3. `M-x` を押して `cindent-region` と入力 (別名 `c-indent-region` も使用可)

### 3.4 `}` (insbrace) -- 閉じブレース入力補助

Rust 版には C 版の `insbrace` に相当する入力補助コマンドは **未移植** です。`}` は通常の自己挿入として扱われ、対応する開きブレースへの整列は `M-J` (`c_indent()`) で行います。

元の C 版で「`c-auto-close-brace` が TRUE の場合に対応する行のインデントを再計算」していた処理は、`src/modes.rs` の `c_insert()` / `c_indent()` に引き継がれています (行末が `}` のときの後方スキャン)。ただし変数 `$c-auto-close` は `src/editor.rs` に定義され `set` から変更できますが、インデント処理からは参照されていません (**未使用 / 未移植**)。

タブストップまでのバックスペース処理も Rust 版にはありません。

### 3.5 `#` (inspound) -- プリプロセッサ `#` 入力補助

Rust 版には C 版の `inspound` に相当する入力補助コマンドは **未移植** です。`#` は通常の自己挿入として扱われます。

プリプロセッサ行のインデントは `Enter` (`c_insert`) と `M-J` (`c_indent`) の `#if` / `#ifdef` / `#ifndef` / `#else` / `#elif` / `#endif` 判定 (`starts_pp()`) で調整され、`$c-pp-indent` はこれらの分岐で参照されます。汎用の `#` を `$c-pp-indent` の位置に配置する処理は未移植です。

---

## 4. 設定変数 (Evar)

CMODE の動作は以下のエディタ変数 (`$c-*`) で制御します。`src/script.rs` の `set_bvar()` が `set` コマンドから変数名を受け取り、`Editor` の対応フィールド (`c_indent_width` など) へ反映します。

C 版の 11 個の変数のうち、Rust 版では `$c-auto-close-brace` → `$c-auto-close`、`$c-gnu-indent-offset` → `$c-gnu-indent`、`$c-brace-standalone-func` → `$c-brace-func` と名称が変わっています。

### 4.1 `$c-indent-width` (デフォルト: 4)

1 レベルのインデント幅を指定します (`Editor.c_indent_width`)。スペースモード (`$c-use-spaces` = TRUE) のときはスペース数、タブモード (`$c-use-spaces` = FALSE) のときはタブ幅の基準となります。

```
set $c-indent-width 2    (2 スペース/タブインデント)
set $c-indent-width 8    (8 スペース/タブインデント)
```

### 4.2 `$c-use-spaces` (デフォルト: FALSE)  -- タブがデフォルト

インデントにスペースを使用するかどうかを BOOL 値で指定します (`Editor.c_use_spaces`)。

- `FALSE` (デフォルト): **タブ**を使用 (タブ幅は `$tabsize` / `$tab-size` に依存。Rust 版では両方の名前が使えます)
- `TRUE`: スペースを使用

```
set $c-use-spaces FALSE   (タブモード -- デフォルト)
set $c-use-spaces TRUE    (スペースモード)
```

### 4.3 `$c-brace-style` (デフォルト: 0)

ブレーススタイルを指定します (`Editor.c_brace_style`)。`}` (閉じ括弧) のインデントターゲットと構造的インデントの解釈に影響を与えます。

| 値 | スタイル名 | 説明 | `}` のインデント |
|---|-----------|------|---|
| 0 | K&R | `{` は行末, `}` は独立行で制御文と同じレベル | 制御文レベル |
| 1 | Allman | `{` と `}` は独立行で制御文と同じレベル | 制御文レベル |
| 2 | GNU | `{` と `}` は独立行で半分インデント (2 カラム) | 制御文レベル + 2 |
| 3 | Linux | K&R と同じ (Linux カーネルスタイル, タブ使用) | 制御文レベル |
| 4 | Whitesmiths | `{` は独立行で本文と同じレベル | 本文レベル |
| 5 | 1TBS | K&R に近似, `else`/`}` を前の行にくっつける | 制御文レベル |
| 6 | Stroustrup | 関数定義: Allman, 制御文: K&R | ケースバイケース |
| 7 | **none** | CMODE はブレーススタイルを仮定せず、すべての `{` と `}` の位置をそのまま尊重 | ユーザが配置した場所から逆算 |

Rust 版の `c_insert()` / `c_indent()` が値を直接参照するのはスタイル **6** (`$c-brace-func` と併用) とスタイル **7** です。0 / 1 / 2 / 4 は `docs/Rust移植.md` でサポートが明記されているスタイルで、C 版と同じ意味で解釈されます。スタイル 3 (Linux) とスタイル 5 (1TBS) 専用の分岐は **未移植** です。

```
set $c-brace-style 0   (K&R スタイル)
set $c-brace-style 1   (Allman スタイル)
set $c-brace-style 7   (none -- すべてのブレース配置を尊重)
```

### 4.4 `$c-cont-indent` (デフォルト: 2)

継続行 (括弧内の改行等) の追加インデントをスペース数で指定します (`Editor.c_cont_indent`)。`bracket_balance()` で括弧が開いたままになっている行の次行に加算されます。

```
set $c-cont-indent 4    (継続行に 4 スペース/タブ追加)
```

### 4.5 `$c-case-indent` (デフォルト: 0)

`switch` 内の `case` / `default` ラベルのインデントを `switch` からの相対値で指定します (`Editor.c_case_indent`)。

- `0` (デフォルト): `case` は `switch` と同じレベル
- `4`: `case` は `switch` から 4 スペースインデント

`c_indent()` のデデント分岐 (`else` / `case` / `default` / `}`) で使用されます。

```
set $c-case-indent 4    (case を switch から 4 スペースインデント)
```

### 4.6 `$c-pp-indent` (デフォルト: 0)

プリプロセッサ指示文 (`#define` 等) のインデントをスペース数で指定します (`Editor.c_pp_indent`)。

- `0` (デフォルト): プリプロセッサは列 0 に配置 (C の標準 convention)

```
set $c-pp-indent 4    (#define を 4 スペースインデント)
```

### 4.7 `$c-auto-close` (デフォルト: TRUE)  -- C 版の `$c-auto-close-brace`

元の C 版では `$c-auto-close-brace` という名前でしたが、Rust 版では `$c-auto-close` (`Editor.c_auto_close`) に改名されています。

```
set $c-auto-close FALSE   (自動デデント無効 -- ただし下記のとおり未使用)
```

**注意:** Rust 版ではこの変数は `set` で変更できますが、`src/modes.rs` のインデント処理からは参照されていません。行末 `}` のデデントは `$c-auto-close` の値にかかわらず実行されます (**未移植 / 未使用**)。

### 4.8 `$c-auto-detect` (デフォルト: FALSE)

ファイルを開く際、または `M-x c-detect-style` 実行時に、ソースコード内の既存インデントからブレーススタイルを自動検出するかどうかを指定します (`Editor.c_auto_detect`)。

Rust 版では `src/modes.rs` の `auto_mode()` が `.c` / `.h` を開いたとき、`$c-auto-detect` が TRUE なら `c_detect_style()` を呼び出します。

```
set $c-auto-detect TRUE    (ファイルオープン時に自動検出)
```

**検出アルゴリズム (Rust 版):**

1. バッファ内の全行をスキャン
2. 先頭の空白を除いた最初の文字が `{` の行を数える (`own_line`)
3. 行末の非空白文字が `{` で、かつ行頭が `{` でない行を数える (`same_line`)
4. `own_line > same_line` → `$c-brace-style = 1` (Allman)、それ以外 → `$c-brace-style = 0` (K&R)

C 版にあった「最初の 200 行のリーダー分析によるタブ/スペース判定、`c_indent_width` の 2/4/8 推定、Google スタイル detection、検出不能時のスタイル 7 + タブ + 8 幅へのフォールバック」は **未移植** です。

### 4.9 `$c-follow-source` (デフォルト: FALSE)

CMODE が前の行のインデントをそのまま尊重する「ソース追随モード」を有効にします。ユーザが手動で設定したインデント幅を無視し、既存コードに対して「そのまま続ける」ことを目的とします (`Editor.c_follow_source`)。

```
set $c-follow-source TRUE
```

`c_insert()` の先頭で判定され、直前の非空行のインデントをそのまま新しい行に使用します。`M-x c-follow-source` (`c_follow_source()`) で対話的に ON/OFF を切り替えることもできます。

### 4.10 `$c-gnu-indent` (デフォルト: 2)  -- C 版の `$c-gnu-indent-offset`

元の C 版では `$c-gnu-indent-offset` という名前で、GNU スタイルで内部ブレースを半分インデントする際のカラム数を指定していました。Rust 版では `$c-gnu-indent` (`Editor.c_gnu_indent`) に改名されています。

```
set $c-gnu-indent 4
```

**注意:** Rust 版では `src/modes.rs` のインデント計算からこの値は参照されていません (**未移植 / 未使用**)。

### 4.11 `$c-brace-func` (デフォルト: FALSE)  -- C 版の `$c-brace-standalone-func`

元の C 版では `$c-brace-standalone-func` という名前で、Stroustrup スタイル (6) で関数定義の開きブレースを独立した行に配置するかどうかを指定していました。Rust 版では `$c-brace-func` (`Editor.c_brace_func`) に改名され、`docs/Rust移植.md` 記載のとおりスタイル 6 の完全実装に使用されています。

```
set $c-brace-func TRUE
```

`c_insert()` は関数定義の `)` の次行で現在列を維持し、`c_indent()` は独立行の `{` を関数定義行と同じ列に、`else` を対応する `if` と同じ列に揃えます。

**注意:** `src/script.rs` の `set_bvar()` は `c-brace-func` を扱わないため、`set` コマンドからこの値を変更することはできません (初期値 FALSE のまま)。設定はスクリプトではなくコード側で行う必要があります (**スクリプト設定は未移植**)。

### 4.12 `.kemacsrc` での永続設定

Rust 版は `~/.kemacsrc` を起動時に自動実行します。設定を適用するには、以下の内容を `~/.kemacsrc` に書いておくか、スクリプト内で `set` を実行してください (`src/script.rs` の `set_bvar()` が `$` 付き変数名を処理します)。

```lisp
; CMODE の永続設定 (スクリプトとして読み込む)
; デフォルト: タブを使用
set $c-indent-width 4
set $c-use-spaces FALSE
set $c-brace-style 0
set $c-cont-indent 4
set $c-case-indent 0
set $c-pp-indent 0
set $c-auto-close TRUE
set $c-auto-detect FALSE

; キーの再バインドは M-K (bindtokey) で対話的に行う
;   bind-to-key cindent M-J      ... スクリプトの bind-to-key は未移植
;   bind-to-key cindent-region C-M-J
;   bind-to-key c-detect-style C-M-D

; 自動モードは src/modes.rs の auto_mode() に組み込まれているため設定不要
;   auto-mode-for-file CMODE "\\.c$"   ... 未移植
;   auto-mode-for-file CMODE "\\.h$"
;   .cpp / .hpp / .cc / .cxx も未移植 (M-x c_mode で手動適用)
```

`M-J` は既定で `smart_indent` に割り当て済みなので、C モードでは追加のバインドなしで `c_indent()` が呼ばれます。

---

## 5. サポートされる C 機能

### 5.1 対応キーワード

**ブロック開始キーワード** (次の行を 1 レベルインデント):

```
if  for  while  switch  do  else
try  catch  finally   (C23)
```

これらは `src/modes.rs` の `is_ctrl_keyword()` で認識されます。

**デデントキーワード** (対応する親構造と同じレベル):

```
else  -- if と同じレベル
case  -- switch と同じレベル (または +c-case-indent)
default -- switch と同じレベル (または +c-case-indent)
```

### 5.2 コメント処理

- `/* */` ブロックコメント内の `{`, `(`, `[`, `#` は無視されます (`skip_comment_or_string()`)
- `//` 行コメント (C99) もサポートされています

### 5.3 文字列/文字リテラル

- `\"...\"` 文字列リテラル内の `{`, `(`, `[`, `#` は無視されます
- `'...'` 文字リテラルもサポートされています
- エスケープ Sequences (`\\\"`, `\\\\`, `\\n` 等) を正しく処理します

### 5.4 プリプロセッサ

| 指令文 | インデント動作 |
|--------|---------------|
| `#if` / `#ifdef` / `#ifndef` | 次の行を +1 レベル (`c-pp-indent + c-indent-width`) |
| `#else` / `#elif` | `#if` と同じレベル (`scan_brace_level()`) |
| `#endif` | `#if` のレベルに戻る (`scan_brace_level()`) |
| `#define` (継続行 `\`) | Rust 版では継続行の専用処理は **未移植** (通常行と同じ) |
| `#include` / `#pragma` / `#error` / `#warning` | 通常の文として扱われる |

### 5.5 C23 機能

| 機能 | インデント動作 |
|------|---------------|
| `[[attribute]]` | 属性リストは現在の行のインデントを維持 (専用処理なし) |
| `try` / `catch` / `finally` | `is_ctrl_keyword()` でブロック開始キーワードとして扱う |
| `_BitInt(N)` | 通常の型として扱う |

---

## 6. ブレーススタイル詳細

CMODE は波括弧の配置をユーザの責務としています。`$c-brace-style` は「`{` がどこにあるか」に基づいて「`}` や本文をどのようにインデントするか」を決定するヒントです。

### 6.1 スタイル 7 「none」

`$c-brace-style = 7` を設定すると、CMODE はどんなブレーススタイルでも受け入れます。`c_insert()` は末尾が `}` のとき `scan_brace_level()` で、`c_indent()` は `else` のとき `scan_matching_if()` (C 版の内部関数に対応する Rust 関数) で位置を逆算します。

- 対応する `{` が行末なら → `}` は `{` と同じレベル (K&R, 1TBS など)
- 対応する `{` が独立行なら → `}` は `{` 行と同じレベル (Allman, Whitesmiths など)
- GNU スタイルの場合 → `}` は `{` と同じレベル

```
set $c-brace-style 7   (none -- すべてのブレース配置を尊重)
```

### 6.2 スタイル 6 「Stroustrup」

関数定義では Allman スタイル (`{` on own line), 制御文では K&R スタイル (`{` at end of line)。`$c-brace-func` と組み合わせて使用します。`docs/Rust移植.md` によれば、C 版では変数だけで未使用だった機能が Rust 版で完全実装されています。

### 6.3 スタイル 5 「1TBS」

K&R に近く、`else` / `}` を前の行にくっつけるスタイルです。Rust 版には専用の分岐がないため、実質的にはスタイル 0 (K&R) と同じ扱いになります (**未移植**)。

---

## 7. 使用例

### 7.1 基本的なブロック構造 (K&R, タブ)

```c
if (x) {
	for (int i = 0; i < n; i++) {
		if (i > 5) {
			process(i);
		} else {
			skip(i);
		}
	}
} else {
	handle_error();
}
```

### 7.2 switch / case

```c
switch (value) {
case 1:
	do_one();
	break;
case 2:
	do_two();
	break;
default:
	do_default();
	break;
}
```

### 7.3 プリプロセッサ

```c
#if defined(DEBUG)
	#define LOG(x) printf(x)
#else
	#define LOG(x)
#endif
```

### 7.4 継続行

```c
result = function_call(arg1,
                       arg2,
                       arg3);
```

### 7.5 do-while

```c
do {
	process();
} while (condition);
```

### 7.6 C23 try/catch

```c
try {
	risky_operation();
} catch (err) {
	handle_error(err);
} finally {
	cleanup();
}
```

---

## 8. 実装の仕組み

ここで挙げる関数はすべて `src/modes.rs` にあります。C 版の内部関数名が分かるものは対応関係を併記します。

### 8.1 インデント計算アルゴリズム

CMODE は以下のアプローチでインデントを計算します:

1. **前の行の解析**: `last_nonws()` が前行 (またはカーソル位置まで) の末尾文字 (`{`, `}`, `(`, `[`, `;`, 等) をチェック
2. **コメント/文字列スキップ**: `skip_comment_or_string()` が `/* */`, `//`, `\"...\"`, `'...'` の内部を解析対象から除外
3. **キーワード認識**: `is_ctrl_keyword()` / `is_keyword()` が C の制御文キーワード (`if`, `else`, `for`, `while`, `switch`, `case`, `do`, `try`, `catch`, `finally`) を認識
4. **ブレースレベル追跡**: `scan_brace_level()` (C 版の内部関数 `c_scan_for_brace_level()` に相当) が後方スキャンで対応する `{` のインデントを検索
5. **プリプロセッサ対応**: `starts_pp()` が `#if` / `#endif` / `#else` / `#elif` のネストを追跡
6. **括弧バランス**: `bracket_balance()` が `()` / `[]` / `{}` の開き残りを数え、継続行インデントを決定
7. **関数定義判定**: `is_function_definition()` (C 版の内部関数 `c_is_function_definition()` に相当) が Stroustrup スタイルの対象行を判定

### 8.2 cindent-region のアルゴリズム

Rust 版の `c_indent_region()` は、リージョンの開始行から終了行まで `c_indent(ed, 1)` を順に呼び出します。各行のインデント決定は 8.1 の `c_indent()` と同じロジックです。

以下は C 版の内部関数が使っていたインデントスタック方式のアルゴリズムで、Rust 版には実装されていません (**未移植**):

```
1. 領域開始前の行からベースインデントを取得
2. スタックを初期化 (indent_stack[0] = base_indent, sp = 0)
3. 各行をスキャン:
   a. `}` があればスタックをポップ
   b. `{` があればスタックにプッシュ (col + c-indent-width)
   c. `case`/`default` は switch レベル + c-case-indent
   d. `else` は対応する if と同じレベル
   e. `#if`/`#ifdef`/`#ifndef` は pp_sp をインクリメント
   f. `#endif` は pp_sp をデクリメント
4. スタックに基づいて各行のインデントを計算
```

### 8.3 インデント単位の選択 (スペース vs タブ)

`indent_bytes()` 関数 (C 版の内部関数 `c_insert_indent()` に相当) が `$c-use-spaces` の値に基づいてインデントを生成します:

- `c_use_spaces = TRUE`: 指定された列数だけスペースを挿入
- `c_use_spaces = FALSE`: タブ (`$c-indent-width` で割り切れる部分) + スペース (余り部分) を挿入

```rust
// src/modes.rs の indent_bytes()
fn indent_bytes(cols: usize, use_spaces: bool, width: usize) -> Vec<u8> {
    if cols == 0 {
        return Vec::new();
    }
    if use_spaces {
        return vec![b' '; cols];
    }
    let w = width.max(1);
    let mut out = vec![b'\t'; cols / w];
    out.extend(std::iter::repeat_n(b' ', cols % w));
    out
}
```

C 版の `c_insert_indent()` はタブ幅に `tabsize` を使っていましたが、Rust 版の `indent_bytes()` は第 3 引数 `width` (通常は `$c-indent-width`) を使う点が異なります。

---

## 9. トラブルシューティング

| 症状 | 原因 | 解決方法 |
|------|------|---------|
| インデントが正しくない | `$c-indent-width` が 0 | `set $c-indent-width 4` |
| スペースが挿入される | `$c-use-spaces` が TRUE | `set $c-use-spaces FALSE` (タブを使う) |
| タブが挿入される | `$c-use-spaces` が FALSE | `set $c-use-spaces TRUE` (スペースを使う) |
| `case` が switch に揃わない | `$c-case-indent` が 0 以外 | `set $c-case-indent 0` |
| `}` が正しくない位置に | Rust 版では `$c-auto-close` は未使用、`$c-brace-style` の解釈違いが考えられる | `M-J` (`c_indent`) で整列。`set $c-brace-style 7` (none) も試す |
| CMODE が機能しない | `MDCMOD` ビットが off | `M-x c_mode` で ON |
| スタイルが検出されない | `$c-auto-detect` が FALSE | `set $c-auto-detect TRUE` または `M-x c-detect-style` (Rust 版はスタイル 0/1 のみ) |
| 自動検出でタブが使われない | Rust 版の検出はブレーススタイルのみ | `$c-use-spaces FALSE` を明示的に設定する |
| `.cpp` などで CMODE が ON にならない | `auto_mode()` は `.c` / `.h` のみ対応 | `M-x c_mode` で手動 ON |
| `.kemacsrc` が効かない | `~/.kemacsrc` 以外のパスを指定している | ファイルを `~/.kemacsrc` に置くか、`M-x execfile` で読み込む |
