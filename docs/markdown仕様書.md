# kemacs Markdown モード 仕様書

**状態**: 実装済み -- MDMODE は `mdmode.C_` として実装・出荷済み
(Phase 1/2 の編集機能、conceal、アウトライン、外部プレビュー)。
行レベルの色付けと折りたたみは意図的に未実装
(折りたたみはアウトラインで代替。`mdmode.C_` 冒頭の注記参照)。
**対象**: kEmacs 2.1K + UTF-8 対応ツリー (`HANDLE_UTF=1`, `DEF_F_CODE=KS_UTF8`)
**既存ドキュメント**: `docs/cmode使用法.md`, `docs/Python_mode使用法.md` と同形式

---

## 0. この文書について

kemacs で Markdown (`.md` / `.markdown`) を扱うためのモード **MDMODE** の仕様案。
表示 (Display) と編集 (Editing) の両面から、既存の CMODE / PYMODE が実証済みの
アーキテクチャに乗せる形で設計する。

新規実装は最小限のフックで既存の編集系 (`newline`, `backdel`, `tab`, `indent`) と
表示系 (`vtputc`, `updone`, `updall`) に接続する。既存の動作を壊さないことが
最優先され、各機能は独立に ON/OFF できる `$md-*` 変数で制御する。

本書は「実装済みの事実」と「新規提案」を区別して記述する。事実には
`ファイル名:行` を参照した。

---

## 1. 目的と範囲

### 1.1 対象とするもの

- Markdown 方言: **CommonMark + GFM 拡張** (フェンスドコードブロック、表、
  タスクリスト、取り消し線)。
- 文字コード: UTF-8 (既存の `HANDLE_UTF` 経路をそのまま使う)。
- ファイル: `.md`, `.markdown` の自動判定。

### 1.2 対象外とするもの

- 完全な Markdown パーサの実装 (kemacs は行指向のエディタであり、
  Markdown のネスト・参照・ footnote 解決までは扱わない)。
- WYSIWYG なプレビュー画面そのもの (外部ビューア/ブラウザへの委譲)。
- 既存の C/Python モードとの統合 (同時 ON は可能だが、優先順位は後述)。

### 1.3 用語

| 用語 | 意味 |
|------|------|
| マーカー | `#`, `>`, `- `, `1. `, `- [ ] ` など行頭に置く Markdown の構造記号 |
| conceal | マーカーや強調記号を画面上だけ非表示にする機能 |
| 行属性 | 色・反転など、表示行全体にかかる属性 (`v_rfcolor` / `VFREQ`) |
| オートモード | ファイル拡張子でモードを自動設定する仕組み (`pattbl`) |

---

## 2. 現状調査 (設計の前提)

### 2.1 モードの仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| モードビット | `enum bmode` の `bm_*` → `MDxxx` マクロ | `estruct.H_:254-285` |
| モード表示名 | `modename[]` | `edef.H_:206-241` |
| モード略字 | `modecode[]` | `edef.H_:244-` |
| 自動判定 | `pattbl[]`, `npatent` | `edef.H_:374-385` |
| モードごとの改行 | `newline()` が `MDCMOD` / `MDPYMOD` でディスパッチ | `random.C_:456-470` |
| モードごとのバックスペース | `backdel()` が `MDPYMOD` でディスパッチ | `random.C_:677-690` |
| モードのオンオフ | `c_mode()` / `py_mode()` がビットを XOR | `cmode.C_:1529`, `python.C_:633` |
| モードの一覧表示 | `%m` / `%M` が `modename` / `modecode` を展開 | `display.C_:1176-1196` |

`enum bmode` の並びは `bm_wrap, bm_cmod, bm_pmod, bm_spell, bm_exact, bm_view,
bm_over` の 7 個の無条件エントリの後、`#if MAGIC` / `#if CRYPT` / `#if ASAVE` /
`#if KANJI` の条件付きエントリが続く (`estruct.H_:254-285`)。

**ルール**: 新しい無条件モードは必ず条件付きエントリより**前**に追加する。
末尾に追加すると、条件付きフラグの有効/無効でビット位置が変わり、
ビルド間で不整合が起きる。

### 2.2 表示の仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| 仮想画面 | `VIDEO` 配列 (`vscreen` / `pscreen`) | `display.C_:35-44` |
| 行の描画 | `updone()` / `updall()` が `vtputc(lgetc(lp,i))` で書き込む | `display.C_:630-631, 671-672` |
| 表示更新 | `updateline()` が仮想/物理を比較して差分出力 | `display.C_:935-1010` |
| 文字出力 | `vtputc()` (タブ展開、制御文字、全角、オーバーフロー `$`) | `display.C_:146-220` |
| 反転表示 | `VFREV` / `VFREQ` (行単位) | `display.C_:50-51` |
| 色 | `v_rfcolor` / `v_rbcolor` (`#if COLOR` のみ) | `display.C_:37-41, 633-634, 682-683` |
| ガター | `gbound` 列 (行番号 `$linenumbers`, `^X l`) | `display.C_:297-317` |
| 横スクロール | `lbound` + `updext()` (長い行) | `display.C_:695-735, 836-` |
| 全角幅 | `char_width()` / `kcol_width()`、`TRAILER` マーカー | `display.C_:176-190` |

**重要な制約**:

1. **色は行単位までしか持てない**。`COLOR` は現在 `0` (`estruct.H_:22`) で、
   `COLOR_DEPTH` も `0` (`econfig.h:55`)。`COLOR=1` にすれば
   `v_rfcolor` / `v_rbcolor` が有効になるが、これは **VIDEO 行 1 行分**の色であり、
   行の中の一部だけ色を変えることはできない (`updateline()` は行全体を
   要求色で塗り直す、`display.C_:958-995`)。
2. ウィンドウ単位の色 (`w_fcolor` / `w_bcolor`) が既定値で、
   `updone` / `updall` は各行の要求色をウィンドウ色で上書きしている
   (`display.C_:633-634, 682-683`)。
3. 反転はモデルインが `VFCHG|VFREQ|VFCOL` を立てて使っている
   (`display.C_:1109`)。一方、本文行では `updone` / `updall` が
   `v_flag &= ~VFREQ` で反転要求を**クリア**している (`display.C_:624, 662`)。
   よって見出し行を反転させたい場合は、この 2 箇所で `VFREQ` を再設定する
   必要がある。
4. ガター (`gbound`) は行番号専用に予約されている。Markdown 用の追加ガターは
   作らない (列が足りなくなる)。

### 2.3 編集の仕組み

| 項目 | 実体 | 参照 |
|------|------|------|
| 行挿入/分割 | `lnewline()`, `linsert()`, `ldelete()` | `line.C_` |
| undo 連携 | `u_record()` を各編集関数が呼ぶ | `undo.C_` |
| リージョン | `REGION` 構造体, `getregion()` | `estruct.H_:291-295` |
| タブ | `tab()`, `tabsize` | `random.C_:261-272` |
| 自動折り返し | `$fillcol` + `MDWRAP` → `wrapword()` | `word.C_:23` |
| フェンス整合 | `getfence()` (C/Python 向け), `fmatch()` | `random.C_:920-1010` |
| インデント計算 | CMODE の `c_insert_indent()` / `c_delete_indent()` | `cmode.C_:407-473` |
| インデント計算 | PYMODE の `insert_indent()` / `delete_leading_whitespace()` | `python.C_:165-228` |

CMODE / PYMODE のインデントヘルパー (`insert_indent`, `get_indent_cols`,
`delete_leading_whitespace`) はどちらも file-scope `static` であり、
そのままでは Markdown モードから呼べない。Markdown モードでは同等の
ヘルパーを `mdmode.C_` に用意する (公開 API にはしない)。

---

## 3. 全体設計

### 3.1 モードビット

`estruct.H_` の `enum bmode` に、無条件エントリの末尾 (条件付きエントリの前)
へ追加する:

```c
    bm_over,
#define MDOVER  (1<<(int)bm_over)   /* overwrite mode            */
    bm_mdmod,
#define MDMDMOD (1<<(int)bm_mdmod)  /* Markdown mode: marker-aware editing */
#if  MAGIC
    bm_magic,
...
```

`edef.H_` の `modename[]` / `modecode[]` も同じ位置に追加する
(`#if` の内側に入れない):

```c
    = {  CSTR("WRAP"), CSTR("CMODE"), CSTR("PYMODE"), CSTR("SPELL"),
         CSTR("EXACT"), CSTR("VIEW"), CSTR("OVER"), CSTR("MDMODE"),
# if MAGIC
         CSTR("MAGIC"),
# endif
         ...
```

`modecode[]` には `'D'` を追加する (既存の
`'W','C','P','S','E','V','O','M','Y','A','B'` と重複しない)。

### 3.2 自動判定 (automode)

`edef.H_` の `pattbl` に 2 エントリ追加し、`npatent` を `INIT(2)` → `INIT(4)`
に変更する:

```c
    = {  { "\\.[ch]$",        MDCMOD },
         { "\\.py$",         MDPYMOD },
         { "\\.md$",         MDMDMOD },
         { "\\.markdown$",   MDMDMOD } }
```

パターンは `readin()` が `regexec()` でファイル名全体に照合する。

### 3.3 ファイル構成

| ファイル | 内容 |
|----------|------|
| `mdmode.C_` | 新規。Markdown モードのコマンドとヘルパー |
| `estruct.H_` | `bm_mdmod` / `MDMDMOD` 追加 |
| `edef.H_` | `modename`, `modecode`, `pattbl`, `npatent` |
| `efunc.H_` | `extern` 宣言 + `names[]` へのコマンド登録 |
| `evar.H_` | `$md-*` 変数 |
| `ebind.H_` | `keytab[]` へのキー登録 |
| `random.C_` | `newline()` / `backdel()` / `tab()` へディスパッチ追加 |
| `display.C_` | `updone` / `updall` / `modeline` へフック追加 |
| `mf.c` | `mdmode.o` を `OBJS` / `SRCS` / `CSRCS` / サフィックスルールに追加 |
| `tests/test_mdmode.c` | 新規ユニットテスト |
| `docs/Markdown_mode使用法.md` | ユーザー向け利用ガイド (別途) |

`mf.c` への追加は 4 箇所 (既存の `python.C_` / `cmode.C_` の並びを踏襲)。

### 3.4 編集フックの接続点

既存のフックは「引数チェックの後、kill フラグ処理の前」にモード判定を置く
(これにより二重 kill を防ぐ)。Markdown モードも同じ場所に置く。

| 関数 | 既存のディスパッチ | Markdown 追加 |
|------|--------------------|----------------|
| `newline()` | `MDCMOD`→`cinsert()`, `MDPYMOD`→`pyinsert()` | `MDMDMOD`→`mdinsert()` |
| `backdel()` | `MDPYMOD`→`py_backspace()` | `MDMDMOD`→`md_backspace()` |
| `tab()` | (モード別なし) | `MDMDMOD`→`md_tab()` |
| `indent()` | (モード別なし) | 要検討 (C-J でマーカー継続を行うか) |

### 3.5 複数モード同時 ON の優先順位

`MDMDMOD` と `MDCMOD` / `MDPYMOD` が同時に立つことは通常ないが、
リージョンのコピーやテンプレ挿入で起こりうる。`newline()` / `backdel()` では
`MDMDMOD` を**先に**判定する (Markdown は行構造を扱うため、
C/Python の文法判定より優先するのが自然)。

---

## 4. 表示の仕様

Markdown の「表示」には 3 段階の選択肢がある。実装コストとリスクは
段階的に上がる。

| 段階 | 名称 | 内容 | コスト |
|------|------|------|--------|
| L0 | 素の表示 | 変更なし。マーカーはテキストとして見える | 0 |
| L1 | 行属性 | 見出し・コードフェンス等を行単位で色付け/反転 | 小 |
| L2 | conceal | `**` や `#` などを画面上だけ非表示 | 大 |
| L3 | 折りたたみ | セクション/フェンスを畳む | 大 |

### 4.1 L0: 素の表示 (既定)

実装しない。Markdown は素のテキストでも可読であり、
`vtputc()` の既存のタブ展開 (`display.C_:163-165`)、全角幅処理
(`display.C_:176-190`)、オーバーフロー `$` (`display.C_:156-162`) が
そのまま使える。

### 4.2 L1: 行レベルの色付け

`updone()` / `updall()` で、描画している行がどの Markdown 要素に属するかを
判定し、`v_rfcolor` / `v_rbcolor` をウィンドウ色ではなく行固有の色に設定する。

```c
/* updone() / updall() の該当箇所 (display.C_:633-634, 682-683 など) */
#if COLOR
    if ((curbp->b_mode & MDMDMOD) && md_highlight) {
        int col = md_line_color(lp);      /* mdmode.C_ で実装 */
        if (col >= 0) {
            vscreen[sline]->v_rfcolor = col;
            vscreen[sline]->v_rbcolor = gbcolor;
        } else {
            vscreen[sline]->v_rfcolor = wp->w_fcolor;
            vscreen[sline]->v_rbcolor = wp->w_bcolor;
        }
    } else {
        vscreen[sline]->v_rfcolor = wp->w_fcolor;
        vscreen[sline]->v_rbcolor = wp->w_bcolor;
    }
#endif
```

色の割り当て案 (`$md-highlight` が TRUE のとき):

| 行の種類 | 色 (8 色モード) | 判定 |
|----------|----------------|------|
| 見出し H1 | 白 (7) | 行頭が `# ` 1 個 |
| 見出し H2 | 黄 (3) | `## ` |
| 見出し H3-H6 | シアン (6) | `### ` 〜 `###### ` |
| コードフェンス内 | 青 (4) | ` ``` ` で囲まれた区間 |
| 引用 | 緑 (2) | 行頭が `> ` |
| 水平線 | 赤 (1) | `---` / `***` / `___` |
| その他 | ウィンドウ既定 | |

**必要条件**: `estruct.H_` の `COLOR` を `1` にし、`econfig.h` の
`COLOR_DEPTH` を `1` 以上にする。既定ビルド (`COLOR=0`) では
`$md-highlight` を設定しても無効 (無害に無視)。

**制限**: 行単位のため、見出しの中の `**強調**` だけを変色するようなことは
できない。また `updateline()` は「色が変わったら行全体を塗り直す」
(`display.C_:958-995`) ため、色の境界で端末への再描画が増える。

### 4.3 L1: リバースビデオによる強調 (COLOR 不要)

色を使わず、行を反転させる方法。見出し行の可読性向上に効果的。

```c
/* updone() / updall() で、本文行の VFREQ を設定する */
vscreen[sline]->v_flag &= ~VFREQ;
if ((curbp->b_mode & MDMDMOD) && md_highlight &&
    md_line_is_heading(lp) && md_heading_level(lp) <= 2)
    vscreen[sline]->v_flag |= VFREQ;
```

`VFREV` / `VFREQ` の処理は `updateline()` が既に持っている
(`display.C_:958-995`) ため、新規の出力系コードは不要。

**注意点**:

- `updone()` / `updall()` は `VFREQ` を無条件にクリアしているため、
  上記の再設定が必須。
- モデルインと同じ反転を使うため、見出しとモデルインの区別が
  画面上つきにくい。H1/H2 のみに限定するのが安全。
- `$md-highlight` は「色が使えるときは色、使えないときは反転」という
  フォールバック動作にすると親切。

### 4.4 L2: conceal (マークアップ非表示)

#### 4.4.1 狙う対象

| 対象 | 表示 | 備考 |
|------|------|------|
| 行頭見出し `#` 1-6 個 | 非表示 | 行頭限定なら影響は小 |
| 行頭引用 `>` | 非表示 | 同上 |
| 行頭リストマーカー `- `, `* `, `+ ` | 非表示 | 行頭限定 |
| 行頭番号 `1. ` | 非表示 | 行頭限定 |
| 強調 `**`, `*`, `__`, `_` | 非表示 | 行内。カーソル位置に影響 |
| コード `` ` `` | 非表示 | 行内 |
| リンク `[text](url)` | `[text](url)` → `text` | 行内。url も消える |

#### 4.4.2 実装方針

描画ループを差し替える。`updone()` / `updall()` の

```c
for (i=0; i < llength(lp); ++i)
    vtputc(lgetc(lp, i));
```

を、Markdown モードかつ `$md-conceal` が TRUE のときだけ
`md_draw_line(sline, lp)` に置き換える。`md_draw_line()` は
`mdmode.C_` に実装し、行をスキャンしながら `vtputc()` を呼ぶ。
conceal する文字は**呼ばない** (ゼロ幅扱い)。

#### 4.4.3 最大の課題: カーソル列と表示列の不一致

`updpos()` は `w_doto` から `curcol` を直接計算する
(`display.C_:695-`)。conceal すると「見た目の列」と `curcol` が
ずれる。具体的には:

- 画面上のカーソル位置が正しくない。
- `vtputc()` の `vtcol` と `curcol` が一致しないため、
  行末のオーバーフロー `$` が誤表示される。
- `lbound` (横スクロール) の基準がずれる。

**対策案**:

1. **行頭マーカー限定に絞る** (推奨)。conceal するのは行頭の
   `#` / `>` / リストマーカーだけにし、行内マークアップはそのまま表示する。
   この場合、ズレは「行頭の分だけ左にずれる」だけで、
   行頭のマーカー長を引く補正 (`curcol -= md_marker_width(lp)`) で
   概ね吸収できる。ただし行頭マーカーはインデントの一部として
   意味を持つため、非表示にすると編集しにくい。
2. **表示列マップを持つ**。行ごとに「実文字位置 → 表示列」の対応表を
   描画時に作り、`updpos()` で参照する。正確だが、
   `LINE` にマップを持たせる (メモリ増) か、表示時に毎回計算する
   (CPU 増) 必要がある。
3. **conceal した分だけ `vtcol` を進めない** ように `vtputc()` の
   ゼロ幅パス (既存の `w == 0` 分岐、`display.C_:207-211`) を使う。
   ただしカーソルは物理位置に置かれるため、視覚的にわかりにくい。

**推奨**: Phase 3 では「行頭見出し `#` と引用 `>` を、
インデント幅分だけ空白で置き換える (完全には消さない)」という
中庸案にする。これなら桁のずれは固定で、補正が容易。

#### 4.4.4 conceal を実装しない場合の代替

`$md-conceal` を常に FALSE とし、マーカーは表示したままにする。
強調は色 (L1) で表現する。

### 4.5 L3: 折りたたみ

#### 4.5.1 対象

- セクション: 見出し行から、次の**同レベル以上**の見出しまでの行。
- コードフェンス: ` ``` ` の開始行から終了行まで。

#### 4.5.2 データ構造

既存には「行を表示・移動から隠す」仕組みがない。そこで `LINE` に
フラグを追加する (kpp が struct 内の `EXT/INIT` を剥がすため、
生の `int` で宣言する):

```c
typedef struct  LINE {
    struct  LINE *l_fp;
    struct  LINE *l_bp;
    int     l_size;
    int     l_used;
    int     l_folded;   /* 1: この行が折りたたみの起点 (隠す) */
    int     l_hidden;   /* 1: この行は折りたたまれている */
    Char    l_text[1];
} LINE;
```

`l_folded` は見出し行だけに立て、`l_hidden` は子行全体に立てる。

#### 4.5.3 影響範囲 (難所)

| 機能 | 影響 |
|------|------|
| `updone` / `updall` | 隠れた行を描画しない |
| `updpos` | 隠れた行にカーソルが乗らないよう移動 |
| `backchar` / `forwchar` / `forwline` / `backline` | 隠れた行を飛ばす |
| `line_number_of()` | 行番号を隠れた行で飛ばすか否か |
| undo | 隠れた行へのundoが壊れないか |
| 検索 (`isearch`) | 隠れた行を検索するか |
| リージョン | 隠れた行を含んだリージョンのコピー/削除 |
| `md-outline` | 折りたたみ状態の表示 |

**推奨**: Phase 4 に回す。それまでの代替として、アウトライン
(4.6) によるセクション間ジャンプで用が足りる。

### 4.6 アウトライン (見出し一覧)

折りたたみに代わる、低コストな構造ナビゲーション。

| コマンド | 内容 |
|----------|------|
| `md-outline` | バッファ内の見出し行を `*Markdown Outline*` バッファに列挙 |
| `md-goto-heading` | アウトラインの行から元の見出しへジャンプ |

実装:

1. バッファ全体を走査し、`^#{1,6} ` に一致する行を集める。
2. `*Markdown Outline*` を作り、`行番号  レベル  テキスト` の形で列挙。
3. アウトライン側で `md-goto-heading` を叩くと、
   アトリビュートとして覚えた行番号へ `gotoline()` で移動。

行番号は `line_number_of()` (`display.C_:432`) で取れる。
逆引き (アウトライン行 → 元の行) は、アウトライン行の先頭の数字を
パースするだけでよい (行挿入でずれる既知の限界)。

見出しのレベルは `md_heading_level(lp)` (1-6、見出しでなければ 0)。

### 4.7 テーブルの表示

表示側の特別処理は**不要**。編集側の `md-table-align` で
パイプ位置を揃えれば、等幅フォントでは自然に表が組まれる。

- 全角文字は `char_width()` で 2 桁として数える (既存)。
- タブは `vtputc()` がタブストップに展開する (既存)。
- 表が長い場合は `lbound` による横スクロールに既存で対応済み。

### 4.8 モードライン拡張

`modeline()` (`display.C_:1098-`) の書式に新しい指定子を追加する。

| 指定子 | 表示 | 意味 |
|--------|------|------|
| `%h` | `H2` | カレント行の見出しレベル (見出しでなければ空) |
| `%H` | 見出しテキスト | カレント行が属するセクションの見出し |
| `%f` | `FENCE` | カレント行がコードフェンス内 |
| `%l` | `MDLIST` | カレント行がリスト項目 |

追加の case は `modeline()` の `switch` に足すだけでよい
(`display.C_:1133-1222`)。値は `mdmode.C_` の
`md_current_heading()` / `md_in_fence(lp)` などから取る。

モード名 `MDMODE` は `modename[]` への登録で `%m` / `%M` に自動で出る
(追加作業不要)。

### 4.9 外部プレビュー

| コマンド | 内容 |
|----------|------|
| `md-preview` | `$md-preview-command` をバックグラウンドで実行し、出力を `*preview*` バッファに表示 |

- 既定コマンド案: `grip %f` (GitHub Flavored Markdown をローカルサーバで表示)。
- 代替: `pandoc -s %f -o %f.html && xdg-open %f.html`。
- 未保存バッファには一時ファイルを渡す (`make_temp_file()` → `writeout()` →
  コマンド → 削除)。
- 保存ごとの自動プレビューは、`filesave` にフックを入れる必要があり
  (既存の `write-file-hooks` 相当がない)、Phase 4 扱い。

### 4.10 表示のパフォーマンス

`md_line_color()` / `md_line_is_heading()` は行表示のたびに呼ばれる。
1 行あたり数文字〜数十文字の走査で収める。コードフェンスの状態は
「フェンスの開始行を覚えておく」方式にすると、毎回バッファ先頭から
走査する必要がなくなる (フェンスはネストしないため、
开始/終了の 2 状態で足りる)。

---

## 5. 編集の仕様

### 5.1 自動インデント (`mdinsert`)

`newline()` の `MDMDMOD` ディスパッチ先。PYMODE の `pyinsert()`
(`python.C_:231-290`) と同型。

#### 5.1.1 遷移表

カーソルより前のテキストを `prev` とする。

| `prev` の構造 | 次の行の先頭 | 条件 |
|---------------|-------------|------|
| `#` 〜 `######` で始まる見出し | (なし) | |
| `- ` / `* ` / `+ ` | 同じ記号 + ` ` | `$md-auto-list` が TRUE |
| `- [ ] ` / `* [x] ` 等 | `- [ ] ` | チェック状態はリセット |
| `1. ` 〜 `999. ` | 番号 + 1 の `N. ` | `$md-auto-list` が TRUE |
| `> ` | `> ` | 空の引用行は `>` のみ |
| ` ``` ` (開始) | 開始行と同じインデント | 記法は無効化 |
| ` ``` ` (終了) | (なし) | |
| `    ` (4 スペースのコードブロック) | 同じインデント | |
| それ以外 | `prev` と同じインデント | |

- リストの連続を途中で破る空行の後は、マーカーを引き継がない
  (`prev` が空行のときは 0 インデント)。
- フェンス内では `#` / `-` / `>` を**マーカーとして扱わない**
  (`md_in_fence()` で判定)。

#### 5.1.2 インデントの挿入

`$md-indent-width` (既定 2) と `$md-use-spaces` (既定 TRUE) に従い、
`insert_indent(cols)` でスペース/タブを挿入する。
`py_indent_width` / `py_use_spaces` と同じパターン
(`python.C_:165-228`)。

### 5.2 再インデント (`mdindent`, M-J)

- 現在の行の構造を再解釈し、正しいインデントに置き換える。
- リストのネストは `$md-indent-width` 単位の階層として再計算する。
- 引用 `> ` は段数を維持し、本文インデントだけを調整する。
- フェンス内は触らない。

`pyindent()` (`python.C_:293-`) の「前行を参照してデデント/インデント」
と同じ考え方。CMODE の `cindent()` (`cmode.C_:1077`) の方が近い
(スタックを持つリージョン処理)。

### 5.3 バックスペース (`md_backspace`)

`backdel()` の `MDMDMOD` ディスパッチ先。`py_backspace()`
(`python.C_:379-`) と同型:

- カーソルより前が全て空白/マーカーで、かつ先頭が
  リストマーカー/引用なら、1 段 (マーカー 1 個分) デデントする。
- それ以外は通常の `backchar()` + `ldelete()`。
- `n != 1` や kill フラグ付きは通常パスに委ねる
  (二重 kill を防ぐため、モード判定は kill フラグ処理の**前**に置く)。

### 5.4 TAB (`md_tab`)

`tab()` に `MDMDMOD` ディスパッチを追加する。

| 状態 | 動作 |
|------|------|
| テーブル行内 | 次の `|` の直前へカーソル移動 (`md-table-next-cell`) |
| リスト項目の行頭 | `$md-indent-width` 分インデント |
| それ以外 | 既存の `tab()` (タブ or スペース) |

### 5.5 見出し

| コマンド | 引数 | 動作 |
|----------|------|------|
| `md-insert-heading` | n | レベル n の見出し (`#` × n + ` `) を現在位置に挿入 |
| `md-heading-up` | n | 見出しレベルを +1 (最大 6) |
| `md-heading-down` | n | 見出しレベルを -1 (最小 1、0 で通常行に戻す) |
| `md-heading-cycle` | - | 通常行 → H1 → H2 → ... → 通常行 |

- 見出しでない行では `md-heading-up` は行頭に `# ` を追加する。
- 見出しレベルの判定は `md_heading_level()` に一元化する。

### 5.6 リスト

| コマンド | 動作 |
|----------|------|
| `md-list-toggle` | 箇条書き記号を `$md-list-bullets` の中で巡回 (`-` → `*` → `+`) |
| `md-list-demote` | インデント +1 段 |
| `md-list-promote` | インデント -1 段 (0 未満にはしない) |
| `md-list-renumber` | 番号リストの採番を振り直す |
| `md-task-toggle` | `- [ ]` ↔ `- [x]` |

採番の振り直しはリージョン内の連続する `N. ` 行を走査し、
1 から順に書き換える。既存の `REGION` と `forwline()` で実装できる。

### 5.7 強調・コード・リンク

| コマンド | キーバインド案 | 動作 |
|----------|----------------|------|
| `md-bold` | `M-x` (既定なし) | リージョンを `**` で囲む/外す |
| `md-italic` | `M-I` | リージョンを `*` で囲む/外す |
| `md-code` | `M-x` | リージョンを `` ` `` で囲む/外す |
| `md-strike` | `M-x` | リージョンを `~~` で囲む/外す |
| `md-insert-link` | `M-x` | `[text](url)` を挿入 (リージョンがあれば text に) |

既定のキーは `M-I` (italic) のみ。`md-bold` などは `M-x bind-to-key` で
ユーザが好みのキーに割り当てる想定 (`M-B` は `backword`、`M-L` は
`lowerword`、`M-A` は `apro` が既に確保済み)。

トグル規則:

1. リージョンの外側がすでにその記号なら、記号を**除去**する。
2. そうでなければ、記号を**付与**する。
3. リージョンがなくカーソル位置 only なら、単語を対象にする
   (既存の `lowerword` / `upperword` が単語単位の操作性の参考)。

`md-insert-link` は `mlreply()` で URL を尋ね、
`[テキスト](URL)` を挿入する。テキストが空なら URL をそのまま使う。

### 5.8 テーブル

| コマンド | 動作 |
|----------|------|
| `md-table-align` | 連続する `|` 行を列幅に合わせて整列 |
| `md-table-insert-row` | 現在行の下に行を挿入 |
| `md-table-insert-column` | 現在セルの右に列を挿入 |
| `md-table-delete-row` | 現在行を削除 |
| `md-table-delete-column` | 現在列を削除 |
| `md-table-next-cell` | 次の `|` へ (TAB から呼ばれる) |

整列の手順:

1. 連続する表行の範囲を特定 (前後の空行/非 `|` 行まで)。
2. 各列の最大表示幅を `char_width()` で数える。
3. セパレータ行 (`|---|:--:|`) はアライメント記号を保持する。
4. 各行のセルに空格を詰めて `|` の位置を揃える。

### 5.9 タスクリスト

`md-task-toggle` は `[ ]` ↔ `[x]` を切り替える。
`[X]` (大文字) にも対応する。

### 5.10 引用

| コマンド | 動作 |
|----------|------|
| `md-quote-region` | リージョンの各行に `> ` を付加/除去 |
| `md-quote-toggle` | カレント行を引用にする/やめる |

### 5.11 フェンス

| コマンド | 動作 |
|----------|------|
| `md-insert-fence` | ` ```lang ` と ` ``` ` を対で挿入し、間にカーソルを置く |
| `md-goto-fence` | 対応する ` ``` ` へジャンプ |

`md-goto-fence` は既存の `getfence()` (`random.C_:920`) を
拡張する形でもよい (` ` ` を追加のフェンスとして認識させる)。
フェンスはネストしないため、単純な前方/後方探索で十分。

### 5.12 自動ペア

`$md-auto-pair` が TRUE のとき、`*`, `` ` ``, `[`, `_` などの自己挿入を
対で挿入する。

**実装箇所**: 自己挿入は `execute()` (`main.C_:588-`) の
`if (!(c & CK_FLAGS))` パスで処理される。このパスには既に
CMODE 用のフックが入っている:

```c
/* main.C_ の self-insert パス (既存) */
if (c == '}' && (curbp->b_mode & MDCMOD) != 0)
    status = insbrace(n, (Char)c);
else if (c == '#' && (curbp->b_mode & MDCMOD) != 0)
    status = inspound();
else {
    status = linsert(n, (Char)c);
}
```

`insbrace()` / `inspound()` (`cmode.C_`) が既存の実績。
Markdown モードも同じ場所に分岐を足す:

```c
else if ((curbp->b_mode & MDMDMOD) && md_auto_pair && md_pair_char(c))
    status = md_self_insert(n, (Char)c);
```

`md_pair_char()` は `*`, `` ` ``, `[`, `_` など、対にすべき文字を返す。
CMODE の `c_auto_close` (既定 1) と同じユーザー向け概念。

### 5.13 領域操作

| コマンド | 動作 |
|----------|------|
| `md-indent-region` | リージョン全体を +1 段 |
| `md-dedent-region` | リージョン全体を -1 段 |
| `md-format-region` | リージョン全体を構造解析して再インデント |

`md-format-region` は `py_indent_region()` (`python.C_:470-`) の
スタック方式を流用する。Markdown では「リスト階層スタック」と
「フェンス状態」を持つ。

### 5.14 保存時処理

| 機能 | 内容 |
|------|------|
| `$md-table-align-on-save` | 保存前にテーブルを整列 |
| 自動プレビュー | 保存後に `$md-preview-command` を呼ぶ |

`filesave()` (`file.C_:453`) にフックを入れる必要がある。
Phase 4。

---

## 6. 設定変数

`evar.H_` の `envars[]` に追加する。int/bool は
`get_int_hook` / `put_int_hook`、`get_bool_hook` / `put_bool_hook` を、
文字列は `get_str_hook` / `put_str_hook` を使う (既存の
`py-indent-width` 等と同じ)。

| 変数 | 型 | 既定 | 意味 |
|------|----|------|------|
| `$md-indent-width` | int | 2 | リスト/引用のインデント幅 |
| `$md-use-spaces` | bool | TRUE | スペースを使う (FALSE でタブ) |
| `$md-auto-list` | bool | TRUE | 改行時にリストマーカーを継続 |
| `$md-auto-pair` | bool | FALSE | 強調記号を自動ペア |
| `$md-conceal` | bool | FALSE | 行頭マーカーを非表示 |
| `$md-highlight` | bool | TRUE | 見出し/フェンスの色付け |
| `$md-list-bullets` | str | `- * +` | 巡回する箇条書き記号 |
| `$md-fence-lang` | str | (空) | フェンス既定言語 |
| `$md-preview-command` | str | `grip %f` | プレビューコマンド |
| `$md-table-align-on-save` | bool | FALSE | 保存時にテーブル整列 |
| `$md-heading-max` | int | 6 | 見出しレベルの上限 |

`$md-highlight` は `COLOR=0` のビルドでは反転表示にフォールバックする
(4.3 参照)。

---

## 7. キーバインド

`ebind.H_` の `keytab[]` に追加する。`execute()` は `keytab[]` を
**線形走査**する (`main.C_:530-539`)。したがって並び順は正しさには影響しない
(先にマッチした方が勝つ) が、レビューのしやすさから `Ckey` の昇順に
揃えるのが慣習。

**META キーの大文字化**: `getcmd()` は ESC (META) の直後の文字を
`islower(c)` なら大文字に正規化する (`input.c:288-289`)。
したがって `M-j` は `META|'J'` として登録する必要がある
(小文字のまま登録すると絶対にマッチしない)。

**CTLX キー**: `^X` の直後は `getcmd()` での大文字化は行われない
(`input.c:302-311`)。`^X l` と `^X L` は別キーとして共存できる
(既存の `toggle_line_numbers` / `set_line_number_width` がその例)。
ただし `execute()` には **大文字・小文字のフォールバック** がある
(`main.C_:541-566`): `^X <letter>` に完全一致する束縛が無ければ、
大小を反転したキーでもう一度探す。つまり大文字で登録しておけば
`^X g` でも `^X G` でも同じコマンドに届く。

| キー | コマンド | 備考 |
|------|----------|------|
| `C-M` | `mdinsert` | `newline` 経由で自動 |
| `Backspace` | `md_backspace` | `backdel` 経由で自動 |
| `TAB` | `md_tab` | `tab` 経由で自動 |
| `M-J` | `mdindent` | 現在 `pyindent` が確保。モード排他 |
| `M-I` | `md-italic` | 未使用 |
| `M-O` | `md-outline` | 未使用 |
| `M-Y` | `md-preview` | 未使用 |
| `^X G` | `md-insert-heading` | 未使用 (大文字 `G`) |
| `^X H` | `md-heading-up` | 未使用 (大文字 `H`) |
| `^X J` | `md-table-align` | 未使用 (大文字 `J`) |
| `^X Q` | `md-fold-toggle` | Phase 4 |
| `^X Y` | `md-conceal-toggle` | Phase 3 |

**配置の制約**:

- `META` グループはアルファベット順: `M-I` (0x49), `M-J` (0x4A),
  `M-O` (0x4F), `M-Y` (0x59)。
- `CTLX` グループは大文字順: `^X G` (0x47), `^X H` (0x48),
  `^X J` (0x4A) は `^X F` (0x46) と `^X K` (0x4B) の間、
  `^X Q` (0x51) は `^X P` (0x50) と `^X R` (0x52) の間、
  `^X Y` (0x59) は `^X X` (0x58) と `^X Z` (0x5A) の間。
- モード固有のキーは `#if MDMDMOD` 相当の preprocessor フラグ
  (`#define MDMODE 1` を `estruct.H_` に追加) で囲む。
  フラグが無いと `#if` が静かに無効化される (既知の落とし穴)。
- `M-J` は `pyindent` が既に使っているため、
  `#if PYMODE` と `#if MDMODE` の両方に同じキーを置けない。
  `M-J` はアクティブなモードの方を呼ぶ統一関数 (`smart_indent()`)
  に割り当て、内部で `MDMDMOD` / `MDPYMOD` / `MDCMOD` を判定するのがよい。

---

## 8. コマンド一覧 (`names[]` 登録)

`efunc.H_` の `names[]` に、以下を追加する (アルファベット順)。

| コマンド名 | 関数 |
|------------|------|
| `md-bold` | `md_bold` |
| `md-code` | `md_code` |
| `md-conceal-toggle` | `md_conceal_toggle` |
| `md-dedent-region` | `md_dedent_region` |
| `md-fold-toggle` | `md_fold_toggle` |
| `md-format-region` | `md_format_region` |
| `md-goto-fence` | `md_goto_fence` |
| `md-goto-heading` | `md_goto_heading` |
| `md-heading-cycle` | `md_heading_cycle` |
| `md-heading-down` | `md_heading_down` |
| `md-heading-up` | `md_heading_up` |
| `md-indent-region` | `md_indent_region` |
| `md-insert-fence` | `md_insert_fence` |
| `md-insert-heading` | `md_insert_heading` |
| `md-insert-link` | `md_insert_link` |
| `md-italic` | `md_italic` |
| `md-list-demote` | `md_list_demote` |
| `md-list-promote` | `md_list_promote` |
| `md-list-renumber` | `md_list_renumber` |
| `md-list-toggle` | `md_list_toggle` |
| `md-mode` | `md_mode` |
| `md-outline` | `md_outline` |
| `md-preview` | `md_preview` |
| `md-quote-region` | `md_quote_region` |
| `md-quote-toggle` | `md_quote_toggle` |
| `md-strike` | `md_strike` |
| `md-table-align` | `md_table_align` |
| `md-table-delete-column` | `md_table_delete_column` |
| `md-table-delete-row` | `md_table_delete_row` |
| `md-table-insert-column` | `md_table_insert_column` |
| `md-table-insert-row` | `md_table_insert_row` |
| `md-table-next-cell` | `md_table_next_cell` |
| `md-task-toggle` | `md_task_toggle` |

名前は `CSTR("md-...")` として `Cstr.data` に追加し、`make` で
`Css.c` を再生成する (`CSTR` を直接 `.H_` に書けば kpp が処理する)。

---

## 9. テスト計画

`tests/test_mdmode.c` を direct-include ハーネスで追加する
(`#include "../mdmode.C_"` + `#define maindef` + `#define static`、
既存の `test_python.c` / `test_cmode.c` を踏襲)。

| テスト項目 | 内容 |
|------------|------|
| 自動インデント | 見出し/リスト/番号リスト/タスクリスト/引用/フェンスの各遷移 |
| 番号リスト採番 | `1. ` → `2. ` → `3. ` |
| フェンス内でのマーカー無効化 | フェンス内の `#` を行頭マーカーと解釈しない |
| 再インデント | リスト階層、引用段数の修正 |
| デデント | 行頭マーカーの 1 段削除 |
| 見出しレベル | up/down/cycle の境界 (1-6) |
| 強調トグル | `**` の付与/除去、リージョン跨ぎ |
| テーブル整列 | 全角混在時の列位置 |
| アウトライン | 見出し抽出と行番号 |
| 色判定 | 対象外 (行レベルの色付けは未実装) |
| conceal | 行頭マーカーの非表示と列補正 |
| automode | `.md` / `.markdown` でビットが立つこと |

既存の `cd tests && make run` は 18 バイナリ 24,871 アサーションが
全パスしている (2026-11-17 時点)。新規テストはこれに破壊を与えない
こと、および新規バイナリを `tests/Makefile` の `all` と `run` に追加すること。

---

## 10. 実装フェーズ

| Phase | 内容 | 依存 |
|-------|------|------|
| 1 | モード骨格: ビット、automode、`md_mode`、`mdinsert`、`mdindent`、`md_backspace` | 既存の `newline`/`backdel` フック |
| 2 | 構造編集: 見出し、リスト、強調、リンク、テーブル、タスクリスト、引用、フェンス、アウトライン | Phase 1 |
| 3 | 表示: 行レベル色/反転 (`$md-highlight`)、行頭 conceal (`$md-conceal`)、モードライン `%h` | `COLOR=1` (任意) |
| 4 | 高度機能: 折りたたみ、自動ペア、保存時フック、外部プレビューの自動実行 | Phase 2/3 |

Phase 1 だけでも「Markdown を Structured Text として編集する」
段階の価値がある。表示系は Phase 3 以降で、`COLOR` の有効化判断と
セットで進める。

---

## 11. 制約・リスク・未決事項

### 11.1 技術的制約

1. **色は行単位** (`display.C_:935-1010`)。部分着色は `VIDEO` 構造体の
   文字ごとの属性を持つ改修が必要 (大規模)。
2. **conceal は表示列とカーソル列の整合**が最大の難所 (`updpos`,
   `display.C_:695-`)。行頭限定なら補正で吸収可能。
3. **折りたたみに行フィルタ機構がない**。`LINE` へのフラグ追加と
   `forwchar` / `backchar` / `isearch` / undo / リージョンへの影響が広い。
4. **自己挿入は `execute()` の直接 `linsert()` パス**。CMODE の
   `insbrace` / `inspound` が既に分岐の実績なので、Markdown も
   同じ場所に分岐を足す形で実現できる。
5. **`M-J` の競合**: `pyindent` が確保済み。統一関数に寄せるか、
   別キーを割り当てる必要がある。
6. **ガター列の競合**: 行番号ガター (`gbound`) が既に使っており、
   Markdown 用ガターは作らない。
7. **Char は 32bit** に拡幅済み (絵文字対応) のため、Markdown の
   UTF-8 本文は問題ない。

### 11.2 デザイン上の未決事項

- フェンス内の構文ハイライト (言語別キーワード色) は**対象外**。
  言語の構文を kemacs 側に持たせるのは非現実的。
- conceal するのか、色で表現するのか (4.3 / 4.4)。
- 折りたたみを実装するか、アウトラインで代替するか (4.5 / 4.6)。
- 表の編集を「TAB でセル移動」に限定するか、本格的な表エディタにするか。
- プレビューを外部任せにするか、`*scratch*` に HTML を出すにとどめるか。

### 11.3 既存モードとの共存

| モード | 共通点 | 相違点 |
|--------|--------|--------|
| CMODE | `cinsert` / `cindent` / `c_mode` の構成 | Markdown はブレースを構造に使わない。行頭マーカー中心 |
| PYMODE | `pyinsert` / `pyindent` / `py_backspace` の構成、`$py-*` 変数 | Markdown はインデントが構文でなく「整形」。マーカー継続が主 |

---

## 12. 参考: 既存モードとの対応表

| Markdown モード | 対応する既存実装 | 参照 |
|-----------------|------------------|------|
| `mdinsert` | `pyinsert` | `python.C_:231` |
| `mdindent` | `pyindent` | `python.C_:293` |
| `md_backspace` | `py_backspace` | `python.C_:379` |
| `md_format_region` | `py_indent_region` | `python.C_:470` |
| `md_mode` | `py_mode` | `python.C_:633` |
| `md_goto_fence` | `getfence` | `random.C_:920` |
| `mdinsert` のインデント | `cinsert` / `insert_indent` | `cmode.C_:850`, `python.C_:165` |
| `mdindent` の再インデント | `cindent` / `cindent_region` | `cmode.C_:1077, 1264` |
| 行の色付け | `updone` / `updall` の `v_rfcolor` | `display.C_:633-634, 682-683` |
| 見出しの反転 | `modeline` の `VFREQ` | `display.C_:1109` |
| 行番号ガター | `draw_gutter` / `line_number_of` | `display.C_:493, 432` |
| 変数 | `$py-*` / `$c-*` | `evar.H_:157-181` |
| 自動判定 | `pattbl` | `edef.H_:380` |
