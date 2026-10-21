# MasseyCode

**A visual debugger for the MASSEY teaching machine.** Step one register
transfer at a time and watch the fetch–decode–execute cycle happen: the control
unit lights, the bus carries a word, a register changes, the memory cell being
read turns blue.

![MasseyCode showing the ALU writing its result to R0](docs/screenshots/alu.png)

<p>
  <a href="https://github.com/LouveteauSean/MasseyCode/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/LouveteauSean/MasseyCode/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/LouveteauSean/MasseyCode/releases"><img alt="Release" src="https://img.shields.io/github/v/release/LouveteauSean/MasseyCode?include_prereleases"></a>
  <img alt="Rust 1.92+" src="https://img.shields.io/badge/rust-1.92%2B-000000?logo=rust">
  <img alt="Windows and macOS" src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS-0b0c0e">
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/license-MIT-6c9ef8"></a>
</p>

## Why

A debugger that shows registers and memory is a spreadsheet. A debugger that
shows *what moved* is a machine. MasseyCode runs the machine at the level of
the register transfers it is actually made of, so a student can answer "what
happened between `PC 02` and `PC 03`?" by looking at the window instead of
guessing from the numbers.

## Features

* **Single-line machine view** — the datapath (`CU`, `ALU`, `PC`, `IR`, `MAR`,
  `MDR`) and the 4×4 register file on one card, the address/data bus below it,
  and all 256 memory words in a 16×16 grid. The parts a transfer uses light up:
  no dots, no legends to decode.
* **Transfer-granular stepping** — one keypress, one `PC -> MAR` or
  `M[MAR] -> MDR`; step back just as easily. Instruction-level stepping sits on
  top as a second pair of buttons, so the diagram and the footer always agree.
* **Two editable views of one program** — machine code (four hex digits per
  line) and assembler (the notation of the course notes), sharing one image.
  Invalid lines turn red and block the view switch, so the two can never
  disagree.
* **Machine-aware highlighting** — the control unit stays lit for the whole
  instruction; the acting register is yellow; a value that changed is green;
  the bus and the addressed memory word light only for a real read or write,
  with the data direction named on the bus.
* **A page of help in the window** (`F1`) and **headless output** for scripts,
  screenshots and tests (`masseycode file.mc --dump=4`).
* Runs on **Windows** and **macOS** as a native window.

## Screenshots

| Fetching an instruction | The ALU writing a result |
| --- | --- |
| ![Fetch](docs/screenshots/fetch.png) | ![ALU](docs/screenshots/alu.png) |

| Writing to memory | The assembler view |
| --- | --- |
| ![Store](docs/screenshots/write.png) | ![Assembler](docs/screenshots/assembler.png) |

The help pane is `docs/screenshots/help.png`; the minimum window
(900×600) is `docs/screenshots/minimum-window.png`.

## Download

Release builds are attached to
[GitHub Releases](https://github.com/LouveteauSean/MasseyCode/releases):

| Platform | File |
| --- | --- |
| Windows 10/11 (x64) | `MasseyCode-win64.zip` — unzip anywhere, run `masseycode.exe` |
| macOS 11+ (Apple silicon) | `MasseyCode-macos.zip` — unzip, drag `MasseyCode.app` into Applications |

The macOS build is ad-hoc signed, not notarised: the first launch needs
*right-click → Open*, or `xattr -dr com.apple.quarantine MasseyCode.app`.

## Build from source

Requires [Rust](https://rustup.rs) 1.92 or newer. Windows also needs the GNU
toolchain (the icon and the no-console setting are applied by `windres`); see
[Building](docs/BUILDING.md) for the full list.

```sh
git clone https://github.com/LouveteauSean/MasseyCode.git
cd MasseyCode
cargo run -p massey-gui                          # opens the first example
cargo run -p massey-gui -- examples/05-loop.mc   # or a program of your own
```

## Using it

The window opens on a worked example. Step with `F11` (one transfer) or `F10`
(one instruction); `F5` runs transfer by transfer and `F6` runs instruction by
instruction. `b` steps back, `Ctrl+Shift+R` resets, `Ctrl+M` switches between
machine code and assembler, `Ctrl+O` opens a file, `F1` explains the machine.
On macOS the `Ctrl` shortcuts also accept `Cmd`.

| Key | Action |
| --- | --- |
| `F11` | step one register transfer |
| `F10` | step one whole instruction |
| `F5` / `Ctrl+R` | run, one transfer per tick |
| `F6` | run, one instruction per tick |
| `b` / `Backspace` | step back one transfer |
| `Ctrl+M` | switch Machine Code / Assembler |
| `Ctrl+Shift+R` | reset the machine |
| `Ctrl+O` | open a file |
| `j` `k` `h` `l` / arrows | scroll memory |
| `Home` / `End` | memory to the top / bottom |
| `F1` | help |
| `q` / `Esc` | quit |

The same commands are on the toolbar, and the footer always names the transfer
being looked at, for example
`[exec 3/3] MDR -> M[MAR] · 03 400A STORE R0, [0A]`.

## The machine

The MASSEY machine is a 16-bit teaching computer: 256 words of memory, sixteen
registers `R0`–`RF`, an 8-bit program counter, and thirteen instructions. Every
instruction is one word — a 4-bit opcode and three operand digits — and every
instruction begins with the same four-transfer fetch:

```text
PC -> MAR · M[MAR] -> MDR · MDR -> IR · PC + 1 -> PC
```

The complete instruction set, encodings, assembly syntax and worked examples
are in **[docs/MACHINE.md](docs/MACHINE.md)**.

## Documentation

| Document | Contents |
| --- | --- |
| [docs/MACHINE.md](docs/MACHINE.md) | the machine, the instruction set, the two source notations |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | how the code is organised, the stepping model, testing |
| [docs/DESIGN.md](docs/DESIGN.md) | the window, the visual grammar, layout rules |
| [docs/BUILDING.md](docs/BUILDING.md) | building, packaging, releasing |
| [CONTRIBUTING.md](CONTRIBUTING.md) | how to build, test and send a change |
| [CHANGELOG.md](CHANGELOG.md) | what changed in each release |

## Repository layout

```text
crates/massey-core   machine model, assembler, disassembler, VM (no UI, no deps)
crates/massey-gui    the GPUI window, machine diagram, help, headless frame
examples             nine worked programs, as listing (.asm) and image (.mc)
assets               application icon and platform metadata
docs                 machine reference, architecture, design, build notes
scripts              icon generation, screenshot helper, macOS bundle, packaging
```

## Contributing

Bug reports, example programs and teaching notes are all welcome. Read
[CONTRIBUTING.md](CONTRIBUTING.md) first; run `cargo fmt`, `cargo clippy` and
`cargo test --workspace` before opening a pull request. Please report security
issues privately as described in [SECURITY.md](SECURITY.md).

## Licence

[MIT](LICENSE) © 2026 LouveteauSean
