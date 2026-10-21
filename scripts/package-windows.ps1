# Package MasseyCode for Windows.
#
#   pwsh -File scripts/package-windows.ps1
#   pwsh -File scripts/package-windows.ps1 -Profile debug -Version 0.1.0
#
# Builds the GUI with the GNU toolchain, strips the binary, assembles
# dist/MasseyCode/ (executable + examples + README.txt) and writes
# dist/MasseyCode-win64.zip.
#
# The GNU toolchain is required: build.rs embeds the icon with windres and
# sets the GUI subsystem, which are both Windows + GNU only.  `strip` comes
# from the same MinGW installation.

param(
    [ValidateSet("debug", "release")]
    [string]$Profile = "release",
    [string]$Version = "",
    [switch]$NoBuild,
    [switch]$Clean,
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    $cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (-not (Test-Path $cargo)) { $cargo = "cargo" }

    if (-not $NoBuild) {
        if ($Profile -eq "release") {
            & $cargo build -p massey-gui --release
        } else {
            & $cargo build -p massey-gui
        }
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }

    $exeDir = if ($Profile -eq "release") { "target\release" } else { "target\debug" }
    $exe = Join-Path $exeDir "masseycode.exe"
    if (-not (Test-Path $exe)) { throw "missing $exe (build it first)" }

    # Version: argument, else the workspace version out of Cargo.toml.
    if (-not $Version) {
        $line = Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1
        if ($line) { $Version = $line.Matches[0].Groups[1].Value } else { $Version = "0.0.0" }
    }

    # strip: MinGW ships it next to windres; a plain PATH lookup is the fallback.
    $strip = $null
    foreach ($candidate in @(
            "C:\msys64\ucrt64\bin\strip.exe",
            "C:\msys64\mingw64\bin\strip.exe",
            "strip"
        )) {
        if ($candidate.Contains("\") -and (Test-Path $candidate)) { $strip = $candidate; break }
        if (-not $candidate.Contains("\")) {
            $found = Get-Command $candidate -ErrorAction SilentlyContinue
            if ($found) { $strip = $found.Source; break }
        }
    }
    if (-not $strip) {
        throw "strip not found: install MSYS2 (mingw-w64) or put strip.exe on PATH"
    }

    $package = Join-Path $OutDir "MasseyCode"
    $stripped = Join-Path $OutDir "masseycode.exe"
    if ($Clean -and (Test-Path $OutDir)) { Remove-Item $OutDir -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $package | Out-Null

    Copy-Item $exe $stripped -Force
    & $strip --strip-all $stripped
    if ($LASTEXITCODE -ne 0) { throw "strip failed" }

    Copy-Item $stripped (Join-Path $package "masseycode.exe") -Force
    Remove-Item $stripped -Force

    $examples = Join-Path $package "examples"
    if (Test-Path $examples) { Remove-Item $examples -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $examples | Out-Null
    Copy-Item "examples\*.mc" $examples -Force

    $readme = @"
MasseyCode $Version -- a visual debugger for the MASSEY machine
================================================================

Run masseycode.exe and open a file from the examples folder, or drag a .mc
file onto the window.

Keyboard
  F11              step one register transfer
  F10              step one whole instruction
  F5 / Ctrl+R      run, one transfer per tick
  F6               run, one instruction per tick
  b / Backspace    step back one transfer
  Ctrl+M           switch machine code / assembler
  Ctrl+Shift+R     reset the machine
  Ctrl+O           open a file
  F1               help
  q / Esc          quit

Command line
  masseycode.exe [program.mc] [--asm] [--dump[=N]] [--help]

  --dump=N  print N instructions as a plain text frame and exit
            (headless; useful for tests and scripts)

The full manual is in the repository: docs/MACHINE.md and docs/DESIGN.md.
MasseyCode is MIT licensed.
"@
    $readme | Set-Content -Path (Join-Path $package "README.txt") -Encoding UTF8

    $zip = Join-Path $OutDir "MasseyCode-win64.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path $package -DestinationPath $zip -CompressionLevel Optimal

    $exeSize = (Get-Item (Join-Path $package "masseycode.exe")).Length
    $zipSize = (Get-Item $zip).Length
    Write-Output ("packaged MasseyCode {0} ({1} profile)" -f $Version, $Profile)
    Write-Output ("  {0}  {1:N1} MiB" -f (Join-Path $package "masseycode.exe"), ($exeSize / 1MB))
    Write-Output ("  {0}  {1:N1} MiB" -f $zip, ($zipSize / 1MB))
}
finally {
    Pop-Location
}
