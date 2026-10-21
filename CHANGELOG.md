# Changelog

All notable changes to MasseyCode are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-21

First public release.

### Added

* Single-line machine view: a datapath schematic (`CU`, `ALU`, `PC`, `IR`,
  `MAR`, `MDR`) and the 4x4 register file on one card, with an address/data bus
  to the 256-word memory block below.
* Transfer-granular stepping: every register transfer of the fetch and execute
  cycles can be stepped forward and back, with instruction-level stepping on
  top of it.
* Two editable views of the same program image: Machine Code (one 16-bit word
  per line) and Assembler (the notation used by the MASSEY course notes), with
  live diagnostics and a shared undo history.
* Machine-aware highlighting: the control unit stays lit for the whole
  instruction, the acting register is drawn in yellow, changed values flash
  green, and the bus and addressed memory word light only for a real memory
  read or write.
* Memory grid showing all 256 words, with PC, program and changed-word marks,
  and keyboard scrolling.
* Help pane explaining the machine, the two views and the key map.
* Headless mode: `masseycode program.mc --dump=N` prints one text frame and
  exits, which makes the debugger scriptable and testable.
* Windows build with an embedded icon and no console window; macOS application
  bundle output by `scripts/make-macos-app.sh`.
* GitHub Actions builds for Windows and macOS, with a release workflow that
  attaches both packages to a tag.

[Unreleased]: https://github.com/LouveteauSean/MasseyCode/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/LouveteauSean/MasseyCode/releases/tag/v0.1.0
