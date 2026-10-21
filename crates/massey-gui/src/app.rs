//! The window: one instrument for watching the machine cycle.
//!
//! The source pane is editable in both views; the machine half is read-only and
//! shows what the current transfer did, read from the `MicroOp` the VM reports.

use std::time::Duration;

use gpui_kit::assets::IconName as Icons;
use gpui_kit::base::input::{
    Diagnostic, DiagnosticSet, DiagnosticSeverity, RangeDecoration, RangeDecorationCollection,
    RangeDecorationStyle, TextDecoration, TextDecorationCollection,
};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, EditorState, InputEvent, Position};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use massey_core::{BusDirection, SourceMode};

use crate::asm_highlight::{self, Token};
use crate::cpu_pane;
use crate::debugger::Debugger;
use crate::pdf_asm;
use crate::theme;

pub struct MasseyApp {
    debugger: Debugger,
    focus: FocusHandle,
    focused_once: bool,
    running: bool,
    /// True while the running timer is stepping whole instructions instead of
    /// one transfer per tick.
    run_instructions: bool,
    mode: SourceMode,
    editor: Option<Entity<EditorState>>,
    editor_sub: Option<Subscription>,
    editor_observe: Option<Subscription>,
    syntax: Option<TextDecorationCollection>,
    current_line: Option<RangeDecorationCollection>,
    /// (highlighted instruction row, PC) the decorations were built for.
    decorated: Option<(Option<usize>, u8)>,
    invalid: Vec<(usize, String)>,
}

impl MasseyApp {
    pub fn new(debugger: Debugger, assembler_view: bool, cx: &mut Context<Self>) -> Self {
        Self {
            debugger,
            focus: cx.focus_handle(),
            focused_once: false,
            running: false,
            run_instructions: false,
            mode: if assembler_view {
                SourceMode::Assembly
            } else {
                SourceMode::MachineCode
            },
            editor: None,
            editor_sub: None,
            editor_observe: None,
            syntax: None,
            current_line: None,
            decorated: None,
            invalid: Vec::new(),
        }
    }

    // -- actions -----------------------------------------------------------

    fn step_transfer(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        self.debugger.step_transfer();
        cx.notify();
    }

    fn step_instruction(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        self.debugger.step_instruction();
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if self.running {
            return;
        }
        self.debugger.back();
        cx.notify();
    }

    fn reset_machine(&mut self, cx: &mut Context<Self>) {
        self.running = false;
        self.debugger.reset();
        cx.notify();
    }

    /// Open a machine-code file through the platform file picker.
    ///
    /// The editor entity is rebuilt on the next frame from the loaded image, so
    /// the new program appears in whichever view is active.
    fn open_program(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pause_if_running();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a MASSEY program".into()),
        });
        let entity = cx.entity();
        cx.spawn_in(window, async move |_view, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            entity.update(cx, |app, cx| {
                let Some(path) = picked else {
                    app.debugger.message = "open cancelled".to_string();
                    cx.notify();
                    return;
                };
                match Debugger::load(Some(&path)) {
                    Ok(loaded) => {
                        app.debugger = loaded;
                        app.invalid.clear();
                        app.editor = None;
                        app.editor_sub = None;
                        app.editor_observe = None;
                        app.syntax = None;
                        app.current_line = None;
                        app.decorated = None;
                        app.debugger.message = format!("opened {}", app.display_name());
                    }
                    Err(error) => {
                        app.debugger.message = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_help(&mut self, cx: &mut Context<Self>) {
        self.pause_if_running();
        self.debugger.toggle_help();
        cx.notify();
    }

    fn pause_if_running(&mut self) {
        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
        }
    }

    fn toggle_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
            cx.notify();
            return;
        }
        if self.debugger.halted || self.debugger.faulted {
            self.debugger.message = "machine already stopped".to_string();
            cx.notify();
            return;
        }
        self.running = true;
        self.run_instructions = false;
        self.debugger.message.clear();
        self.spawn_run(window, cx);
        cx.notify();
    }

    /// Same timer loop as [`Self::toggle_run`], but one whole instruction per
    /// tick.  The diagram jumps to the last transfer of the instruction instead
    /// of animating each of them.
    fn toggle_run_instructions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
            cx.notify();
            return;
        }
        if self.debugger.halted || self.debugger.faulted {
            self.debugger.message = "machine already stopped".to_string();
            cx.notify();
            return;
        }
        self.running = true;
        self.run_instructions = true;
        self.debugger.message.clear();
        self.spawn_run(window, cx);
        cx.notify();
    }

    /// Continue runs the machine until it halts: either one register transfer
    /// per tick (slow, shows the datapath) or one whole instruction per tick.
    fn spawn_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let fast = self.run_instructions;
        cx.spawn_in(window, async move |_view, cx| loop {
            let keep_running = entity.update(cx, |app, cx| {
                if !app.running {
                    return false;
                }
                if fast {
                    app.debugger.step_instruction();
                } else {
                    app.debugger.step_transfer();
                }
                if app.debugger.halted || app.debugger.faulted {
                    app.running = false;
                    app.debugger.message = if app.debugger.halted {
                        "halted".to_string()
                    } else {
                        "machine faulted".to_string()
                    };
                }
                cx.notify();
                app.running
            });
            if !keep_running {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(if fast { 90 } else { 220 }))
                .await;
        })
        .detach();
    }

    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    fn text_for_mode(&self, mode: SourceMode) -> String {
        match mode {
            SourceMode::MachineCode => self
                .debugger
                .image
                .iter()
                .map(|word| format!("{word:04X}"))
                .collect::<Vec<_>>()
                .join("\n"),
            SourceMode::Assembly => self
                .debugger
                .image
                .iter()
                .map(|word| pdf_asm::disassemble_line(*word).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    fn parse_editor_text(&self, text: &str) -> (Option<Vec<u16>>, Vec<(usize, String)>) {
        let mut image = Vec::new();
        let mut errors = Vec::new();
        for (line, raw) in text.lines().enumerate() {
            match self.mode {
                SourceMode::MachineCode => {
                    let word = raw.trim();
                    if word.len() == 4 && word.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        image.push(u16::from_str_radix(word, 16).unwrap_or(0));
                    } else {
                        errors.push((
                            line,
                            "machine code must be exactly four hexadecimal digits".to_string(),
                        ));
                    }
                }
                SourceMode::Assembly => match pdf_asm::parse_line(raw) {
                    Ok(Some(word)) => image.push(word),
                    Ok(None) => image.push(self.debugger.image.get(line).copied().unwrap_or(0)),
                    Err(message) => {
                        errors.push((line, message));
                    }
                },
            }
        }
        if errors.is_empty() {
            (Some(image), errors)
        } else {
            (None, errors)
        }
    }

    fn apply_diagnostics(&self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        let text = editor.read(cx).text().clone();
        let lines: Vec<String> = text.to_string().lines().map(str::to_string).collect();
        let mut diagnostics = DiagnosticSet::new(&text);
        for (line, message) in &self.invalid {
            if *line >= lines.len() {
                continue;
            }
            let end = lines[*line].chars().count() as u32;
            diagnostics.push(
                Diagnostic::new(
                    Position::new(*line as u32, 0)..Position::new(*line as u32, end.max(1)),
                    message.clone(),
                )
                .with_severity(DiagnosticSeverity::Error),
            );
        }
        editor.update(cx, |state, cx| {
            if let Some(target) = state.diagnostics_mut() {
                *target = diagnostics;
            }
            cx.notify();
        });
    }

    fn on_editor_event(
        &mut self,
        editor: &Entity<EditorState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event, InputEvent::Change) {
            return;
        }
        let text = editor.read(cx).text().to_string();
        self.validate_text(&text, editor, cx);
    }

    fn validate_text(&mut self, text: &str, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        let (image, errors) = self.parse_editor_text(text);
        self.invalid = errors;
        if let Some(image) = image {
            if image != self.debugger.image {
                self.running = false;
                self.debugger.set_image(&image);
            }
        }
        self.apply_diagnostics(editor, cx);
        self.refresh_decorations(editor, cx);
        cx.notify();
    }

    /// Rebuild the syntax colours and the current-line fill for the editor.
    fn refresh_decorations(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        let text = editor.read(cx).text().to_string();
        let mode = self.mode;
        let lines: Vec<&str> = text.split('\n').collect();
        let mut offsets = Vec::with_capacity(lines.len());
        let mut offset = 0usize;
        for line in &lines {
            offsets.push(offset);
            offset += line.len() + 1;
        }

        let mut decorations = Vec::new();
        for (index, line) in lines.iter().enumerate() {
            let base = offsets[index];
            for span in asm_highlight::line_spans(mode, line) {
                let (color, weight) = match span.token {
                    Token::Mnemonic => (theme::accent(), Some(FontWeight::SEMIBOLD)),
                    Token::Register => (theme::changed(), None),
                    Token::Number if mode == SourceMode::MachineCode => {
                        let word = line.trim();
                        let value = u16::from_str_radix(word, 16).unwrap_or(0);
                        if pdf_asm::disassemble_line(value).is_some() {
                            (theme::accent(), None)
                        } else {
                            (theme::faint(), None)
                        }
                    }
                    Token::Number => (theme::text(), None),
                    Token::Bracket | Token::Punct => (theme::muted(), None),
                };
                decorations.push(TextDecoration::new(
                    base + span.start..base + span.end,
                    HighlightStyle {
                        color: Some(color),
                        font_weight: weight,
                        ..Default::default()
                    },
                ));
            }
        }
        if let Some(collection) = self.syntax.clone() {
            collection.set(decorations, cx);
        }

        let pc = self.debugger.machine.pc as usize;
        let row = self.debugger.current_row();
        let highlight = row.filter(|row| *row < lines.len()).or(Some(pc));
        let current = highlight.map(|row| {
            let start = offsets.get(row).copied().unwrap_or(0);
            let length = lines.get(row).map(|line| line.len()).unwrap_or(0).max(1);
            RangeDecoration::new(start..start + length)
                .with_style(RangeDecorationStyle::Fill)
                .with_color(theme::accent().opacity(0.22))
        });
        if let Some(collection) = self.current_line.clone() {
            collection.set(current.into_iter().collect(), cx);
        }
        self.decorated = Some((row, self.debugger.machine.pc));
    }

    /// The address gutter beside the editor: `00`, `01`, ... plus a triangle
    /// marker on the line the PC points at.  The editor's own decimal line
    /// numbers are switched off; this column is the address.
    fn gutter(&self, editor: &Entity<EditorState>, height: f32, cx: &App) -> AnyElement {
        const WIDTH: f32 = 44.0;
        // The editor's own content starts below its 8px top padding; the gutter
        // must start at the same y so the addresses line up with the text.
        const TOP_INSET: f32 = 8.0;
        let (scroll, line_h) = {
            let state = editor.read(cx);
            let offset = f32::from(state.scroll_offset().y);
            let line_h = state
                .line_height()
                .map(f32::from)
                .unwrap_or(theme::ROW_H)
                .max(1.0);
            ((-offset).max(0.0), line_h)
        };
        let first = (scroll / line_h).floor() as usize;
        let visible = ((height - TOP_INSET) / line_h).ceil() as usize + 2;
        let total = self.debugger.image.len();
        let pc = self.debugger.machine.pc as usize;
        let current = self.debugger.current_row();
        let address_size = (line_h * 0.58).clamp(9.5, 12.0);

        let mut rows = Vec::new();
        for line in first..(first + visible).min(total) {
            let top = TOP_INSET + line as f32 * line_h - scroll;
            let is_pc = line == pc;
            let is_current = current == Some(line);
            let is_invalid = self.invalid.iter().any(|(bad, _)| *bad == line);
            let color = if is_invalid {
                theme::danger()
            } else if is_pc {
                theme::accent()
            } else {
                theme::faint()
            };
            rows.push(
                h_flex()
                    .absolute()
                    .top(px(top))
                    .left(px(0.0))
                    .w_full()
                    .h(px(line_h))
                    .items_center()
                    .justify_end()
                    .gap(px(3.0))
                    .pr(px(6.0))
                    .when(is_current, |this| {
                        this.border_l_2()
                            .border_color(theme::accent().opacity(0.85))
                    })
                    .child(
                        div()
                            .w(px(8.0))
                            .text_size(px(address_size * 0.85))
                            .text_color(if is_pc {
                                theme::accent()
                            } else {
                                theme::faint()
                            })
                            .child(if is_pc { "\u{25B6}" } else { " " }),
                    )
                    .child(
                        div()
                            .px(px(3.0))
                            .rounded(px(3.0))
                            .when(is_pc, |this| this.bg(theme::accent().opacity(0.16)))
                            .text_size(px(address_size))
                            .font_family(theme::mono())
                            .text_color(color)
                            .child(format!("{line:02X}")),
                    )
                    .into_any_element(),
            );
        }

        div()
            .relative()
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .children(rows)
            .into_any_element()
    }

    fn set_mode(&mut self, mode: SourceMode, window: &mut Window, cx: &mut Context<Self>) {
        if !self.invalid.is_empty() {
            self.debugger.message = "fix the red lines before switching views".to_string();
            cx.notify();
            return;
        }
        if mode == self.mode {
            return;
        }
        let text = self.text_for_mode(mode);
        self.mode = mode;
        self.invalid.clear();
        if let Some(editor) = self.editor.clone() {
            editor.update(cx, |state, cx| state.set_value(text, window, cx));
            self.apply_diagnostics(&editor, cx);
            self.refresh_decorations(&editor, cx);
        }
        cx.notify();
    }

    // -- keyboard ----------------------------------------------------------

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        // Windows uses Ctrl for the shortcuts; macOS keeps Ctrl working but also
        // accepts the native Cmd (GPUI reports that as `platform`).
        let primary = modifiers.control || (cfg!(target_os = "macos") && modifiers.platform);
        if primary && !modifiers.alt {
            match (key, modifiers.shift) {
                ("m", false) => self.set_mode(self.mode.toggled(), window, cx),
                ("r", false) => self.toggle_run_or_pause(window, cx),
                ("r", true) => self.reset_machine(cx),
                ("o", false) => self.open_program(window, cx),
                _ => {}
            }
            return;
        }
        if modifiers.control || modifiers.alt || modifiers.platform {
            return;
        }

        if self.debugger.show_help {
            if key == "q" {
                cx.quit();
            } else {
                self.debugger.close_help();
                cx.notify();
            }
            return;
        }

        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
            if key == "q" {
                cx.quit();
                return;
            }
            cx.notify();
            return;
        }

        match key {
            "f11" | "n" | "enter" | "space" => self.step_transfer(cx),
            "f10" | "i" => self.step_instruction(cx),
            "f5" => self.toggle_run_or_pause(window, cx),
            "f6" => self.toggle_run_instructions_or_pause(window, cx),
            "b" | "backspace" => self.back(cx),
            "r" => self.toggle_run_or_pause(window, cx),
            "c" => self.toggle_run_or_pause(window, cx),
            "j" | "down" => self.debugger.scroll_memory(1, 0),
            "k" | "up" => self.debugger.scroll_memory(-1, 0),
            "h" | "left" => self.debugger.scroll_memory(0, -1),
            "l" | "right" => self.debugger.scroll_memory(0, 1),
            "home" => self.debugger.memory_home(),
            "end" => self.debugger.memory_end(),
            "?" | "f1" => self.debugger.toggle_help(),
            "q" | "escape" => cx.quit(),
            _ => {}
        }
        cx.notify();
    }

    fn toggle_run_or_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
            cx.notify();
        } else {
            self.toggle_run(window, cx);
        }
    }

    fn toggle_run_instructions_or_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running {
            self.running = false;
            self.debugger.message = "paused".to_string();
            cx.notify();
        } else {
            self.toggle_run_instructions(window, cx);
        }
    }

    // -- layout helpers ----------------------------------------------------

    fn display_name(&self) -> String {
        std::path::Path::new(&self.debugger.name)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.debugger.name)
            .to_string()
    }

    fn memory(
        &self,
        mem_rows: usize,
        mem_cols: usize,
        cell_width: f32,
        width: f32,
        height: f32,
    ) -> AnyElement {
        let row0 = self.debugger.mem_row0;
        let col0 = self.debugger.mem_col0;
        // The bus transfer is the machine's own record of memory traffic, so
        // the view lights the bus and the addressed cell only when the transfer
        // really reads or writes memory.
        let bus = cpu_pane::live_bus(&self.debugger);
        let reading_memory = bus.is_some();
        let label_h = theme::HEADER_H;
        let ruler_h = if mem_cols < 16 { 20.0 } else { 0.0 };
        let available = (height - label_h - ruler_h).max(40.0);
        let available_row = available / mem_rows.max(1) as f32;
        let row_h = available_row.clamp(9.0, 44.0);
        let memory_text = if row_h >= 30.0 && cell_width >= 40.0 {
            13.0
        } else if row_h >= 20.0 && cell_width >= 28.0 {
            11.0
        } else if row_h >= 13.0 {
            9.5
        } else {
            8.5
        };
        let mut columns = Vec::new();
        columns.push(div().w(px(28.0)).flex_none().into_any_element());
        for column in 0..mem_cols {
            columns.push(
                div()
                    .w(px(cell_width))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .text_size(px(10.5))
                    .font_family(theme::mono())
                    .text_color(theme::faint())
                    .child(format!("{:02X}", col0 + column))
                    .into_any_element(),
            );
        }

        let mut rows = Vec::new();
        for row in 0..mem_rows {
            let address_row = row0 + row;
            let mut cells = Vec::new();
            cells.push(
                div()
                    .w(px(28.0))
                    .flex_none()
                    .text_size(px(10.5))
                    .font_family(theme::mono())
                    .text_color(theme::faint())
                    .child(format!("{address_row:02X}"))
                    .into_any_element(),
            );
            for column in 0..mem_cols {
                let address = address_row * 16 + col0 + column;
                let value = self.debugger.machine.memory[address];
                let is_pc = address == self.debugger.machine.pc as usize;
                let changed = self.debugger.memory_changed(address as u8);
                let read = bus.is_some_and(|bus| {
                    bus.direction == BusDirection::Read && bus.address as usize == address
                });
                let written = bus.is_some_and(|bus| {
                    bus.direction == BusDirection::Write && bus.address as usize == address
                });
                let bg = if read {
                    theme::accent().opacity(0.22)
                } else if written {
                    theme::changed().opacity(0.22)
                } else if is_pc {
                    theme::accent().opacity(0.18)
                } else if changed {
                    theme::changed().opacity(0.13)
                } else if address_row % 2 == 1 {
                    theme::panel_soft()
                } else {
                    theme::panel()
                };
                let color = if read {
                    theme::accent()
                } else if written {
                    theme::changed()
                } else if is_pc {
                    theme::accent()
                } else if changed {
                    theme::changed()
                } else if value == 0 {
                    theme::faint()
                } else {
                    theme::text()
                };
                let border = if read {
                    Some(theme::accent())
                } else if written {
                    Some(theme::changed())
                } else if is_pc {
                    Some(theme::accent().opacity(0.45))
                } else {
                    None
                };
                cells.push(
                    div()
                        .w(px(cell_width))
                        .h_full()
                        .min_h(px(0.0))
                        .flex_none()
                        .rounded(px(2.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg)
                        .when(border.is_some(), |this| {
                            this.border_1()
                                .border_color(border.unwrap_or_else(theme::border_soft))
                        })
                        .text_size(px(memory_text))
                        .font_family(theme::mono())
                        .text_color(color)
                        .child(format!("{value:04X}"))
                        .into_any_element(),
                );
            }
            rows.push(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .max_h(px(row_h))
                    .items_center()
                    .justify_center()
                    .gap(px(2.0))
                    .border_b_1()
                    .border_color(theme::border_soft())
                    .children(cells)
                    .into_any_element(),
            );
        }

        let mar = self.debugger.machine.mar;
        let mar_value = self.debugger.machine.memory[mar as usize];

        v_flex()
            .w(px(width))
            .flex_1()
            .min_h_0()
            .bg(theme::panel())
            .rounded(px(6.0))
            .border_1()
            .border_color(if reading_memory {
                theme::accent()
            } else {
                theme::border()
            })
            .overflow_hidden()
            .child(
                h_flex()
                    .h(px(theme::HEADER_H))
                    .px_3()
                    .items_center()
                    .justify_between()
                    .bg(theme::panel_alt())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::muted())
                            .child("MEMORY"),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .font_family(theme::mono())
                                    .text_color(if reading_memory {
                                        theme::accent()
                                    } else {
                                        theme::muted()
                                    })
                                    .child(format!("@{mar:02X} {mar_value:04X}")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_family(theme::mono())
                                    .text_color(theme::faint())
                                    .child(format!("{}/256", self.debugger.machine.image_len())),
                            ),
                    ),
            )
            .when(mem_cols < 16, |this| {
                this.child(
                    h_flex()
                        .w_full()
                        .h(px(20.0))
                        .px_3()
                        .items_center()
                        .gap(px(2.0))
                        .children(columns),
                )
            })
            .child(
                v_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .gap(px(2.0))
                    .justify_center()
                    .overflow_hidden()
                    .children(rows),
            )
            .into_any_element()
    }

    fn debug_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let stopped = self.debugger.halted || self.debugger.faulted;
        let back_disabled = self.running || self.debugger.history.len() <= 1;
        let step_disabled = self.running || stopped;
        let run_disabled = stopped;

        let back = Button::new("back")
            .ghost()
            .compact()
            .icon(Icons::StepBack)
            .tooltip("Back one transfer (b / Backspace)")
            .disabled(back_disabled)
            .on_click(cx.listener(|this, _, window, cx| {
                this.back(cx);
                this.refocus(window, cx);
            }));
        let next = Button::new("next")
            .ghost()
            .compact()
            .icon(Icons::StepForward)
            .tooltip("Step one transfer (F11)")
            .disabled(step_disabled)
            .on_click(cx.listener(|this, _, window, cx| {
                this.step_transfer(cx);
                this.refocus(window, cx);
            }));
        let next_instruction = Button::new("next-instruction")
            .ghost()
            .compact()
            .icon(Icons::SkipForward)
            .tooltip("Step one whole instruction (F10)")
            .disabled(step_disabled)
            .on_click(cx.listener(|this, _, window, cx| {
                this.step_instruction(cx);
                this.refocus(window, cx);
            }));
        let run = Button::new("run")
            .ghost()
            .compact()
            .icon(if self.running && !self.run_instructions {
                Icons::Pause
            } else {
                Icons::Play
            })
            .tooltip(if self.running && !self.run_instructions {
                "Pause (F5 / Ctrl+R)"
            } else {
                "Run transfer by transfer (F5 / Ctrl+R)"
            })
            .disabled(run_disabled || (self.running && self.run_instructions))
            .on_click(cx.listener(|this, _, window, cx| {
                this.toggle_run_or_pause(window, cx);
                this.refocus(window, cx);
            }));
        let run_instructions = Button::new("run-instructions")
            .ghost()
            .compact()
            .icon(if self.running && self.run_instructions {
                Icons::Pause
            } else {
                Icons::FastForward
            })
            .tooltip(if self.running && self.run_instructions {
                "Pause (F6)"
            } else {
                "Run instruction by instruction (F6)"
            })
            .disabled(run_disabled || (self.running && !self.run_instructions))
            .on_click(cx.listener(|this, _, window, cx| {
                this.toggle_run_instructions_or_pause(window, cx);
                this.refocus(window, cx);
            }));
        let open = Button::new("open")
            .ghost()
            .compact()
            .icon(Icons::FolderOpen)
            .tooltip("Open a program (Ctrl+O)")
            .disabled(self.running)
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_program(window, cx);
                this.refocus(window, cx);
            }));
        let reset = Button::new("reset")
            .ghost()
            .compact()
            .icon(Icons::RefreshCw)
            .tooltip("Reset the machine (Ctrl+Shift+R)")
            .disabled(self.running)
            .on_click(cx.listener(|this, _, window, cx| {
                this.reset_machine(cx);
                this.refocus(window, cx);
            }));
        let help = Button::new("help")
            .ghost()
            .compact()
            .icon(Icons::CircleQuestionMark)
            .tooltip("Help (F1)")
            .toggled(self.debugger.show_help)
            .when(self.debugger.show_help, |button| {
                button.bg(theme::accent().opacity(0.12))
            })
            .on_click(cx.listener(|this, _, window, cx| {
                this.toggle_help(cx);
                this.refocus(window, cx);
            }));

        let mode_button = |mode: SourceMode, label: &'static str| {
            let selected = self.mode == mode;
            Button::new(match mode {
                SourceMode::MachineCode => "mode-machine",
                SourceMode::Assembly => "mode-assembly",
            })
            .ghost()
            .compact()
            .label(label)
            .text_color(if selected {
                theme::text()
            } else {
                theme::muted()
            })
            .toggled(selected)
            .when(selected, |button| button.bg(theme::accent().opacity(0.18)))
            .disabled(!self.invalid.is_empty())
            .tooltip(if self.invalid.is_empty() {
                "Switch view (Ctrl+M)"
            } else {
                "Fix the red lines before switching"
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.set_mode(mode, window, cx);
                this.refocus(window, cx);
            }))
        };
        let mode_switch = h_flex()
            .h(px(28.0))
            .px(px(3.0))
            .items_center()
            .gap_1()
            .rounded(px(6.0))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            .when(!self.invalid.is_empty(), |this| this.opacity(0.55))
            .child(mode_button(SourceMode::MachineCode, "MACHINE CODE"))
            .child(mode_button(SourceMode::Assembly, "ASSEMBLER"));

        h_flex()
            .w_full()
            .h(px(theme::TOOLBAR_H))
            .px_3()
            .items_center()
            .justify_between()
            .gap_3()
            .bg(theme::panel_alt())
            .border_b_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(7.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::faint())
                            .child("CODE"),
                    )
                    .child(mode_switch),
            )
            .child(
                h_flex()
                    .h(px(32.0))
                    .px(px(3.0))
                    .gap_0p5()
                    .items_center()
                    .rounded(px(7.0))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::border())
                    .child(open)
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme::border()).mx(px(4.0)))
                    .child(reset)
                    .child(back)
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme::border()).mx(px(4.0)))
                    .child(next)
                    .child(run)
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme::border()).mx(px(4.0)))
                    .child(next_instruction)
                    .child(run_instructions)
                    .child(div().w(px(1.0)).h(px(16.0)).bg(theme::border()).mx(px(4.0)))
                    .child(help),
            )
            .into_any_element()
    }

    fn title_bar(&self) -> AnyElement {
        h_flex()
            .w_full()
            .h_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .size(px(18.0))
                    .rounded(px(4.0))
                    .bg(theme::accent())
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::bg())
                    .child("M"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("MasseyCode"),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(theme::faint())
                    .child("/"),
            )
            .child(
                div()
                    .max_w(px(520.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(11.0))
                    .font_family(theme::mono())
                    .text_color(theme::muted())
                    .child(self.display_name()),
            )
            .child(div().flex_1())
            .into_any_element()
    }

    fn footer(&self, cols: usize) -> AnyElement {
        let message = if !self.debugger.message.is_empty() {
            self.debugger.message.clone()
        } else if self.debugger.show_help {
            "any key closes the help".to_string()
        } else if let Some(text) = self.debugger.micro_text() {
            let address = self.debugger.machine.instruction_address as usize;
            let word = self.debugger.machine.memory[address];
            let instruction = self.debugger.instruction_text(address);
            if instruction.is_empty() {
                text.to_string()
            } else {
                format!("{text}   \u{00B7}   {address:02X}  {word:04X}  {instruction}")
            }
        } else {
            "F11 transfer \u{00B7} F10 instruction \u{00B7} F5 run \u{00B7} F6 fast \u{00B7} F1 help"
                .to_string()
        };

        let mut status = format!(
            "{}  step {}  PC={:02X}",
            shorten_path(&self.debugger.name, 54),
            self.debugger.step_count(),
            self.debugger.machine.pc
        );
        if self.running {
            status.push_str("  [RUNNING]");
        }
        if self.debugger.halted {
            status.push_str("  [HALTED]");
        }
        if self.debugger.faulted {
            status.push_str("  [FAULTED]");
        }
        let mut ruler = String::new();
        if self.debugger.view_rows < 16 {
            ruler.push_str(&format!(
                "rows {:02X}-{:02X}",
                self.debugger.mem_row0,
                self.debugger.mem_row0 + self.debugger.view_rows - 1
            ));
        }
        if self.debugger.view_cols < 16 {
            if !ruler.is_empty() {
                ruler.push_str("  ");
            }
            ruler.push_str(&format!(
                "cols {:02X}-{:02X}",
                self.debugger.mem_col0,
                self.debugger.mem_col0 + self.debugger.view_cols - 1
            ));
        }
        if self.debugger.history_trimmed() {
            if !ruler.is_empty() {
                ruler.push_str("  ");
            }
            ruler.push_str("history trimmed");
        }

        v_flex()
            .w_full()
            .h(px(theme::FOOTER_H))
            .flex_none()
            .bg(theme::panel_alt())
            .border_t_1()
            .border_color(theme::border())
            .child(
                h_flex().w_full().h(px(18.0)).px_3().items_center().child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(11.0))
                        .font_family(theme::sans())
                        .text_color(if self.debugger.faulted {
                            theme::danger()
                        } else {
                            theme::muted()
                        })
                        .child(message),
                ),
            )
            .child(
                h_flex()
                    .w_full()
                    .h(px(24.0))
                    .px_3()
                    .items_center()
                    .justify_between()
                    .bg(theme::panel_soft())
                    .border_t_1()
                    .border_color(theme::border_soft())
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(10.5))
                            .font_family(theme::mono())
                            .text_color(if self.debugger.faulted {
                                theme::danger()
                            } else if self.debugger.halted {
                                theme::active()
                            } else if self.running {
                                theme::accent()
                            } else {
                                theme::muted()
                            })
                            .child(status),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_family(theme::mono())
                            .text_color(theme::faint())
                            .child(clip(&ruler, cols / 2)),
                    ),
            )
            .into_any_element()
    }
}

impl Render for MasseyApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            window.focus(&self.focus, cx);
        }
        let viewport = window.viewport_size();
        let width = f32::from(viewport.width);
        let height = f32::from(viewport.height);
        let main_h = (height - theme::TITLE_H - theme::TOOLBAR_H - theme::FOOTER_H).max(240.0);
        let program_w = if width < 1000.0 {
            (width * 0.42).clamp(340.0, 740.0)
        } else if width > 1600.0 {
            (width * 0.38).clamp(620.0, 880.0)
        } else {
            (width * 0.47).clamp(460.0, 780.0)
        };
        let editor_text = if width > 1600.0 {
            15.0
        } else if width > 1000.0 {
            13.5
        } else {
            12.5
        };
        let right_w = (width - program_w).max(420.0);
        let content_w = (right_w - 32.0).max(300.0);
        let cpu_h = (main_h * 0.31).clamp(190.0, 280.0);
        let mem_h = (main_h
            - 8.0
            - if self.debugger.show_help {
                0.0
            } else {
                cpu_h + 24.0
            })
        .max(120.0);
        let mem_w = content_w;
        let inner = (mem_w - 28.0 - 24.0).max(60.0);
        let cell_for = |cols: usize| (inner - (cols.saturating_sub(1)) as f32 * 2.0) / cols as f32;
        let mem_cols: usize = if cell_for(16) >= 18.0 {
            16
        } else if cell_for(12) >= 18.0 {
            12
        } else if cell_for(8) >= 20.0 {
            8
        } else {
            6
        };
        let cell_width = cell_for(mem_cols).clamp(16.0, 64.0);
        self.debugger.set_viewport(16, mem_cols);
        if !self.debugger.show_help
            && mem_cols < 16
            && !self.debugger.halted
            && !self.debugger.faulted
        {
            if let Some(bus) = cpu_pane::live_bus(&self.debugger) {
                let column = bus.address as usize % 16;
                let first = self.debugger.mem_col0;
                if column < first || column >= first + mem_cols {
                    self.debugger.mem_col0 = column
                        .saturating_sub(mem_cols / 2)
                        .min(16usize.saturating_sub(mem_cols));
                }
            }
        }

        if !self.debugger.show_help && self.editor.is_none() {
            let text = self.text_for_mode(self.mode);
            let editor = cx.new(|cx| {
                EditorState::new(window, cx)
                    .default_value(text)
                    .line_number(false)
            });
            let subscription = cx.subscribe_in(
                &editor,
                window,
                |this, editor, event: &InputEvent, window, cx| {
                    this.on_editor_event(editor, event, window, cx);
                },
            );
            let observe = cx.observe(&editor, |this, _editor, cx| {
                // The gutter follows the editor's scroll offset; rebuild the
                // decorations on the next frame as well.
                this.decorated = None;
                cx.notify();
            });
            let syntax = editor.update(cx, |state, cx| {
                state.create_decorations_collection(Vec::new(), cx)
            });
            let current_line = editor.update(cx, |state, cx| {
                state.create_range_decorations_collection(Vec::new(), cx)
            });
            self.editor = Some(editor.clone());
            self.editor_sub = Some(subscription);
            self.editor_observe = Some(observe);
            self.syntax = Some(syntax);
            self.current_line = Some(current_line);
            let text = editor.read(cx).text().to_string();
            self.validate_text(&text, &editor, cx);
        }
        let editor = self.editor.clone();
        if let Some(editor) = &editor {
            let key = (self.debugger.current_row(), self.debugger.machine.pc);
            if self.decorated != Some(key) {
                self.refresh_decorations(editor, cx);
            }
        }

        let title = if self.debugger.show_help {
            "HELP  press any key to close".to_string()
        } else if self.mode == SourceMode::Assembly {
            format!(
                "ASSEMBLER  {} lines  gutter = address",
                self.debugger.image.len()
            )
        } else {
            format!(
                "MACHINE CODE  {} words  gutter = address",
                self.debugger.image.len()
            )
        };
        let error_count = self.invalid.len();

        v_flex()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::text())
            .track_focus(&self.focus)
            .key_context("MasseyCode")
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                TitleBar::new()
                    .bg(theme::title_bg())
                    .border_color(theme::border())
                    .child(self.title_bar()),
            )
            .child(self.debug_bar(cx))
            .child(
                h_flex()
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .when(self.debugger.show_help, |this| {
                        this.child(
                            v_flex()
                                .w_full()
                                .h_full()
                                .min_h_0()
                                .bg(theme::bg())
                                .child(crate::help::help_pane(width, main_h)),
                        )
                    })
                    .when(!self.debugger.show_help, |this| {
                        this.child(
                            v_flex()
                                .w(px(program_w))
                                .h_full()
                                .flex_none()
                                .bg(theme::panel())
                                .border_r_1()
                                .border_color(theme::border())
                                .child(
                                    h_flex()
                                        .h(px(theme::HEADER_H))
                                        .px_3()
                                        .items_center()
                                        .justify_between()
                                        .bg(theme::panel_alt())
                                        .border_b_1()
                                        .border_color(theme::border())
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(theme::muted())
                                                .child(title),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(9.5))
                                                .font_family(theme::mono())
                                                .text_color(if error_count > 0 {
                                                    theme::danger()
                                                } else {
                                                    theme::faint()
                                                })
                                                .child(if error_count > 0 {
                                                    format!("{error_count} error(s)")
                                                } else {
                                                    "valid".to_string()
                                                }),
                                        ),
                                )
                                .child(match &editor {
                                    Some(editor) => h_flex()
                                        .w_full()
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_hidden()
                                        .child(self.gutter(editor, main_h - theme::HEADER_H, cx))
                                        .child(
                                            div().flex_1().min_w_0().h_full().child(
                                                Editor::new(editor)
                                                    .bordered(false)
                                                    .text_size(px(editor_text))
                                                    .h_full(),
                                            ),
                                        )
                                        .into_any_element(),
                                    None => div().into_any_element(),
                                }),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .h_full()
                                .min_w_0()
                                .bg(theme::bg())
                                .px_2()
                                .pt_2()
                                .pb_2()
                                .child(
                                    h_flex()
                                        .w_full()
                                        .h(px(cpu_h))
                                        .flex_none()
                                        .items_stretch()
                                        .child(cpu_pane::cpu_block(
                                            &self.debugger,
                                            content_w,
                                            cpu_h,
                                            self.running,
                                        )),
                                )
                                .child(cpu_pane::bus_row(&self.debugger, content_w))
                                .child(self.memory(16, mem_cols, cell_width, content_w, mem_h)),
                        )
                    }),
            )
            .child(self.footer(width as usize))
    }
}

fn shorten_path(path: &str, limit: usize) -> String {
    if path.chars().count() <= limit {
        path.to_string()
    } else {
        let tail: String = path
            .chars()
            .rev()
            .take(limit.saturating_sub(1))
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("<{tail}")
    }
}

fn clip(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}
