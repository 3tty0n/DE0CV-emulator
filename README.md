# DE0-CV FPGA Emulator

A terminal-based emulator for the [DE0-CV](https://www.terasic.com.tw/cgi-bin/page/archive.pl?No=921) FPGA development board, written in Rust. It parses and simulates Verilog HDL files, rendering LEDs and 7-segment displays in a TUI.

## Requirements

- Rust toolchain (1.85+)

## Build

```sh
cargo build --release
```

## Usage

```sh
cargo run --release -- <file.v> [file.v ...] [--top <module>] [--speed <cycles_per_frame>]
```

Pass one or more `.v` files. The first module found becomes the top module (override with `--top`). If a module instantiates another (e.g., `SEG7DEC_U`), pass the dependency file as well.

### Options

| Option | Description |
|--------|-------------|
| `--top <name>` | Specify the top-level module name |
| `--speed <N>` | Clock cycles per frame (default: 833333 for real-time at 50MHz/60fps) |

### Examples

```sh
# 7-segment decoder: toggle SW[0]-SW[3] with F1-F4
cargo run --release -- src_ans/day1/seg7dec.v

# 1-second counter (0-9)
cargo run --release -- src_ans/day1/sec10.v

# 60-second counter (requires seg7dec_u dependency)
cargo run --release -- src_ans/day3/sec60_for_ModelSim.v src_ans/day3/seg7dec_u.v

# Dice counter
cargo run --release -- src_ans/day2/Dice_Counter.v

# Rock-Paper-Scissors / Dice trial
cargo run --release -- src_ans/day3/RPS.v

# Shift register
cargo run --release -- src_ans/day1/SR2.v

# Gate-level dice counter
cargo run --release -- src_ans/day2/Dice_Trial_Gate.v
```

## Controls

| Key | Action |
|-----|--------|
| `1` `2` `3` `4` | Press KEY[0]–KEY[3] (push buttons) |
| `F1`–`F10` | Toggle SW[0]–SW[9] (DIP switches) |
| `r` | Reset |
| `Space` | Pause / resume simulation |
| `q` / `Esc` | Quit |

## Supported Verilog Subset

The emulator supports the Verilog constructs commonly used in introductory FPGA labs:

- `module` / `endmodule` (ANSI and non-ANSI port styles)
- `input`, `output`, `reg`, `wire` declarations with bit ranges
- `always @(posedge CLK)`, `always @(posedge RST or posedge CLK)`, `always @*`
- `if` / `else if` / `else`, `case` / `endcase` / `default`
- Blocking (`=`) and non-blocking (`<=`) assignments
- `assign` (continuous assignment, delays ignored)
- Operators: `+`, `-`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&`, `|`, `^`, `~`, `!`, `&&`, `||`, `? :`
- Number literals: `0`, `3'd5`, `4'hF`, `7'b1000000`, `26'd49_999_999`
- Bit select: `signal[n]`
- Module instantiation with positional ports

## Emulated Hardware

- **LEDR[9:0]** — 10 red LEDs
- **HEX0–HEX5** — 6 seven-segment displays (active-low, rendered in red)
- **KEY[3:0]** — 4 push buttons (active for one frame per press)
- **SW[9:0]** — 10 DIP switches (toggle on/off)
- **CLK** — 50 MHz clock (simulated, configurable speed)
- **RST** — System reset
