//! The visual language: a Zed-like, monochrome workbench with one accent and a
//! small set of semantic colours borrowed from the terminal debugger.
//!
//! Nothing here is decorative: blue is the current/PC, yellow is the register
//! the current instruction acts on, green is a value that changed in the last
//! step and red is a machine fault.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

/// Window and application background.
pub fn bg() -> Hsla {
    rgb(0x0B0C0E).into()
}

/// The title bar is the same value as the window; its border separates them.
pub fn title_bg() -> Hsla {
    rgb(0x0B0C0E).into()
}

/// A pane background.
pub fn panel() -> Hsla {
    rgb(0x111316).into()
}

/// A slightly raised strip, used for pane headers, the debug bar and the
/// status bar.
pub fn panel_alt() -> Hsla {
    rgb(0x16181D).into()
}

/// A recessed strip, used for gaps and the register pane.
pub fn panel_soft() -> Hsla {
    rgb(0x0E1013).into()
}

pub fn border() -> Hsla {
    rgb(0x22252B).into()
}

pub fn border_soft() -> Hsla {
    rgb(0x191C21).into()
}

pub fn text() -> Hsla {
    rgb(0xD6DAE1).into()
}

pub fn muted() -> Hsla {
    rgb(0x8A919D).into()
}

pub fn faint() -> Hsla {
    rgb(0x5B626E).into()
}

/// The only UI accent: the current instruction, the PC and focus.
pub fn accent() -> Hsla {
    rgb(0x6C9EF8).into()
}

pub fn current_row() -> Hsla {
    rgb(0x1B2230).into()
}

pub fn pc_row() -> Hsla {
    rgb(0x16243A).into()
}

/// The register the current instruction acts on.
pub fn active() -> Hsla {
    rgb(0xE5C07B).into()
}

/// A value changed by the last step.
pub fn changed() -> Hsla {
    rgb(0x7BD88F).into()
}

/// A machine fault or an invalid source line.
pub fn danger() -> Hsla {
    rgb(0xE06C75).into()
}

pub fn mono() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Cascadia Mono"
    } else {
        "DejaVu Sans Mono"
    }
}

pub fn sans() -> &'static str {
    if cfg!(target_os = "macos") {
        "Helvetica Neue"
    } else if cfg!(target_os = "windows") {
        "Segoe UI Variable Text"
    } else {
        "DejaVu Sans"
    }
}

/// Height of one row of machine text.
pub const ROW_H: f32 = 20.0;
/// Height of the workspace title bar (matches gpui-component's TitleBar).
pub const TITLE_H: f32 = 34.0;
/// Height of the debug toolbar strip.
pub const TOOLBAR_H: f32 = 44.0;
/// Height of the message + status footer.
pub const FOOTER_H: f32 = 42.0;
/// Height of a pane header.
pub const HEADER_H: f32 = 26.0;

/// Install the dark, Zed-like variant of the gpui-component theme and use the
/// mono font throughout the machine panes.
pub fn install(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |theme| {
        theme.background = bg();
        theme.foreground = text();
        theme.border = border();
        theme.muted = panel_alt();
        theme.muted_foreground = muted();
        theme.primary = accent();
        theme.primary_foreground = bg();
        theme.secondary = panel_alt();
        theme.secondary_foreground = text();
        theme.secondary_hover = current_row();
        theme.secondary_active = pc_row();
        theme.danger = danger();
        theme.danger_foreground = text();
        theme.title_bar = title_bg();
        theme.title_bar_border = border();
        theme.radius = px(5.0);
        theme.radius_lg = px(8.0);
        theme.shadow = false;
        theme.font_family = sans().into();
        theme.font_size = px(13.0);
        theme.mono_font_family = mono().into();
        theme.mono_font_size = px(13.0);
    });
}
