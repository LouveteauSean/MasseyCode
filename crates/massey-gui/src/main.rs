//! MasseyCode — a visual debugger for the MASSEY machine.
//!
//! The binary has two faces:
//!
//! * without `--dump` it opens the window;
//! * with `--dump[=N]` it prints one plain-text frame and exits, which makes the
//!   debugger scriptable and testable.

use std::path::PathBuf;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

use crate::app::MasseyApp;
use crate::debugger::Debugger;

mod app;
mod asm_highlight;
mod cpu_pane;
mod debugger;
mod frame;
mod help;
mod pdf_asm;
mod theme;

const USAGE: &str = "usage: masseycode [program-file] [--asm] [--dump[=N]] [--help]

  program-file   machine code text file, one hex word per line
                 (default: the first worked example)
  --asm          start in the Assembler view instead of Machine Code
  --dump[=N]     non-interactive: run N instructions (default 0), print one
                 plain frame and exit; MASSEY_DUMP_COLS/ROWS can set its size
  --help         show this message
";

#[derive(Default)]
struct Options {
    program: Option<PathBuf>,
    asm: bool,
    dump: Option<u64>,
    help: bool,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    for argument in args {
        if argument == "--help" || argument == "-h" {
            options.help = true;
        } else if argument == "--asm" {
            options.asm = true;
        } else if argument == "--dump" {
            options.dump = Some(0);
        } else if let Some(value) = argument.strip_prefix("--dump=") {
            let steps = value
                .parse::<u64>()
                .map_err(|_| format!("--dump needs a step count, e.g. --dump=3 ({value})"))?;
            options.dump = Some(steps);
        } else if argument.starts_with('-') {
            return Err(format!("unknown option '{argument}'"));
        } else if options.program.is_some() {
            return Err("more than one program file given".to_string());
        } else {
            options.program = Some(PathBuf::from(argument));
        }
    }
    Ok(options)
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn GetModuleHandleW(name: *const u16) -> isize;
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn LoadIconW(instance: isize, name: *const u16) -> isize;
    fn SendMessageW(window: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetClassLongPtrW(window: isize, index: i32, value: isize) -> isize;
}

#[cfg(windows)]
#[link(name = "shell32")]
unsafe extern "system" {
    fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
}

/// Give the window (and therefore the taskbar button) the icon embedded in the
/// executable.  The executable resource alone is not always picked up by a
/// custom GPUI window class, so this also sends `WM_SETICON`.
#[cfg(windows)]
fn set_window_icon() {
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const GCLP_HICON: i32 = -14;
    const GCLP_HICONSM: i32 = -34;

    let app_id: Vec<u16> = "MasseyCode.Teaching\0".encode_utf16().collect();
    let title: Vec<u16> = "MasseyCode\0".encode_utf16().collect();

    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr());
    }

    let mut window = 0;
    for _ in 0..40 {
        unsafe {
            window = FindWindowW(std::ptr::null(), title.as_ptr());
        }
        if window != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    if window == 0 {
        return;
    }

    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        // `1` is the integer resource id written by `assets/masseycode.rc`;
        // `MAKEINTRESOURCE` is exactly this integer reinterpreted as a pointer.
        let resource = std::ptr::without_provenance::<u16>(1);
        let icon = LoadIconW(instance, resource);
        if icon == 0 {
            return;
        }
        SendMessageW(window, WM_SETICON, ICON_BIG, icon);
        SendMessageW(window, WM_SETICON, ICON_SMALL, icon);
        SetClassLongPtrW(window, GCLP_HICON, icon);
        SetClassLongPtrW(window, GCLP_HICONSM, icon);
    }
}

fn main() {
    #[cfg(windows)]
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--help" || argument == "-h" || argument.starts_with("--dump"))
    {
        // The GUI binary is linked as a Windows-subsystem executable, so it has
        // no console of its own.  The headless `--dump`/`--help` modes attach
        // to the terminal that started them.
        unsafe {
            AttachConsole(u32::MAX);
        }
    }

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("error: {error}");
            eprint!("{USAGE}");
            std::process::exit(2);
        }
    };
    if options.help {
        print!("{USAGE}");
        return;
    }

    let debugger = match Debugger::load(options.program.as_deref()) {
        Ok(debugger) => debugger,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };

    if let Some(steps) = options.dump {
        let mut debugger = debugger;
        for _ in 0..steps {
            if debugger.halted || debugger.faulted {
                break;
            }
            debugger.step_instruction();
        }
        let cols = env_usize("MASSEY_DUMP_COLS", 100);
        let rows = env_usize("MASSEY_DUMP_ROWS", 30);
        println!("{}", frame::render(&debugger, cols, rows));
        return;
    }

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            theme::install(cx);

            let bounds = Bounds::centered(None, size(px(1180.0), px(760.0)), cx);
            let mut window_options = TitleBar::window_options();
            window_options.window_bounds = Some(WindowBounds::Windowed(bounds));
            window_options.window_min_size = Some(size(px(900.0), px(600.0)));
            window_options.window_background = WindowBackgroundAppearance::Opaque;
            window_options.titlebar = Some(TitlebarOptions {
                title: Some("MasseyCode".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(9.0), px(9.0))),
            });

            let (window, _view) = gpui_kit::open_window(window_options, cx, |_, cx| {
                cx.new(|cx| MasseyApp::new(debugger, options.asm, cx))
            })
            .expect("failed to open the MasseyCode window");

            #[cfg(windows)]
            set_window_icon();

            window
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .ok();
            cx.activate(true);
        });
}
