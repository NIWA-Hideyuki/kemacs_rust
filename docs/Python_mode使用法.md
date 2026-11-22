# kemacs Rust 版 Python Mode 使用方法

kemacs Rust 版に組み込まれた **Python Mode (PYMODE)** は、Python のインデンテーションを自動的に計算・挿入・調整する機能です。コロン (`:`)、バックスラッシュ (`\`)、キーワード (`else:`, `elif:`, `except:`, `finally:`) を解析して適切なインデントを決定します。バックスラッシュ継続の扱いは 3.1 / 3.2 のとおりです。

Rust 版の実装は `src/modes.rs` にあります (`py_insert` / `py_indent` / `py_backspace` / `py_indent_region` / `py_mode` など)。Python モード関連のコードはこの 1 ファイルに統合されています。ビルド・テストは `cargo build` / `cargo test` を使用します。

---

## 1. 機能概要

| 機能 | コマンド名 | キー | 説明 |
|------|-----------|------|------|
| 自動インデント (改行時) | `pyinsert` (Rust: `py_insert`) | `Enter` | 行末のコロンに基づいて新しい行のインデントを自動計算 |
| 行の再インデント | `pyindent` (Rust: `py_indent`) | `M-J` | カレント行のインデントを前行の文脈に基づいて修正 |
| 領域の再インデント | `py-format-region` (Rust: `py_indent_region`) | `M-x` | 選択リージョン内の各行を Python 構文に基づいて再インデント |
| バックスペース (デデント) | `py-backspace` (Rust: `py_backspace`) | `Backspace` | インデント領域でバックスペースを押すと 1 インデントレベル分削除 |

Rust 版では `M-x` で呼び出すコマンド名が `src/commands.rs` / `src/commands/extra.rs` のディスパッチに登録されています。`M-J` には `smart_indent` がバインドされ、`indent_auto()` がモードを見て Python Mode なら `py_indent()` を呼び出します。`Enter` は `newline_auto()` 経由で `py_insert()` が呼ばれます。`Backspace` は `backdel` → `delete_cmd()` → `backspace_auto()` の順にディスパッチされ、Python Mode では `py_backspace()` に到達します。

---

## 2. Python Mode の有効化

Python Mode は **バッファローカル** なモードです。以下の方法で有効にできます。

### 2.1 ファイル拡張子による自動判定

`.py` 拡張子のファイルを開くと、`src/modes.rs` の `auto_mode()` が自動的に Python Mode (`MDPYMOD` ビット) を ON にします。C 版の `MAGIC` による拡張子判定に相当する処理が `auto_mode()` に組み込まれているため、追加の設定は不要です。判定は小文字化したファイル名の末尾を見るため、`.PY` も対象です。

### 2.2 手動で有効化

```
M-x py_mode    (py_mode と入力)
```

`M-x` は「名前を指定してコマンドを実行する」コマンド (namedcmd) です。`py_mode` と入力すると、`src/modes.rs` の `py_mode()` がカレントバッファの Python Mode (`MDPYMOD`) を ON/OFF 切り替えます。

引数を指定することもできます:

- `M-x py_mode` (引数なし): ON/OFF トグル
- `C-U 1 M-x py_mode`: 強制的に ON
- `C-U -1 M-x py_mode`: 強制的に OFF

`py_mode(ed, f, n)` は、数値引数が与えられた (`f == true`) ときは `n > 0` で ON/OFF を決め、引数なしのときは現在のビットを反転します。切り替え結果は `[Python mode ON]` / `[Python mode OFF]` として表示されます。

---

## 3. キー操作

### 3.1 `Enter` (pyinsert) — 自動インデント

Python Mode が ON のバッファで `Enter` を押すと、`py_insert()` が前行の内容に基づいて新しい行のインデントを自動計算されます。

**ルール:**

- 前行が `:` で終わる場合 → 1 インデントレベル (`py-indent-width`) 増やす (`last_nonws()` が `:` を検出)
- 前行が `\` (バックスラッシュ) で終わる場合 → 同じインデントを継承。`py_insert()` にバックスラッシュ専用の分岐はなく、既定の「前行と同じインデント」として処理されます (領域再インデントの `py_indent_region()` では `\` 終端も +1 レベルの判定に使われます)
- それ以外 → 前行と同じインデント

```
if True:          ──Enter──▶
    x = 1          │
                   ▼    ← 4 スペース自動挿入
```

### 3.2 `M-J` (py-indent) — 行の再インデント

Rust 版では `M-J` に `smart_indent` がバインドされ、`indent_auto()` が Python Mode を検出して `py_indent()` を呼び出します。コマンド名として `M-x pyindent` から直接呼ぶこともできます。

カーソル行のインデントを、前の非空行 (`prev_nonblank()`) の文脈に基づいて再計算します (`py_target()`)。

**ルール:**

- 前行が `:` で終わる → 1 レベル追加
- キーワード (`else:` / `elif:` / `except:` / `finally:`) → `is_dedent_kw()` が検出し、1 レベル削減
- バックスラッシングによる継続行 → `py_target()` に専用の分岐はなく、前行と同じインデントを継承

```
if True:
    x = 1
    else:         ──M-J──▶
                │    ▼
                │  else:   ← デデント (4 スペース削除)
```

数値引数 `n` を付けると、現在行から下方へ `n` 行分まとめて再インデントします。

### 3.3 `M-x py-format-region` (py-format-region) — 領域の再インデント

リージョン (範囲選択) 内の各行を、Python のブロック構造に基づいて再インデントします。

Rust 版の `py_indent_region()` は、各行について直前の行 (`li - 1`) のインデントを基準に再計算します。直前の行が `:` または `\` で終わる場合は +1 レベル、現在行が `else` / `elif` / `except` / `finally` のときは -1 レベルです。空行・空白行はスキップされ、処理後にマークが解除されます。

**使用方法:**

1. `M-space` でマークを設定 ("[マーク 設定]")
2. カーソルを移動して再インデントしたい範囲を選択
3. `M-x` を押して `py-format-region` と入力 (別名 `py_indent_region` も使用可)

### 3.4 `Backspace` (py-backspace) — デデント

Python Mode が ON のバッファで、行頭の空白領域内で `Backspace` を押すと、`py_backspace()` が 1 インデントレベル分の空白を削除します。

- スペースモード (`$py-use-spaces` = TRUE): `$py-indent-width` スペースを 1 回で削除 (`off` が幅より小さい場合はその分だけ)
- タブモード (`$py-use-spaces` = FALSE): 1 タブ (1 バイト) を削除

空白領域以外で `Backspace` を押した場合は、通常のバックスペース (1 文字削除) が動作します。

---

## 4. 設定変数

Python Mode の動作は以下の 2 つのグローバル変数で制御します。`src/script.rs` の `set_bvar()` が `set` コマンドから変数名を受け取り、`Editor` の対応フィールドへ反映します。

### 4.1 `$py-indent-width` (デフォルト: 4)

インデントの幅をスペース数で指定します (`Editor.py_indent_width`)。PEP 8 の推奨値は 4 です。

```
set $py-indent-width 2    (2 スペースインデント)
set $py-indent-width 8    (8 スペースインデント)
```

### 4.2 `$py-use-spaces` (デフォルト: TRUE)

インデントにスペースを使用するかどうかを BOOL 値で指定します (`Editor.py_use_spaces`)。

- `TRUE` (デフォルト): スペースを使用
- `FALSE`: タブを使用 (タブ幅は `$tabsize` / `$tab-size` に依存。Rust 版では両方の名前が使えます)

```
set $py-use-spaces FALSE   (タブモード)
set $py-use-spaces TRUE    (スペースモード)
```

### 4.3 `.kemacsrc` での永続設定

Rust 版は `~/.kemacsrc` を起動時に自動実行します。以下の内容を `~/.kemacsrc` に書いておくか、スクリプト内で `set` を実行してください (`src/script.rs` の `set_bvar()` が `$` 付き変数名を処理します)。

```lisp
set $py-indent-width 4
set $py-use-spaces TRUE
```

## 5. 使用例

### 5.1 基本的なインデント

```
if True:
    for i in range(10):
        if i > 5:
            print(i)
        else:
            print("skip")
```

### 5.2 .kemacsrc 設定

```lisp
; Python Mode の永続設定 (スクリプトとして読み込む)
set $py-indent-width 4
set $py-use-spaces TRUE
```
