# Security policy

## Supported versions

Only the latest release of MasseyCode receives fixes.

| Version | Supported |
| --- | --- |
| 0.1.x | yes |
| < 0.1 | no |

## Reporting a vulnerability

Please do **not** open a public issue for a security problem. Report it
privately through GitHub's private vulnerability reporting:

<https://github.com/LouveteauSean/MasseyCode/security/advisories/new>

Include, as far as it applies:

* the version and how you obtained it (release asset, or a commit hash);
* the platform;
* the input that triggers the problem (a `.mc` or `.asm` file is usually
  enough);
* what happens, and what you expected instead.

You can expect an acknowledgement within a week. If the report is accepted we
will agree on a disclosure date with you before publishing a fix.

## Scope

MasseyCode is a desktop application that opens local programs. The following are
in scope:

* parsing of `.mc`, `.asm` and `.pdf`-notation source files (a malformed file
  must never panic or loop forever);
* the assembler, disassembler and virtual machine in `crates/massey-core`;
* escaping or path handling when a file is opened from the window;
* the Windows and macOS packaging scripts.

Out of scope:

* vulnerabilities in GPUI or in the graphics driver;
* denial of service by intentionally running a program that loops forever
  (the debugger has a step limit by design);
* issues that require an attacker to already control the machine the
  application runs on.

## Hardening notes

Programs are data, not code: MasseyCode interprets them and never executes them
natively. The machine model has no unsafe code; the only `unsafe` blocks in the
workspace are the Windows FFI calls used to attach the console in `--dump`
mode and to set the window icon.
