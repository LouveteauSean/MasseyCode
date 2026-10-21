# Building MasseyCode

MasseyCode is a normal Cargo workspace. `massey-core` has no dependencies;
`massey-gui` is the GPUI window and owns the platform-specific bits.

## Requirements

| Requirement | Needed for |
| --- | --- |
| Rust 1.92 or newer (`rustup`) | everything |
| Windows: the GNU toolchain `stable-x86_64-pc-windows-gnu` plus MinGW `windres` and `strip` | Windows builds with the embedded icon and no console window |
| Windows: `fxc.exe` from the Windows SDK | **release** builds on Windows (GPUI compiles its shaders in release only) |
| macOS: Xcode command line tools | macOS builds (Metal shaders, `.icns`, `codesign`) |
| Python 3 + Pillow | only to regenerate the application icon |

### Windows

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

The GNU toolchain needs a linker and `windres`. The usual installation is
[MSYS2](https://www.msys2.org) with the `mingw-w64-ucrt-x86_64-toolchain` (or
`mingw-w64-x86_64-toolchain`) package group; either
`C:\msys64\ucrt64\bin` or `C:\msys64\mingw64\bin` must be on `PATH`, or
`WINDRES` must point at `windres.exe`.

Builds with the MSVC toolchain also compile, but `build.rs` skips the `.rc`
resource and `-mwindows` for anything other than Windows + GNU, so an MSVC
binary has no icon and opens a console. Use GNU for anything you ship.

A **release** build also needs the DirectX shader compiler, because GPUI turns
its HLSL into shader bytecode only when `debug_assertions` is off. `fxc.exe`
ships with the Windows SDK:

```powershell
$fxc = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter fxc.exe |
  Where-Object { $_.FullName -match '\\x64\\' } | Select-Object -First 1
$env:GPUI_FXC_PATH = $fxc.FullName
cargo build -p massey-gui --release
```

Without the SDK, use a debug build for local testing and packaging (see
[Packaging](#packaging-windows)); debug builds use shaders that ship with GPUI
and need no `fxc`.

### macOS

```sh
xcode-select --install
rustup default stable
```

The bundle scripts additionally use `sips`, `iconutil` and `codesign`, all of
which are part of the command line tools.

## Build and run

```sh
cargo build -p massey-gui            # debug binary
cargo run -p massey-gui              # first example
cargo run -p massey-gui -- examples/05-loop.mc
cargo run -p massey-gui -- --asm     # start in the assembler view
```

The debug profile builds every dependency with `opt-level = 2` (see the
workspace `Cargo.toml`); without that GPUI is unusably slow even in a debug
build of our own crate.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy -p massey-gui --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands on Windows and macOS, and `cargo test -p massey-core`
on Linux (the machine model has no GUI dependencies, the window does).

## Headless mode

Every build also contains the text frame renderer used for scripts and tests:

```sh
masseycode examples/01-addition.mc --dump=4
```

It prints one frame (program, registers and memory) after running the given
number of instructions and exits. `MASSEY_DUMP_COLS` and `MASSEY_DUMP_ROWS`
override the frame size, which defaults to 100×30.

## Packaging Windows

```powershell
pwsh -File scripts/package-windows.ps1                 # release build + zip
pwsh -File scripts/package-windows.ps1 -Profile debug # when fxc is unavailable
```

The script builds `massey-gui`, strips the executable, assembles
`dist/MasseyCode/` (`masseycode.exe`, `examples/*.mc`, `README.txt`) and writes
`dist/MasseyCode-win64.zip`. `-NoBuild` packages an existing
`target/<profile>/masseycode.exe`, `-Version` stamps the text file and
`-Clean` removes `dist/` first.

The equivalent by hand:

```powershell
cargo build -p massey-gui --release
C:\msys64\ucrt64\bin\strip.exe --strip-all target\release\masseycode.exe
```

A stripped release binary is a few MiB; a stripped debug binary is around
44 MiB, which is why releases use the release profile.

## Packaging macOS

```sh
bash scripts/make-macos-app.sh
open dist/MasseyCode.app
```

The script builds `-p massey-gui --release`, copies the binary into
`MasseyCode.app/Contents/MacOS/`, writes `Info.plist`, builds `MasseyCode.icns`
from `assets/masseycode-1024.png` with `sips` and `iconutil`, ad-hoc signs the
bundle (`codesign --sign -`) and zips it with `ditto` to
`dist/MasseyCode-macos.zip`. Set `MASSEYCODES_SKIP_BUILD=1` to bundle an
already-built `target/release/masseycode`.

The bundle is ad-hoc signed, not notarised. On another Mac the first launch
needs right-click → Open, or:

```sh
xattr -dr com.apple.quarantine MasseyCode.app
```

## Regenerating the icon

The icon is drawn in code so both platforms use the same artwork:

```sh
python -m pip install pillow
python scripts/make-icon.py
```

This rewrites `assets/masseycode.ico` (embedded on Windows) and
`assets/masseycode-1024.png` (source for the macOS `.icns`). Both files are
committed; only run it when the artwork changes, then rebuild.

## Releasing

1. Update `CHANGELOG.md` and the version in `Cargo.toml` (`[workspace.package]`).
2. `cargo fmt --all -- --check`, `cargo clippy … -D warnings`,
   `cargo test --workspace`.
3. Commit, tag, push:

   ```sh
   git tag v0.1.0
   git push origin main --tags
   ```

The `Release` workflow then builds `MasseyCode-win64.zip` (GNU release build,
stripped) and `MasseyCode-macos.zip` (`.app` bundle), smoke-tests both with
`--dump=2`, and attaches them to a GitHub Release with generated notes.
`workflow_dispatch` builds the same artifacts without publishing, which is the
quickest way to check a packaging change.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| `need windres to embed the Windows icon` | MinGW is not on `PATH`. Add `C:\msys64\ucrt64\bin`, or set `WINDRES` to the full path. |
| `Failed to find fxc.exe` in a release build | Windows SDK missing, or `fxc.exe` not found. Set `GPUI_FXC_PATH` (see above) or build with `-Profile debug`. |
| `strip not found` from the packaging script | Install MSYS2, or put `strip.exe` on `PATH`. |
| The Windows binary opens a console window | It was built with MSVC. Rebuild with the GNU toolchain; `build.rs` only applies `-mwindows` for Windows + GNU. |
| `xcrun: error: invalid active developer path` | `xcode-select --install` (again) on macOS. |
| macOS bundle: "app is damaged" | The ad-hoc signature does not survive some transfers; re-run `scripts/make-macos-app.sh`, or clear quarantine with `xattr -dr com.apple.quarantine`. |
| `error: linking with cc failed` on Windows | The GNU toolchain cannot find `gcc`; install MSYS2 and add its `bin` to `PATH`. |
| Blank headless frame in a pipe | `--dump` attaches to the parent console. Run it from a terminal, or redirect the output explicitly (`--dump=2 > frame.txt`). |

## Project files worth knowing

| Path | Purpose |
| --- | --- |
| `crates/massey-gui/build.rs` | embeds the icon, sets the Windows GUI subsystem |
| `assets/masseycode.rc` | the Windows resource script |
| `assets/Info.plist` | macOS bundle metadata |
| `scripts/make-icon.py` | generates both icon files |
| `scripts/capture.ps1` | regenerates `docs/screenshots/` |
| `scripts/make-macos-app.sh` | builds the macOS bundle |
| `scripts/package-windows.ps1` | builds the Windows zip |
| `.github/workflows/ci.yml` | tests on Windows, macOS and Linux |
| `.github/workflows/release.yml` | tagged releases with both packages |
