# DE0-CV FPGA Emulator

[DE0-CV](https://www.terasic.com.tw/cgi-bin/page/archive.pl?No=921) FPGAボードのターミナルエミュレータです。
自分で書いた Verilog HDL ファイルを読み込み、LED や 7セグメントディスプレイの動作をターミナル上で確認できます。

実機がなくても、手元のPC（macOS / Linux / Windows）で動作確認ができます。

> **[English version](./README.md)**

---

## インストール方法

### 方法1: ビルド済みバイナリをダウンロード（おすすめ）

[Releases ページ](https://github.com/3tty0n/DE0CV-simulator/releases) から、自分の OS に合ったファイルをダウンロードしてください。

| OS | ファイル |
|----|---------|
| macOS (Apple Silicon) | `de0cv_emulator-macos-aarch64.tar.gz` |
| macOS (Intel) | `de0cv_emulator-macos-x86_64.tar.gz` |
| Linux | `de0cv_emulator-linux-x86_64.tar.gz` |
| Windows | `de0cv_emulator-windows-x86_64.zip` |

Apple Silicon を使用した macOS ラップトップを使用している場合:

ダウンロード後、展開して実行します。

```sh
tar xzf de0cv_emulator-macos-aarch64.tar.gz
./de0cv_emulator assignment/day1/seg7dec.v
```

#### macOS で「開発元を検証できません」と表示される場合

macOS では、署名のないバイナリがブロックされることがあります。
以下のいずれかの方法で解除してください。

**方法A: ターミナルで実行（簡単）**

```sh
xattr -d com.apple.quarantine ./de0cv_emulator
```

**方法B: システム設定から許可**

1. エミュレータを一度実行する（エラーが出る）
2. **「システム設定」>「プライバシーとセキュリティ」** を開く
3. 下にスクロールして **「このまま許可」** をクリック

---

### 方法2: ソースからビルド

#### Rust のインストール

まだ Rust をインストールしていない場合は、以下のコマンドを実行してください。

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

インストール後、ターミナルを再起動してください。

#### ビルド

このリポジトリをクローンして、ビルドします。

```sh
git clone git@github.com:3tty0n/DE0CV-simulator.git
cd DE0CV-simulator
cargo build --release
```

---

## 使い方

```sh
cargo run --release -- <ファイル.v> [ファイル.v ...] [オプション]
```

引数に `.v` ファイルを渡すと、最初に見つかったモジュールがトップモジュールになります。
依存モジュール（例: `SEG7DEC_U`）は同じディレクトリから自動的に検索されるので、多くの場合1ファイルだけで動きます。

### オプション

| オプション | 説明 |
|-----------|------|
| `--top <モジュール名>` | トップモジュールを明示的に指定する |
| `--speed <N>` | 1フレームあたりのクロックサイクル数（デフォルト: 1000） |

---

## 実行例

### Day1

```sh
# 7セグメントデコーダ（SW[3:0] で 0〜F を表示）
cargo run --release -- assignment/day1/seg7dec.v

# 1秒カウンタ（0〜9 を繰り返す）
cargo run --release -- assignment/day1/sec10.v

# シフトレジスタ
cargo run --release -- assignment/day1/SR2.v

# サイコロカウンタ（0〜5 を繰り返す）
cargo run --release -- assignment/day1/Dice_Counter.v
```

### Day2

```sh
# サイコロカウンタ（7セグ表示つき）
cargo run --release -- assignment/day2/Dice_Trial_s7d.v

# ゲートレベルのサイコロカウンタ
cargo run --release -- assignment/day2/Dice_Trial_Gate.v
```

### Day3

```sh
# じゃんけん / サイコロ（1〜6 を高速で回転、KEY[0] で停止）
cargo run --release -- assignment/day3/RPS.v

# 60秒カウンタ（HEX1:HEX0 に 00〜59 を表示）
cargo run --release -- assignment/day3/sec60_for_ModelSim.v
```

---

## キー操作

エミュレータ起動後、以下のキーで操作できます。

| キー | 操作 |
|------|------|
| `1` `2` `3` `4` | KEY[0]〜KEY[3] を押す（プッシュボタン） |
| `F1`〜`F10` | SW[0]〜SW[9] を切り替える（DIPスイッチ） |
| `r` | リセット（RST を1フレーム送信） |
| `F10` | SW9 / RST のトグル（実機と同じ配置） |
| `Space` | シミュレーションの一時停止 / 再開 |
| `q` / `Esc` | 終了 |

### 操作のポイント

- **seg7dec**: `F1`〜`F4`（SW[0]〜SW[3]）を切り替えると、HEX0 の表示が変わります
- **Dice_Counter**: 高速でカウントしているので、`F10`（RST/SW9）で停止・再開します
- **SR2**: `F1`（SW[0]）でデータ入力、`1`（KEY[0]）でクロックを送ります
- **sec10 / sec60**: 自動でカウントアップします。`--speed` を大きくすると速くなります

---

## エミュレートされるハードウェア

ターミナル上に以下の DE0-CV ボードの部品が表示されます。

| 部品 | 説明 |
|------|------|
| **LEDR[9:0]** | 10個の赤色LED |
| **HEX0〜HEX5** | 6個の7セグメントディスプレイ（赤色表示） |
| **KEY[3:0]** | 4個のプッシュボタン（キー `1`〜`4` で操作） |
| **SW[9:0]** | 10個のDIPスイッチ（`F1`〜`F10` で切り替え） |
| **CLK** | 50MHz クロック（シミュレーション速度は `--speed` で調整） |
| **RST** | リセット信号（`r` キーまたは `F10` / SW9） |

---

## 対応している Verilog の書き方

このエミュレータは、実験で使う基本的な Verilog 構文に対応しています。

### モジュール

```verilog
module モジュール名(ポート一覧);
  // ANSI スタイル・非ANSIスタイルの両方に対応
endmodule
```

### 宣言

- `input` / `output` / `reg` / `wire`（ビット幅指定可）
- `reg [3:0] cnt = 0;`（初期値つき）

### always ブロック

- `always @(posedge CLK)` — クロック立ち上がりで動作
- `always @(posedge RST or posedge CLK)` — 複数エッジ
- `always @*` — 組み合わせ回路

### 代入

- ブロッキング代入: `cnt = cnt + 1;`
- ノンブロッキング代入: `cnt <= cnt + 1;`
- 連続代入: `assign wire名 = 式;`

### 制御構文

- `if` / `else if` / `else`
- `case` / `endcase` / `default`

### 演算子

`+` `-` `==` `!=` `<` `>` `<=` `>=` `&` `|` `^` `~` `!` `&&` `||` `? :`

### 数値リテラル

```verilog
0           // 10進
3'd5        // 3ビット幅の10進数 5
4'hF        // 4ビット幅の16進数 F
7'b1000000  // 7ビット幅の2進数
26'd49_999_999  // アンダースコア区切り可
```

### その他

- ビット選択: `signal[n]`
- モジュールインスタンス化（位置指定）: `SEG7DEC_U seg7du(sec, HEX0);`
- テストベンチ（`initial` / `always #delay`）は自動でスキップされます
