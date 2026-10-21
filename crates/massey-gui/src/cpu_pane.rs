//! The structure diagram: one CPU card on the top-right, the memory bus below
//! it, and the 16x16 memory across the bottom-right.
//!
//! The CPU card has two sections on one surface: the datapath (`CU`, `ALU`,
//! `PC`, `IR`, and the memory-facing `MAR`/`MDR`) and the register file, split
//! by a hairline.  All six datapath blocks are the same size, and every gap
//! between blocks is the same width.
//!
//! Components named by the current transfer light up; nothing else moves.

use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use massey_core::{AluRecord, BusDirection, BusTransfer, Instruction, Opcode, Phase};

use std::time::Duration;

use crate::debugger::{CpuField, Debugger};
use crate::theme;

/// Which components the current transfer text mentions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Active {
    pub pc: bool,
    pub mar: bool,
    pub memory: bool,
    pub mdr: bool,
    pub ir: bool,
    pub registers: bool,
    pub alu: bool,
    pub pc_plus_one: bool,
    /// The register named by the transfer, if any.
    pub register: Option<String>,
    /// A transfer of the current instruction is in flight, so the control unit
    /// is driving the fetch or the execute cycle.
    pub instruction: bool,
    /// Memory bus traffic caused by this transfer, from the machine itself.
    /// The text can only hint at it (`M[...]`), this is the real record.
    pub bus: Option<BusTransfer>,
}

impl Active {
    /// True when any component is lit.
    #[cfg(test)]
    pub fn any_node(&self) -> bool {
        self.pc || self.mar || self.memory || self.mdr || self.ir || self.registers || self.alu
    }

    fn any_cpu(&self) -> bool {
        self.instruction || self.pc || self.mar || self.mdr || self.ir || self.registers || self.alu
    }
}

/// Map a transfer line such as `[fetch 2/4]  M[MAR] -> MDR` to the parts it uses.
pub fn active(text: &str) -> Active {
    let registers = registers_in(text);
    let pc_plus_one = text.contains("PC + 1");
    let alu = !pc_plus_one
        && (text.contains(" + ")
            || text.contains('~')
            || text.contains(" >> ")
            || text.contains(" << ")
            || text.contains(" AND ")
            || text.contains(" OR ")
            || text.contains(" XOR "));
    Active {
        // A jump that is not taken names PC as the transfer it would have made,
        // but PC does not move; only the comparison happened.
        pc: text.contains("PC") && !text.contains("(not taken)"),
        mar: text.contains("MAR"),
        memory: text.contains("M["),
        mdr: text.contains("MDR"),
        ir: text.contains("IR"),
        registers: !registers.is_empty(),
        alu,
        pc_plus_one,
        register: register_label(text, &registers),
        instruction: false,
        bus: None,
    }
}

/// Every `R0`..`RF` mentioned in the line, in order, without duplicates.
fn registers_in(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found: Vec<String> = Vec::new();
    let mut index = 0;
    while index + 1 < bytes.len() {
        let letter = bytes[index] == b'R';
        let digit = (bytes[index + 1] as char).is_ascii_hexdigit();
        let starts_word = index == 0 || !(bytes[index - 1] as char).is_ascii_alphanumeric();
        if letter && digit && starts_word {
            let name = format!("R{}", (bytes[index + 1] as char).to_ascii_uppercase());
            if !found.contains(&name) {
                found.push(name);
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    found
}

/// Prefer the destination register (`MDR -> R5`), otherwise the first one.
fn register_label(text: &str, registers: &[String]) -> Option<String> {
    let first = registers.first()?;
    if let Some((_, after)) = text.rsplit_once("->") {
        if let Some(register) = registers.iter().find(|name| after.contains(name.as_str())) {
            return Some(register.clone());
        }
    }
    Some(first.clone())
}

fn live_active(debugger: &Debugger) -> Active {
    if debugger.halted || debugger.faulted {
        return Active::default();
    }
    let Some(text) = debugger.micro_text() else {
        return Active::default();
    };
    let mut active = active(text);
    if let Some(op) = debugger.last_op() {
        // The control unit drives every fetch and execute transfer; it is not
        // only active when it writes IR.
        active.instruction = op.phase != Phase::Halt;
        active.bus = op.bus;
    }
    active
}

/// The bus traffic of the last transfer, suppressed once the machine is stopped
/// so the row does not stay lit after a halt.
pub fn live_bus(debugger: &Debugger) -> Option<BusTransfer> {
    if debugger.halted || debugger.faulted {
        None
    } else {
        debugger.bus()
    }
}

fn mode_text(debugger: &Debugger) -> (&'static str, Hsla) {
    if debugger.faulted {
        ("faulted", theme::danger())
    } else if debugger.halted {
        ("halted", theme::active())
    } else if debugger.step_count() == 0 {
        ("idle", theme::faint())
    } else {
        match debugger.micro_text() {
            Some(text) if text.starts_with("[fetch") => ("fetch", theme::accent()),
            Some(text) if text.starts_with("[exec") => ("execute", theme::accent()),
            Some(_) => ("transfer", theme::accent()),
            None => ("idle", theme::faint()),
        }
    }
}

/// Inner padding of the CPU card.
const CARD_PAD: f32 = 8.0;
/// Thickness of the CPU card border; it eats into the box like the padding.
const CARD_BORDER: f32 = 1.0;
/// Gap between the datapath section and the register section.
const CARD_GAP: f32 = 16.0;
/// Height of the CPU header.
const HEADER_H: f32 = 18.0;
/// The one gap used between every pair of datapath blocks, horizontal and
/// vertical, with or without a connector in it.
const BLOCK_GAP: f32 = 16.0;
/// Gap between register cells.
const CELL_GAP: f32 = 4.0;
/// The whole CPU card: datapath on the left, register file on the right.
///
/// The two inner cards fill the width instead of sitting as a small centred
/// island: wide windows get wider tiles and an 8x2 register grid, narrow
/// windows get two-column tiles and a 4x4 grid.
pub fn cpu_block(debugger: &Debugger, width: f32, height: f32, running: bool) -> AnyElement {
    let dense = width < 560.0;
    let active = live_active(debugger);
    let (mode, mode_color) = mode_text(debugger);
    let reg_w = if dense {
        (width * 0.42).clamp(168.0, 240.0)
    } else {
        (width * 0.40).clamp(250.0, 500.0)
    };
    let body_h = (height - 2.0 * (CARD_PAD + CARD_BORDER) - HEADER_H - 6.0).max(60.0);
    let step = debugger.step_count();

    let dot_color = if active.any_cpu() {
        theme::accent()
    } else {
        theme::faint().opacity(0.8)
    };
    let dot = div()
        .w(px(6.0))
        .h(px(6.0))
        .flex_none()
        .rounded(px(3.0))
        .bg(dot_color);
    let dot: AnyElement = if running {
        dot.with_animation(
            ElementId::named_usize("cpu-running-pulse", 0),
            Animation::new(Duration::from_millis(700)).repeat(),
            |this, t| {
                let pulse = 0.45 + 0.55 * (1.0 - (2.0 * t - 1.0).abs());
                this.opacity(pulse)
            },
        )
        .into_any_element()
    } else {
        dot.into_any_element()
    };

    let title = h_flex().items_center().gap(px(6.0)).child(dot).child(
        div()
            .text_size(px(9.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if active.any_cpu() {
                theme::text()
            } else {
                theme::muted()
            })
            .child("CPU"),
    );
    let state = h_flex()
        .items_center()
        .gap(px(5.0))
        .child(
            div()
                .w(px(5.0))
                .h(px(5.0))
                .flex_none()
                .rounded(px(3.0))
                .bg(mode_color),
        )
        .child(
            div()
                .text_size(px(8.0))
                .font_family(theme::mono())
                .text_color(mode_color)
                .child(mode),
        );
    let header = h_flex()
        .w_full()
        .h(px(HEADER_H))
        .flex_none()
        .items_center()
        .justify_between()
        .child(title)
        .child(state);

    h_flex()
        .w_full()
        .max_w(px(width))
        .h_full()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(8.0))
        .border_1()
        .border_color(theme::border_soft())
        .bg(theme::panel_soft())
        .p(px(CARD_PAD))
        .child(
            v_flex()
                .w_full()
                .h_full()
                .min_h_0()
                .gap(px(6.0))
                .child(header)
                .child(
                    h_flex()
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .overflow_hidden()
                        .items_stretch()
                        .child(datapath(debugger, &active, dense, step, body_h))
                        .child(
                            div()
                                .w(px(1.0))
                                .h_full()
                                .flex_none()
                                .mx(px((CARD_GAP - 1.0) * 0.5))
                                .bg(theme::border_soft()),
                        )
                        .child(register_file(debugger, reg_w, dense, &active, body_h, step)),
                ),
        )
        .into_any_element()
}

/// The ALU annotation: operands and the value written back, e.g. `0005+0006=000B`.
fn alu_equation(record: &AluRecord) -> String {
    let left = record.left;
    let result = record.result;
    match record.operation.as_str() {
        "ADD" => format!("{left:04X}+{:04X}={result:04X}", record.right.unwrap_or(0)),
        "AND" => format!("{left:04X}&{:04X}={result:04X}", record.right.unwrap_or(0)),
        "OR" => format!("{left:04X}|{:04X}={result:04X}", record.right.unwrap_or(0)),
        "XOR" => format!("{left:04X}^{:04X}={result:04X}", record.right.unwrap_or(0)),
        "SHL" => format!(
            "{left:04X}<<{:02X}={result:04X}",
            record.right.unwrap_or(0) & 0xFF
        ),
        "SHR" => format!(
            "{left:04X}>>{:02X}={result:04X}",
            record.right.unwrap_or(0) & 0xFF
        ),
        "NEGATE" => format!("-{left:04X}={result:04X}"),
        _ => format!("{left:04X}?={result:04X}"),
    }
}

/// The mnemonic the control unit would decode from the current instruction.
fn current_mnemonic(debugger: &Debugger) -> String {
    let address = debugger.machine.instruction_address as usize;
    let word = debugger.machine.memory.get(address).copied().unwrap_or(0);
    match Instruction::from_word(word).opcode() {
        Some(op) => match op.mnemonic() {
            "" => match op {
                Opcode::Add => "ADD",
                Opcode::And => "AND",
                Opcode::Or => "OR",
                Opcode::Xor => "XOR",
                _ => "-",
            }
            .to_string(),
            mnemonic => mnemonic.to_string(),
        },
        None => "-".to_string(),
    }
}

/// The datapath card: `CU`/`ALU`, `PC`/`IR`, and the memory-facing
/// `MAR`/`MDR` at the bottom, joined by short wires.
fn datapath(
    debugger: &Debugger,
    active: &Active,
    dense: bool,
    step: usize,
    body_h: f32,
) -> AnyElement {
    // The three rows share the card height, so a short window makes them
    // shorter than the label and the value would like.  Everything inside a
    // tile is sized from this number, which is what keeps the bottom padding.
    let row_h = ((body_h - 2.0 * BLOCK_GAP) / 3.0).max(1.0);
    let style = TileStyle::for_row(row_h, dense);
    let cu = unit(
        "CU",
        current_mnemonic(debugger),
        // The control unit is busy for every transfer of the current
        // instruction, not only for the one that writes IR.
        active.instruction,
        false,
        false,
        step,
        style,
    );
    let alu = unit(
        "ALU",
        if active.alu {
            debugger
                .machine
                .last_alu
                .as_ref()
                .map(alu_equation)
                .unwrap_or_else(|| "idle".to_string())
        } else {
            "idle".to_string()
        },
        active.alu,
        false,
        false,
        step,
        style,
    );
    let pc = unit(
        "PC",
        format!("{:02X}", debugger.machine.pc),
        active.pc,
        active.pc_plus_one,
        debugger.field_changed(CpuField::Pc),
        step,
        style,
    );
    let ir = unit(
        "IR",
        format!("{:04X}", debugger.machine.ir),
        active.ir,
        false,
        debugger.field_changed(CpuField::Ir),
        step,
        style,
    );
    let mar = unit(
        "MAR",
        format!("{:02X}", debugger.machine.mar),
        active.mar,
        false,
        debugger.field_changed(CpuField::Mar),
        step,
        style,
    );
    let mdr = unit(
        "MDR",
        format!("{:04X}", debugger.machine.mdr),
        active.mdr,
        false,
        debugger.field_changed(CpuField::Mdr),
        step,
        style,
    );

    // Rows share the available height equally (flex grow), so no pixel
    // rounding or extra padding can push the bottom row out of the card; the
    // only fixed parts are the two BLOCK_GAP spacers.
    let gutter_w = BLOCK_GAP;
    let grid_row = |left: AnyElement, right: AnyElement, mid: AnyElement| {
        h_flex()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .min_w_0()
            .items_stretch()
            .child(left)
            .child(
                div()
                    .w(px(gutter_w))
                    .h_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mid),
            )
            .child(right)
            .into_any_element()
    };
    let empty_gutter = div().into_any_element();
    let vertical_arrows = h_flex()
        .w_full()
        .h(px(BLOCK_GAP))
        .flex_none()
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .justify_center()
                .child(v_connector(true, active.pc && active.mar, dense)),
        )
        .child(div().w(px(gutter_w)).flex_none())
        .child(
            div()
                .flex_1()
                .h_full()
                .flex()
                .justify_center()
                .child(v_connector(false, active.ir && active.mdr, dense)),
        );

    v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(
            h_flex()
                .w_full()
                .h(px(HEADER_H))
                .flex_none()
                .items_center()
                .child(
                    div()
                        .text_size(px(8.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted())
                        .child("DATA PATH"),
                ),
        )
        .child(
            v_flex()
                .w_full()
                .flex_1()
                .min_h_0()
                .child(grid_row(cu, alu, h_connector(active.alu, dense)))
                .child(div().w_full().h(px(BLOCK_GAP)).flex_none())
                .child(grid_row(pc, ir, empty_gutter))
                .child(vertical_arrows)
                .child(grid_row(mar, mdr, div().into_any_element())),
        )
        .into_any_element()
}

/// The register file card inside the CPU border.
///
/// Always a 4x4 bank; the cells shrink or grow with the card so the grid stays
/// legible at every window size.  The register named by the transfer is the
/// yellow "acting" register; values changed by the last transfer are green.
fn register_file(
    debugger: &Debugger,
    width: f32,
    _dense: bool,
    active: &Active,
    body_h: f32,
    step: usize,
) -> AnyElement {
    let columns: usize = 4;
    let rows: usize = 4;
    let inner_h = (body_h - HEADER_H - (rows.saturating_sub(1)) as f32 * CELL_GAP).max(16.0);
    let cell_h = inner_h / rows as f32;
    let cell_w = (width - (columns.saturating_sub(1)) as f32 * CELL_GAP) / columns as f32;
    let inline = cell_h < 26.0 && cell_w >= 44.0;
    // The label and the value are one centred group inside the cell, so both
    // sizes are chosen to fit the cell height with padding above and below it.
    let (label_size, pad_y, value_size): (f32, f32, f32) = if inline {
        (7.0, 2.0, 11.0)
    } else if cell_h >= 46.0 {
        (8.5, 4.0, 16.0)
    } else if cell_h >= 36.0 {
        (8.0, 3.0, 13.0)
    } else if cell_h >= 28.0 {
        (7.0, 2.0, 11.0)
    } else if cell_h >= 26.0 {
        (6.5, 1.5, 10.0)
    } else {
        (6.0, 0.5, 8.5)
    };
    let label_h = (label_size * 1.2).ceil();
    let value_h = (value_size * 1.2).ceil();
    let acting = active
        .register
        .as_deref()
        .and_then(|name| name.strip_prefix('R'))
        .and_then(|hex| u8::from_str_radix(hex, 16).ok());

    let mut grid_rows = Vec::new();
    for row in 0..rows {
        let mut cells = Vec::new();
        for column in 0..columns {
            let register = row * columns + column;
            let is_acting = acting == Some(register as u8);
            let changed = debugger.highlight_from.as_ref().is_some_and(|previous| {
                previous.registers[register] != debugger.machine.registers[register]
            });
            // One register gets one colour at a time: yellow while it is the
            // register the current transfer touches, green when it only changed
            // in the last step, neutral otherwise.  Mixing the two made the
            // acting cell look broken (green value, yellow label, olive fill).
            let highlight = if is_acting {
                Some((theme::active(), 0.55, 0.10))
            } else if changed {
                Some((theme::changed(), 0.45, 0.05))
            } else {
                None
            };
            let value_color = highlight.map_or(theme::text(), |(color, _, _)| color);
            let label_color = highlight.map_or(theme::faint(), |(color, _, _)| color.opacity(0.9));
            let label = div()
                .flex_none()
                .text_size(px(label_size))
                .line_height(px(label_h))
                .font_family(theme::mono())
                .text_color(label_color)
                .child(format!("R{register:02X}"));
            let value = div()
                .flex_none()
                .text_size(px(value_size))
                .line_height(px(value_h))
                .font_family(theme::mono())
                .text_color(value_color)
                .child(format!("{:04X}", debugger.machine.registers[register]));
            let content = if inline {
                h_flex()
                    .w_full()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .child(label)
                    .child(value)
                    .into_any_element()
            } else {
                v_flex()
                    .w_full()
                    .h_full()
                    .justify_center()
                    .gap(px(1.0))
                    .px(px(4.0))
                    .py(px(pad_y))
                    .child(h_flex().w_full().items_center().child(label))
                    .child(
                        // `flex_none` so the label and the value stay one
                        // centred group: the cell keeps equal padding at the
                        // top and at the bottom instead of pushing the value
                        // against the lower border in a short cell.
                        v_flex()
                            .w_full()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .child(value),
                    )
                    .into_any_element()
            };
            let (border, background) = match highlight {
                Some((color, border_alpha, fill_alpha)) => {
                    (color.opacity(border_alpha), color.opacity(fill_alpha))
                }
                None => (theme::border_soft(), theme::panel()),
            };
            let cell = v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(border)
                .bg(background)
                .overflow_hidden()
                .child(content);
            cells.push(match highlight {
                Some((color, border_alpha, _)) if changed => cell
                    .with_animation(
                        ElementId::named_usize(format!("register-{register}"), step),
                        Animation::new(Duration::from_millis(240))
                            .with_easing(|t| 1.0 - (1.0 - t) * (1.0 - t)),
                        move |this, t| {
                            this.border_color(
                                color.opacity(border_alpha + (0.85 - border_alpha) * (1.0 - t)),
                            )
                        },
                    )
                    .into_any_element(),
                _ => cell.into_any_element(),
            });
        }
        grid_rows.push(
            h_flex()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .min_w_0()
                .items_stretch()
                .gap(px(CELL_GAP))
                .children(cells)
                .into_any_element(),
        );
    }

    v_flex()
        .w(px(width))
        .h_full()
        .flex_none()
        .min_h_0()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .h(px(HEADER_H))
                .flex_none()
                .items_center()
                .child(
                    div()
                        .text_size(px(8.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::muted())
                        .child("REGISTER FILE"),
                ),
        )
        .child(
            v_flex()
                .w_full()
                .flex_1()
                .min_h_0()
                .justify_center()
                .gap(px(CELL_GAP))
                .children(grid_rows),
        )
        .into_any_element()
}

/// The horizontal address/data bus between the CPU card and memory.
///
/// It lights only when the transfer really touches memory, as recorded by the
/// machine's bus transfer, and the data label follows the direction of that
/// traffic: reads come up into MDR, writes go down into memory.
pub fn bus_row(debugger: &Debugger, width: f32) -> AnyElement {
    let bus = live_bus(debugger);
    let lit = bus.is_some();
    let color = if lit { theme::accent() } else { theme::faint() };
    let label = |text: String| {
        div()
            .flex_none()
            .text_size(px(8.5))
            .font_family(theme::mono())
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    let address = match bus {
        Some(bus) => format!("MAR {:02X} \u{2192} addr \u{2193}", bus.address),
        None => "MAR \u{2192} addr \u{2193}".to_string(),
    };
    let data = match bus.map(|bus| bus.direction) {
        Some(BusDirection::Write) => "MDR \u{2192} data \u{2193}".to_string(),
        _ => "data \u{2191} \u{2192} MDR".to_string(),
    };
    h_flex()
        .w(px(width))
        .h(px(26.0))
        .flex_none()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .bg(theme::panel_alt())
        .border_t_1()
        .border_b_1()
        .border_color(if lit {
            theme::accent().opacity(0.35)
        } else {
            theme::border_soft()
        })
        .child(
            div()
                .w(px(4.0))
                .h(px(4.0))
                .flex_none()
                .rounded(px(2.0))
                .bg(if lit {
                    theme::accent()
                } else {
                    theme::faint().opacity(0.7)
                }),
        )
        .child(label(address))
        .child(div().flex_1().h(px(1.0)).bg(if lit {
            theme::accent().opacity(0.55)
        } else {
            theme::border_soft()
        }))
        .child(label(data))
        .child(
            div()
                .w(px(4.0))
                .h(px(4.0))
                .flex_none()
                .rounded(px(2.0))
                .bg(if lit {
                    theme::accent()
                } else {
                    theme::faint().opacity(0.7)
                }),
        )
        .into_any_element()
}

/// Type scale for one datapath tile, chosen from the height the tile got.
///
/// Sizing from the row height rather than from the window width is what keeps
/// the label and the value inside the tile, with padding above and below, when
/// the window is short.
#[derive(Clone, Copy)]
struct TileStyle {
    label_size: f32,
    label_h: f32,
    pad_x: f32,
    pad_y: f32,
    value_size: f32,
    value_size_long: f32,
}

impl TileStyle {
    fn for_row(row_h: f32, dense: bool) -> Self {
        let (label_size, label_h, pad_y, value_size, value_size_long): (f32, f32, f32, f32, f32) =
            if row_h >= 48.0 {
                (8.0, 10.0, 6.0, 16.0, 11.5)
            } else if row_h >= 36.0 {
                (8.0, 9.0, 4.0, 14.0, 11.5)
            } else if row_h >= 27.0 {
                (7.0, 8.0, 2.5, 11.0, 9.5)
            } else if row_h >= 20.0 {
                (6.5, 7.0, 1.5, 9.5, 8.5)
            } else {
                (6.0, 6.0, 0.5, 8.5, 7.5)
            };
        // A narrow card keeps the type small even when there is height to
        // spare, so a long value cannot overflow the tile horizontally.
        let cap = if dense { 11.5 } else { f32::INFINITY };
        TileStyle {
            label_size: if dense {
                label_size.min(7.5)
            } else {
                label_size
            },
            label_h,
            pad_x: if dense { 6.0 } else { 9.0 },
            pad_y,
            value_size: value_size.min(cap),
            value_size_long: value_size_long.min(cap),
        }
    }

    /// Value size for a string that has to fit in the tile width.
    fn value_size(self, characters: usize) -> f32 {
        if characters > 9 {
            self.value_size_long
        } else {
            self.value_size
        }
    }
}

fn unit(
    name: &'static str,
    value: String,
    lit: bool,
    plus_one: bool,
    changed: bool,
    step: usize,
    style: TileStyle,
) -> AnyElement {
    let border = if lit {
        theme::accent().opacity(0.8)
    } else if changed {
        theme::changed().opacity(0.55)
    } else {
        theme::border_soft()
    };
    let background = if lit {
        theme::accent().opacity(0.08)
    } else {
        theme::panel()
    };
    let value_color = if lit {
        theme::accent()
    } else if changed {
        theme::changed()
    } else {
        theme::text()
    };
    let label_color = if lit {
        theme::accent()
    } else if changed {
        theme::changed().opacity(0.85)
    } else {
        theme::faint()
    };
    // The label and the value are one centred group inside the tile: the type
    // scale is chosen so that label + value + padding always fits the row.
    let value_size = style.value_size(value.chars().count());
    let label_size = style.label_size;
    let label_h = style.label_h;
    let value_h = (value_size * 1.25).ceil();
    let header = h_flex()
        .w_full()
        .h(px(label_h))
        .flex_none()
        .items_center()
        .px(px(style.pad_x))
        .child(
            div()
                .text_size(px(label_size))
                .line_height(px(label_h))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(label_color)
                .child(name),
        );
    let value_row = h_flex()
        .w_full()
        .h(px(value_h))
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(value_size))
                .line_height(px(value_h))
                .font_family(theme::mono())
                .text_color(value_color)
                .child(value),
        )
        .when(plus_one, |this| {
            this.child(
                div()
                    .px(px(3.0))
                    .rounded(px(3.0))
                    .bg(theme::accent().opacity(0.16))
                    .text_size(px(7.0))
                    .font_family(theme::mono())
                    .text_color(theme::accent())
                    .child("+1"),
            )
        });
    let tile = v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .justify_center()
        .py(px(style.pad_y))
        .rounded(px(4.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .overflow_hidden()
        .child(header)
        .child(value_row);
    if changed && !lit {
        tile.with_animation(
            ElementId::named_usize(name, step),
            Animation::new(Duration::from_millis(260)).with_easing(|t| 1.0 - (1.0 - t) * (1.0 - t)),
            |this, t| this.border_color(theme::changed().opacity(0.25 + 0.5 * (1.0 - t))),
        )
        .into_any_element()
    } else {
        tile.into_any_element()
    }
}

/// Solid heads for the connectors.  They are drawn separately from the shaft
/// so the shaft can be sized to fill the connector exactly: line touches the
/// source block, head touches the destination block, nothing floats or breaks.
const HEAD_RIGHT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><path d="M1 1l6 3-6 3z" fill="currentColor"/></svg>"#;
const HEAD_DOWN: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><path d="M1 1h6l-3 6z" fill="currentColor"/></svg>"#;
const HEAD_UP: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><path d="M1 7h6l-3-6z" fill="currentColor"/></svg>"#;

fn h_connector(lit: bool, _dense: bool) -> AnyElement {
    let color = if lit {
        theme::accent()
    } else {
        theme::muted().opacity(0.8)
    };
    h_flex()
        .w_full()
        .h_full()
        .items_center()
        .child(div().h(px(1.0)).flex_1().bg(color))
        .child(
            Icon::default()
                .data(HEAD_RIGHT)
                .text_color(color)
                .w(px(7.0))
                .h(px(7.0)),
        )
        .into_any_element()
}

fn v_connector(down: bool, lit: bool, _dense: bool) -> AnyElement {
    let color = if lit {
        theme::accent()
    } else {
        theme::muted().opacity(0.8)
    };
    let line = div().w(px(1.0)).h(px((BLOCK_GAP - 7.0).max(4.0))).bg(color);
    let head = Icon::default()
        .data(if down { HEAD_DOWN } else { HEAD_UP })
        .text_color(color)
        .w(px(7.0))
        .h(px(7.0));
    let mut column = v_flex().h_full().items_center();
    if down {
        column = column.child(line).child(head);
    } else {
        column = column.child(head).child(line);
    }
    column.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{active, alu_equation, live_active, live_bus};
    use crate::debugger::Debugger;
    use massey_core::{AluRecord, BusDirection};

    #[test]
    fn alu_annotations_show_operands_and_result() {
        let add = AluRecord {
            operation: "ADD".to_string(),
            left: 0x001B,
            right: Some(0x0008),
            result: 0x0023,
        };
        assert_eq!(alu_equation(&add), "001B+0008=0023");

        let shift = AluRecord {
            operation: "SHR".to_string(),
            left: 0x0023,
            right: Some(0x0004),
            result: 0x0002,
        };
        assert_eq!(alu_equation(&shift), "0023>>04=0002");

        let negate = AluRecord {
            operation: "NEGATE".to_string(),
            left: 0x0008,
            right: None,
            result: 0xFFF8,
        };
        assert_eq!(alu_equation(&negate), "-0008=FFF8");
    }

    #[test]
    fn fetch_transfers_light_the_right_nodes() {
        let fetch = active("[fetch 1/4]  PC -> MAR");
        assert!(fetch.pc && fetch.mar && !fetch.memory && !fetch.mdr && !fetch.ir);
        assert!(!fetch.alu);

        let read = active("[fetch 2/4]  M[MAR] -> MDR");
        assert!(read.mar && read.memory && read.mdr && !read.pc && !read.ir);

        let decoded = active("[fetch 3/4]  MDR -> IR");
        assert!(decoded.mdr && decoded.ir && !decoded.memory);

        let bump = active("[fetch 4/4]  PC + 1 -> PC");
        assert!(bump.pc && bump.pc_plus_one && !bump.mar);
        assert!(!bump.alu);
    }

    #[test]
    fn register_transfers_name_the_register() {
        let write = active("[exec 3/3]  MDR -> R5");
        assert!(write.mdr && write.registers);
        assert_eq!(write.register.as_deref(), Some("R5"));

        let add = active("R0 = R5 + R6");
        assert!(add.alu && add.registers);
        assert_eq!(add.register.as_deref(), Some("R0"));
    }

    #[test]
    fn every_transfer_names_something() {
        let plan = massey_core::Machine::transfer_plan(0x3508);
        for text in plan {
            assert!(active(&text).any_node(), "{text}");
        }
    }

    #[test]
    fn a_not_taken_jump_does_not_light_pc() {
        let taken = active("[exec 1/1]  if R5 = R0: 08 -> PC");
        assert!(taken.pc);

        let not_taken = active("[exec 1/1]  if R5 = R0: 08 -> PC (not taken)");
        assert!(!not_taken.pc);
        assert_eq!(not_taken.register.as_deref(), Some("R5"));
    }

    #[test]
    fn control_unit_is_lit_for_fetch_and_execute() {
        let mut debugger = Debugger::from_image("test.mc", &[0x1102, 0xE000]);
        assert!(!live_active(&debugger).instruction);

        debugger.step_transfer(); // fetch 1/4, PC -> MAR
        assert!(live_active(&debugger).instruction);

        for _ in 0..4 {
            debugger.step_transfer(); // finish the fetch and execute LOAD R1, #02
        }
        let active = live_active(&debugger);
        assert!(active.instruction);
        assert!(!active.alu, "a LOAD does not use the ALU");
    }

    #[test]
    fn the_bus_lights_only_for_memory_transfers() {
        let mut debugger = Debugger::from_image("test.mc", &[0x1102, 0xE000]);

        debugger.step_transfer(); // PC -> MAR: no bus traffic
        assert!(live_bus(&debugger).is_none());

        debugger.step_transfer(); // M[MAR] -> MDR: a read
        let read = live_bus(&debugger).expect("the fetch reads memory");
        assert_eq!(read.direction, BusDirection::Read);
        assert_eq!(read.address, 0);

        for _ in 0..2 {
            debugger.step_transfer(); // MDR -> IR, PC + 1: internal transfers
        }
        assert!(live_bus(&debugger).is_none());
    }
}
