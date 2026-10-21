# Contributing to MasseyCode

Thanks for taking the time to contribute. This document explains how to build
the project, what the code is expected to look like, and how to get a change
merged.

## Before you start

* For a bug, open an issue first if the fix is not obvious. Include the
  platform, how you started the app, the program you were running and a
  screenshot if the problem is visual.
* For a new feature, open an issue describing the problem it solves. MasseyCode
  is a teaching tool: features that add a mode, a pane or a key have to earn
  their place. "What does this help a student understand?" is the question a
  proposal has to answer.
* Security issues must not be reported in public. See [SECURITY.md](SECURITY.md).

## Development setup

Prerequisites:

* Rust stable, 1.92 or newer (`rustup` is the easiest way to get it);
* a C toolchain and, on Windows, the GNU target plus `windres` (see
  [docs/BUILDING.md](docs/BUILDING.md));
* on macOS, the Xcode command line tools for the bundle scripts.

Build and run:

```sh
cargo build -p massey-gui
cargo run -p massey-gui -- examples/05-loop.mc
```

Headless check, useful in CI and when you only changed the machine model:

```sh
cargo run -p massey-gui -- examples/01-addition.mc --dump=2
```

## Checks before sending a pull request

Run all three; CI runs the same commands.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`macOS`, `Windows` and the pure core are tested separately in CI. If you touch
platform code, build on the platform you changed.

## Code style

* `rustfmt` with the repository's `rustfmt.toml` is the source of truth. Do not
  reformat unrelated code in the same commit.
* Clippy warnings are errors for `massey-gui`; keep the tree clean rather than
  adding an allow.
* Comments explain *why*, not *what*. Named constants beat magic numbers,
  especially in layout code where the numbers are pixels.
* No `unwrap()` on user input or files. The machine model returns `Result` and
  diagnostics; use them.
* Keep `massey-core` free of UI dependencies. It is the part that can be tested
  headlessly and embedded elsewhere.

## Commits and pull requests

* One logical change per commit. Imperative subject line, 72 characters or
  fewer, for example `Fix MAR/MDR clipped border in short windows`.
* Reference the issue in the body when there is one.
* Fill in the pull request template, including "how was this tested".
* User-visible changes need a `CHANGELOG.md` entry under `## [Unreleased]`.
* Visual changes need a before/after screenshot.

## Changing the machine

The machine definition lives in one place and is shared by the assembler, the
disassembler, the VM and the debugger. Adding or changing an instruction means
touching all of them:

| Area | File |
| --- | --- |
| Encoding, mnemonics, registers | `crates/massey-core/src/isa.rs` |
| Assembly parsing and diagnostics | `crates/massey-core/src/asm.rs` |
| Disassembly | `crates/massey-core/src/disasm.rs` |
| Fetch/decode/execute, micro-ops, bus | `crates/massey-core/src/vm.rs` |
| Assembler tests | `crates/massey-core/tests/assembler.rs` |
| Example programs | `examples/` |

The machine reference in [docs/MACHINE.md](docs/MACHINE.md) has to stay in sync
with the code.

## Documentation

* `README.md` is the front page: what the project is and how to run it.
* `docs/MACHINE.md` describes the machine and the assembly language.
* `docs/ARCHITECTURE.md` describes how the code is organised.
* `docs/DESIGN.md` describes the window and the visual rules.
* `docs/BUILDING.md` describes building, packaging and releasing.

Documentation changes are welcome on their own. If you find a statement that no
longer matches the code, that is a bug.

## Licence

By contributing you agree that your contribution is licensed under the MIT
licence, as described in [LICENSE](LICENSE).

