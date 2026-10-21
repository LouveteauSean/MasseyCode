# Architecture

MasseyCode is a Rust workspace with two crates and a thin platform layer. The
machine model is deliberately independent of the window, so the same code that
runs the debugger can be tested headlessly, embedded in another tool, or
replaced without touching the UI.

## Workspace

```text
masseycode/
├── Cargo.toml                 workspace, shared metadata, profiles
├── crates/
│   ├── massey-core/           machine, assembler, disassembler, VM
│   │   ├── src/
│   │   │   ├── lib.rs         public API, Document, SourceMode
│   │   │   ├── isa.rs         encodings, opcodes, registers
│   │   │   ├── asm.rs         .mc assembler and diagnostics
│   │   │   ├── disasm.rs      instruction to text
│   │   │   ├── vm.rs          Machine, micro-ops, bus, faults
│   │   │   ├── examples.rs    bundled programs
│   │   │   ├── complete.rs    completion for editor-like front ends
│   │   │   ├── highlight.rs   tokenizer for source colouring
│   │   │   └── session.rs     run/pause/log session for embedders
│   │   └── tests/             assembler and example-program regressions
│   └── massey-gui/            GPUI window
│       ├── src/
│       │   ├── main.rs        CLI, window setup, platform glue
│       │   ├── app.rs         layout, panes, toolbar, keys, memory grid
│       │   ├── cpu_pane.rs    datapath schematic and highlight logic
│       │   ├── debugger.rs    stepping model, history, per-transfer records
│       │   ├── frame.rs       headless text frame (--dump)
│       │   ├── help.rs        help pane
│       │   ├── asm_highlight.rs  syntax colouring for the source panes
│       │   ├── pdf_asm.rs     the Assembler view's notation
│       │   └── theme.rs       colours, fonts, spacing
│       └── build.rs           Windows icon resource and GUI subsystem
├── assets/                    icon source, .rc, Info.plist
├── docs/                      this documentation
├── examples/                  example programs (.asm listing + .mc image)
└── scripts/                   icon generation, macOS bundle, packaging
```

Dependencies point one way: `massey-gui` uses `massey-core`, never the reverse.
`massey-core` has no dependencies at all, and nothing in it mentions a window,
a colour or a font.

## massey-core

| Module | Responsibility |
| --- | --- |
| `isa` | The single definition of the instruction set: opcode nibbles, mnemonics, encoding patterns, register names, word formatting. |
| `asm` | Assembles the listing notation into a program image; produces per-line `SourceLine`s, labels and `Diagnostic`s. |
| `disasm` | Turns a word into its canonical assembler spelling, and reports which register a word reads and writes. |
| `vm` | `Machine`: fetch/decode/execute as `MicroOp`s, `Snapshot` for stepping back, `BusTransfer`, `AluRecord`, faults. |
| `examples` | The bundled example programs, embedded with `include_str!`. |
| `complete` | Completion candidates for a front end that wants an editor. |
| `highlight` | A small tokenizer (`Token`, `TokenKind`) for source colouring. |
| `session` | A run/pause/log facade for embedders that do not want to own a clock. |
| `lib` | Re-exports and `Document`, which keeps the machine-code image and the assembly text in sync. |

The important invariant is that **one word is one instruction and one memory
location**. Everything else follows from it: the assembler is a list of lines
to a list of words, the disassembler is the inverse, and the source map is a
line-to-address table.

`Machine::micro_step()` is the core of the debugger. It performs exactly one
register transfer and returns a `MicroOp` describing what moved:

```rust
pub struct MicroOp {
    pub text: String,                       // "M[MAR] -> MDR"
    pub phase: Phase,                       // fetch, execute, halt
    pub ends_instruction: bool,
    pub halts: bool,
    pub faulted: bool,
    pub bus: Option<BusTransfer>,           // real bus traffic, if any
    pub register_writes: Vec<(u8, u16)>,    // registers this transfer wrote
    pub address: u8,                        // instruction address
}
```

Everything the window highlights is derived from these fields. The view never
parses `text` to decide what happened.

`Machine::transfer_plan(word)` returns the same information ahead of time, as
readable strings, which is what the tests use to check that every possible
instruction marks at least one part of the diagram.

## massey-gui

| Module | Responsibility |
| --- | --- |
| `main` | Argument parsing (`--asm`, `--dump=N`, `--help`), window creation, Windows console/icon glue. |
| `app` | The whole window: title bar, toolbar, source pane, CPU card and memory block, footer, keyboard map, animations. |
| `cpu_pane` | Draws the datapath and register file, decides what is lit from the current `MicroOp`. |
| `debugger` | Owns the `Machine`, the history of snapshots, the per-transfer text and micro-op records, scrolling state and the run/pause loop. |
| `frame` | Renders one plain-text frame for `--dump`; no GPUI, no window. |
| `help` | The help pane, including the key map. |
| `asm_highlight` | Token colours for the editor text. |
| `pdf_asm` | Parses and prints the restricted notation the Assembler view accepts. |
| `theme` | Colours, font families, sizes and spacing constants. |

### Data flow

```text
             ┌──────────────────────────────┐
 .mc file ──▶│ image: Vec<u16>              │
             └──────────────┬───────────────┘
                            │ load_image()
                            ▼
                    ┌───────────────┐
                    │ Machine       │  memory, registers, PC/IR/MAR/MDR
                    └───────┬───────┘
                            │ micro_step()
                            ▼
                    ┌───────────────┐
                    │ MicroOp       │  what moved, where, across which bus
                    └───────┬───────┘
                            │ recorded by Debugger
                            ▼
        ┌───────────────────┴────────────────────┐
        │                                        │
        ▼                                        ▼
  cpu_pane::datapath()                    app::memory()
  lit CU/ALU/PC/IR/MAR/MDR                highlighted cell
  register file, bus row                  PC marker, changed mark
```

The Assembler view is the reverse direction: its text is parsed line by line to
an image, and the image is what the machine runs. Switching views disassembles
the image, so the two spellings always agree.

### Stepping model

`Debugger` is the only owner of the machine. It exposes four operations:

| Operation | Effect |
| --- | --- |
| `step_transfer` | one `micro_step`, pushing the previous snapshot and the `MicroOp` on the history |
| `step_instruction` | repeated `micro_step` until the instruction boundary |
| `back` | pop the last snapshot and the last transfer record |
| `run_slice(n)` | run `n` transfers without touching the UI; used by the timer |

The window's timer calls `run_slice` in a loop so the UI stays responsive
between transfers, and the animation is what makes a transfer visible: the
acting register and the changed value are painted from the *last* `MicroOp`,
not from a guess about the next one.

History is bounded (20,000 snapshots, trimmed oldest-first, reported in the
footer) so that running a long program cannot exhaust memory.

### Rendering

Every frame is a GPUI element tree built from the current state; there is no
retained widget tree to keep in sync. The window is two independent halves:

* the **workspace** — custom title bar, toolbar, source pane (left);
* the **machine** — `CPU` card (datapath + register file), bus row, and the
  `MEMORY` block (right).

Sizes are computed from the window rectangle rather than hard-coded per
resolution. The content height is the window height minus the chrome and the
card padding and border, and the rows inside a card use `flex_1` with a zero
minimum, so a card can never be taller than the space it was given. That is
what keeps `MAR`/`MDR` and the last register row from being clipped at the
minimum window size.

### Platform integration

`crates/massey-gui/build.rs` only does anything for `windows` + `gnu`:

* compiles `assets/masseycode.rc` with `windres`, which embeds the application
  icon in the PE file;
* passes `-mwindows`, so the binary is a GUI program and double-clicking it
  does not open a console.

`main.rs` carries the matching runtime code, all behind `#[cfg(windows)]`:
`WM_SETICON` after the window exists (the custom GPUI window class does not
inherit the executable's icon) and `AttachConsole` so `--dump` can print to the
terminal that started it.

On macOS the icon is not a PE resource; `scripts/make-macos-app.sh` builds the
bundle, converts the PNG to `.icns` and signs it. Fonts and the command key are
selected at compile time in `theme.rs` and `app.rs`.

## Testing

| Layer | Where | What it pins down |
| --- | --- | --- |
| Instruction encoding | `isa.rs`, `asm.rs`, `pdf_asm.rs` tests | every form assembles to the documented word |
| Machine | `vm.rs` tests | fetch sequence, execute transfers, bus traffic, halt, faults, snapshots |
| Examples | `crates/massey-core/tests/examples.rs` | all nine example programs assemble and produce their documented results |
| Debugger | `debugger.rs` tests | transfer/instruction stepping, back, run to halt, scrolling, history trimming |
| Diagram | `cpu_pane.rs` tests | every `transfer_plan` line marks something; a not-taken jump does not light PC; CU is lit for the whole instruction |
| Text frame | `frame.rs` tests | the headless frame has the right marks and never exceeds its width |
| Window | — | checked by hand at 900×600, 1180×760 and 2560×1528 |

The tests that matter most are the ones that tie the diagram to the machine:
the diagram is allowed to be pretty, but it is not allowed to disagree with
`MicroOp`.

## Conventions

* `massey-core` is the place for anything the machine knows. If a rule can be
  tested without a window, it belongs there.
* The UI reads state; it does not recompute it. If the window needs a new fact,
  add it to `MicroOp`/`Snapshot` rather than parsing transfer text.
* Layout numbers are named constants in `theme.rs` or in `cpu_pane.rs`, never
  literals spread through the drawing code.
* No `unsafe` in `massey-core`; the only `unsafe` in the workspace is the
  Windows FFI in `main.rs`.
* Adding an instruction touches `isa.rs`, the assembler, the disassembler, the
  VM, the docs and the tests. [CONTRIBUTING.md](../CONTRIBUTING.md) lists the
  files.

## Where to start reading

1. `crates/massey-core/src/isa.rs` — the machine, in one page.
2. `crates/massey-core/src/vm.rs`, `Machine::micro_step` — the cycle.
3. `crates/massey-gui/src/debugger.rs` — the stepping model.
4. `crates/massey-gui/src/cpu_pane.rs` — how the diagram is drawn from state.
5. `crates/massey-gui/src/app.rs` — the window around it.

See also: [MACHINE.md](MACHINE.md) for the machine reference,
[DESIGN.md](DESIGN.md) for the visual rules, and
[BUILDING.md](BUILDING.md) for the build.
