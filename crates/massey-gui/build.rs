//! Embed the Windows icon and link the GUI binary as a Windows-subsystem
//! executable, so double-clicking it does not open a console window.
//!
//! `windres` comes with the MinGW toolchain used for the GNU target.  If it is
//! not on PATH, the usual MSYS2 location is tried.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/masseycode.ico");
    println!("cargo:rerun-if-changed=assets/masseycode.rc");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "gnu" {
        return;
    }

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("../../assets");
    let rc = assets.join("masseycode.rc");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let object = out.join("masseycode_icon.o");

    let windres = std::env::var("WINDRES")
        .ok()
        .filter(|path| Path::new(path).exists())
        .or_else(|| {
            ["windres", r"C:\msys64\ucrt64\bin\windres.exe"]
                .into_iter()
                .find(|candidate| {
                    candidate.contains('\\') && Path::new(candidate).exists()
                        || !candidate.contains('\\')
                            && Command::new(candidate).arg("--version").output().is_ok()
                })
                .map(str::to_string)
        })
        .expect("need windres to embed the Windows icon");

    let status = Command::new(windres)
        .current_dir(&manifest)
        .args([
            "-I",
            &assets.to_string_lossy(),
            "-i",
            &rc.to_string_lossy(),
            "-o",
            &object.to_string_lossy(),
        ])
        .status()
        .expect("failed to run windres");
    assert!(status.success(), "windres failed");

    println!("cargo:rustc-link-arg-bin=masseycode={}", object.display());
    println!("cargo:rustc-link-arg-bin=masseycode=-mwindows");
}
