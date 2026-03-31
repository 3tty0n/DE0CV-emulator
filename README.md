# DE0-CV FPGA Emulator

A terminal-based emulator for the [DE0-CV](https://www.terasic.com.tw/cgi-bin/page/archive.pl?No=921) FPGA development board.
It parses and simulates Verilog HDL files, rendering LEDs and 7-segment displays in a TUI.

Works on macOS, Linux, and Windows — no FPGA hardware required.

> **[日本語版はこちら / Japanese version](./README.ja.md)**

---

## Installation

### Option 1: Download pre-built binary (recommended)

Download the latest release for your platform from the [Releases page](https://github.com/3tty0n/DE0CV-simulator/releases).

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | `de0cv_emulator-macos-aarch64.tar.gz` |
| macOS (Intel) | `de0cv_emulator-macos-x86_64.tar.gz` |
| Linux (x86_64) | `de0cv_emulator-linux-x86_64.tar.gz` |
| Windows (x86_64) | `de0cv_emulator-windows-x86_64.zip` |

Example: when you use macOS with Apple Silicon:

```sh
tar xzf de0cv_emulator-macos-aarch64.tar.gz
./de0cv_emulator assignment/day1/seg7dec.v
```

#### macOS: "Apple could not verify" warning

macOS blocks unsigned binaries by default. To fix this, run:

```sh
xattr -d com.apple.quarantine ./de0cv_emulator
```

Or: **System Settings > Privacy & Security > scroll down > "Allow Anyway"**.

### Option 2: Build from source

Requires [Rust](https://rustup.rs/) 1.85+.

```sh
git clone git@github.com:3tty0n/DE0CV-simulator.git
cd DE0CV-simulator
cargo build --release
```

---

## Usage

```sh
cargo run --release -- <file.v> [file.v ...] [options]
```

Pass one or more `.v` files. The first module found becomes the top module.
Dependencies (e.g., `SEG7DEC_U`) are auto-discovered from the same and sibling directories.

### Options

| Option | Description |
|--------|-------------|
| `--top <name>` | Specify the top-level module name |
| `--speed <N>` | Clock cycles per frame (default: 1000) |

### Examples

```sh
# 7-segment decoder: toggle SW[0]-SW[3] with F1-F4
cargo run --release -- assignment/day1/seg7dec.v

# 1-second counter (0-9)
cargo run --release -- assignment/day1/sec10.v

# 60-second counter (auto-discovers seg7dec_u.v)
cargo run --release -- assignment/day3/sec60_for_ModelSim.v

# Dice counter
cargo run --release -- assignment/day2/Dice_Counter.v

# Rock-Paper-Scissors / Dice trial
cargo run --release -- assignment/day3/RPS.v

# Shift register
cargo run --release -- assignment/day1/SR2.v

# Gate-level dice counter
cargo run --release -- assignment/day2/Dice_Trial_Gate.v
```

---

## Controls

| Key | Action |
|-----|--------|
| `1` `2` `3` `4` | Press KEY[0]--KEY[3] (push buttons) |
| `F1`--`F10` | Toggle SW[0]--SW[9] (DIP switches) |
| `r` | Reset (pulse RST for one frame) |
| `F10` | Toggle SW9 / RST (same as real DE0-CV) |
| `Space` | Pause / resume simulation |
| `q` / `Esc` | Quit |

---

## Emulated Hardware

| Component | Description |
|-----------|-------------|
| **LEDR[9:0]** | 10 red LEDs |
| **HEX0--HEX5** | 6 seven-segment displays (active-low, red) |
| **KEY[3:0]** | 4 push buttons (keys `1`--`4`) |
| **SW[9:0]** | 10 DIP switches (`F1`--`F10`) |
| **CLK** | 50 MHz clock (speed adjustable with `--speed`) |
| **RST** | Reset signal (`r` key or `F10` / SW9) |

---

## Supported Verilog Subset

The emulator supports the Verilog constructs commonly used in introductory FPGA labs.

### Modules

```verilog
module name(port_list);   // ANSI and non-ANSI port styles
endmodule
```

### Declarations

- `input` / `output` / `reg` / `wire` with bit ranges
- Initial values: `reg [3:0] cnt = 0;`

### Always blocks

- `always @(posedge CLK)` -- sequential logic
- `always @(posedge RST or posedge CLK)` -- multiple edges
- `always @*` -- combinational logic

### Assignments

- Blocking: `cnt = cnt + 1;`
- Non-blocking: `cnt <= cnt + 1;`
- Continuous: `assign wire_name = expr;`

### Control flow

- `if` / `else if` / `else`
- `case` / `endcase` / `default`

### Operators

`+` `-` `==` `!=` `<` `>` `<=` `>=` `&` `|` `^` `~` `!` `&&` `||` `? :`

### Number literals

```verilog
0                // decimal
3'd5             // 3-bit decimal
4'hF             // 4-bit hex
7'b1000000       // 7-bit binary
26'd49_999_999   // underscores allowed
```

### Other

- Bit select: `signal[n]`
- Module instantiation (positional): `SEG7DEC_U seg7du(sec, HEX0);`
- Testbench constructs (`initial`, `always #delay`) are automatically skipped

---

## Architecture

```
src/
├── main.rs              # TUI app, event loop, CLI
├── board.rs             # DE0-CV board state (LEDs, 7-seg, switches)
├── display.rs           # Ratatui widgets for 7-segment and LED rendering
└── verilog/
    ├── lexer.rs         # Tokenizer
    ├── parser.rs        # Recursive descent parser -> AST
    ├── ast.rs           # AST type definitions
    └── simulator.rs     # Compiles AST to indexed IR, evaluates clock cycles
```

The simulator compiles Verilog modules into an indexed intermediate representation where all signal names are resolved to array indices at setup time. This avoids HashMap lookups during simulation, achieving ~30M clock cycles/sec in release mode.

---

## License

MIT
