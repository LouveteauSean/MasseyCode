//! Plain-text frames for `masseycode --dump`.
//!
//! A text frame is the easiest thing to test and the easiest thing to script,
//! so it uses the same marks and the same two-line footer as the window.

use crate::debugger::{CpuField, Debugger, HELP_LINES};

pub fn render(debugger: &Debugger, cols: usize, rows: usize) -> String {
    let cols = cols.max(40);
    let rows = rows.max(12);
    let mut out: Vec<String> = Vec::new();
    let body = rows.saturating_sub(2);

    let two_pane = cols >= 52;
    let (mem_rows, mem_cols) = if two_pane {
        let prog_w = ((cols * 2) / 5).max(24).min(cols.saturating_sub(26));
        let right_w = cols.saturating_sub(prog_w + 1);
        let mem_cols = memory_columns(right_w);
        let per_row = register_columns(right_w);
        let reg_rows = 16_usize.div_ceil(per_row);
        let mem_rows = 16.min(body.saturating_sub(2 + reg_rows + 2));
        let left = program_lines(debugger, body);
        let right = right_lines(debugger, right_w, body);
        let count = body.min(left.len().max(right.len()));
        for index in 0..count {
            let l = left.get(index).map(String::as_str).unwrap_or("");
            let r = right.get(index).map(String::as_str).unwrap_or("");
            out.push(clip(&format!("{l:<prog_w$}| {r}"), cols));
        }
        (mem_rows, mem_cols)
    } else {
        let mem_cols = memory_columns(cols);
        let reg_rows = 16_usize.div_ceil(4);
        let mem_rows = 16.min(body.saturating_sub(reg_rows + 4));
        let lines = stacked_lines(debugger, cols, body);
        out.extend(lines.into_iter().take(body).map(|line| clip(&line, cols)));
        (mem_rows, mem_cols)
    };

    while out.len() < body {
        out.push(String::new());
    }
    let message = footer_message(debugger);
    out.push(clip(&message, cols));
    out.push(clip(&status_line(debugger, cols, mem_rows, mem_cols), cols));
    out.truncate(rows);
    out.join("\n")
}

fn program_lines(debugger: &Debugger, body_rows: usize) -> Vec<String> {
    if debugger.show_help {
        let mut lines = vec!["Help - press any key to close".to_string()];
        lines.extend(
            HELP_LINES
                .iter()
                .take(body_rows.saturating_sub(1))
                .map(|line| (*line).to_string()),
        );
        return lines;
    }
    let loaded = debugger.program_len();
    let mut lines = Vec::new();
    if loaded == 0 {
        let marker = if debugger.current_row().is_none() {
            "*"
        } else {
            " "
        };
        lines.push(format!("{marker}  HEAD"));
        lines.push("   (no program loaded)".to_string());
        return lines;
    }
    lines.push(format!("Program ({loaded} words)"));
    lines.extend(visible_program_rows(debugger, body_rows.saturating_sub(1)));
    lines
}

/// The program window: HEAD, the loaded words, an end marker, centred on the
/// current row.
fn visible_program_rows(debugger: &Debugger, body_rows: usize) -> Vec<String> {
    let loaded = debugger.program_len();
    let total = loaded + 2;
    let window = total.min(body_rows.max(3));
    let current_slot = debugger.current_row().map(|row| row + 1).unwrap_or(0);
    let first = if total > window {
        current_slot.saturating_sub(window / 2).min(total - window)
    } else {
        0
    };
    let pc = debugger.machine.pc as usize;
    let current = debugger.current_row();
    let mut lines = Vec::new();
    for slot in first..first + window {
        if slot == 0 {
            let marker = if current.is_none() { "*" } else { " " };
            lines.push(format!("{marker}  HEAD"));
            continue;
        }
        let row = slot - 1;
        if row == loaded {
            lines.push("   ---- end of program ----".to_string());
            continue;
        }
        let word = debugger.machine.memory[row];
        let marker = if row == pc {
            '>'
        } else if current == Some(row) {
            '*'
        } else {
            ' '
        };
        lines.push(clip(
            &format!(
                "{marker}  {row:02X}  {:04X}  {}",
                word,
                debugger.instruction_text(row)
            ),
            10_000,
        ));
    }
    lines
}

fn right_lines(debugger: &Debugger, width: usize, body_rows: usize) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("Registers".to_string());
    lines.push(cpu_line(debugger, width, false));
    let per_row = register_columns(width);
    for start in (0..16).step_by(per_row) {
        lines.push(register_row(debugger, start, per_row, false));
    }
    lines.push("Memory - 16-bit words".to_string());
    let cols = memory_columns(width);
    let overhead = lines.len() + 1;
    let row_count = 16.min(body_rows.saturating_sub(overhead));
    let row0 = debugger
        .mem_row0
        .min(16usize.saturating_sub(row_count.max(1)));
    let col0 = debugger.mem_col0.min(16usize.saturating_sub(cols));
    lines.push(memory_header(col0, cols));
    for row in 0..row_count {
        lines.push(memory_row(debugger, row0 + row, col0, cols));
    }
    lines
}

fn stacked_lines(debugger: &Debugger, cols: usize, body_rows: usize) -> Vec<String> {
    let mut lines = vec!["Registers".to_string(), cpu_line(debugger, cols, false)];
    for start in (0..16).step_by(4) {
        lines.push(register_row(debugger, start, 4, false));
    }
    if debugger.show_help {
        lines.push("Help - press any key to close".to_string());
        lines.extend(HELP_LINES.iter().map(|line| (*line).to_string()));
    } else {
        lines.push(format!("Program ({} words)", debugger.program_len()));
        lines.extend(visible_program_rows(
            debugger,
            body_rows.saturating_sub(lines.len()),
        ));
    }
    lines.push("Memory - 16-bit words".to_string());
    let cols = memory_columns(cols);
    let row_count = 16.min(body_rows.saturating_sub(lines.len() + 1));
    let row0 = debugger
        .mem_row0
        .min(16usize.saturating_sub(row_count.max(1)));
    let col0 = debugger.mem_col0.min(16usize.saturating_sub(cols));
    lines.push(memory_header(col0, cols));
    for row in 0..row_count {
        lines.push(memory_row(debugger, row0 + row, col0, cols));
    }
    lines
}

fn cpu_line(debugger: &Debugger, _width: usize, ansi_markers: bool) -> String {
    let marker = |field, text: String| {
        let changed = debugger.field_changed(field);
        if ansi_markers && changed {
            format!("{text}*")
        } else {
            text
        }
    };
    format!(
        "{}  {}  {}  {}",
        marker(CpuField::Pc, format!("PC={:02X}", debugger.machine.pc)),
        marker(CpuField::Ir, format!("IR={:04X}", debugger.machine.ir)),
        marker(CpuField::Mar, format!("MAR={:02X}", debugger.machine.mar)),
        marker(CpuField::Mdr, format!("MDR={:04X}", debugger.machine.mdr)),
    )
}

fn register_columns(width: usize) -> usize {
    ((width + 1) / 10).clamp(1, 8)
}

fn register_row(debugger: &Debugger, start: usize, count: usize, _ansi: bool) -> String {
    let active = debugger.active_register();
    let mut line = String::new();
    for register in start..start + count.min(16 - start) {
        if register > start {
            line.push(' ');
        }
        line.push_str(&format!(
            "R{register:02X}={:04X}",
            debugger.machine.registers[register]
        ));
        line.push(if active == Some(register as u8) {
            '*'
        } else {
            ' '
        });
    }
    line
}

fn memory_columns(width: usize) -> usize {
    ((width.saturating_sub(4)) / 5).clamp(1, 16)
}

fn memory_header(col0: usize, cols: usize) -> String {
    let mut line = String::from("    ");
    for column in 0..cols {
        line.push_str(&format!("  {:02X} ", col0 + column));
    }
    clip(&line, 10_000)
}

fn memory_row(debugger: &Debugger, row: usize, col0: usize, cols: usize) -> String {
    let mut line = format!("{row:02X}  ");
    for column in 0..cols {
        let address = row * 16 + col0 + column;
        let value = debugger.machine.memory[address];
        let is_pc = address == debugger.machine.pc as usize;
        let changed = debugger.memory_changed(address as u8);
        line.push_str(&format!("{:04X}", value));
        line.push(if is_pc {
            '>'
        } else if changed {
            '*'
        } else {
            ' '
        });
    }
    clip(&line, 10_000)
}

fn footer_message(debugger: &Debugger) -> String {
    if !debugger.message.is_empty() {
        return debugger.message.clone();
    }
    if debugger.show_help {
        return "any key closes the help".to_string();
    }
    if let Some(text) = debugger.micro_text() {
        return text.to_string();
    }
    "[n]ext transfer [i]nstruction [b]ack [c]ontinue [j/k/h/l]scroll [?]help [q]uit".to_string()
}

fn status_line(debugger: &Debugger, cols: usize, rows: usize, cols_visible: usize) -> String {
    let mut state = format!(
        " {}  step {}  PC={:02X}",
        shorten_path(&debugger.name, cols.saturating_sub(34)),
        debugger.step_count(),
        debugger.machine.pc
    );
    if debugger.halted {
        state.push_str("  [HALTED]");
    }
    if debugger.faulted {
        state.push_str("  [FAULTED]");
    }
    let rows = rows.min(16);
    let cols_visible = cols_visible.min(16);
    let mut ruler = String::new();
    if rows < 16 {
        ruler.push_str(&format!(
            "mem rows {:02X}-{:02X}",
            debugger.mem_row0,
            debugger.mem_row0 + rows - 1
        ));
    }
    if cols_visible < 16 {
        if !ruler.is_empty() {
            ruler.push_str("  ");
        }
        ruler.push_str(&format!(
            "mem cols {:02X}-{:02X}",
            debugger.mem_col0,
            debugger.mem_col0 + cols_visible - 1
        ));
    }
    if debugger.history_trimmed() {
        if !ruler.is_empty() {
            ruler.push_str("  ");
        }
        ruler.push_str("history trimmed");
    }
    let gap = cols.saturating_sub(state.chars().count() + ruler.chars().count() + 1);
    clip(&format!("{state}{}{ruler}", " ".repeat(gap.max(1))), cols)
}

fn shorten_path(path: &str, width: usize) -> String {
    if path.chars().count() <= width {
        return path.to_string();
    }
    let tail: String = path
        .chars()
        .rev()
        .take(width.saturating_sub(1))
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("<{tail}")
}

fn clip(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: [u16; 5] = [0x1102, 0x1203, 0x6012, 0x40FF, 0xE000];

    fn demo() -> Debugger {
        Debugger::from_image("tests/demo.mc", &DEMO)
    }

    #[test]
    fn wide_frame_has_the_marks_and_fits() {
        let mut debugger = demo();
        for _ in 0..3 {
            debugger.step_instruction();
        }
        let frame = render(&debugger, 100, 30);
        let lines: Vec<&str> = frame.lines().collect();
        assert_eq!(lines.len(), 30);
        assert!(lines.iter().all(|line| line.chars().count() <= 100));
        assert!(frame.contains("*  02  6012"));
        assert!(frame.contains(">  03  40FF"));
        assert!(frame.contains("R00=0005*"));
        assert!(frame.contains("mem cols 00-0A"));
        assert!(lines[29].contains("step "));
    }

    #[test]
    fn halt_and_fault_are_reported() {
        let mut debugger = demo();
        for _ in 0..5 {
            debugger.step_instruction();
        }
        let frame = render(&debugger, 100, 30);
        assert!(frame.contains("[HALTED]"));
        assert!(frame.contains("0005"));

        let mut fault = Debugger::from_image("bad.mc", &[0x5000]);
        fault.step_instruction();
        let frame = render(&fault, 100, 30);
        assert!(frame.contains("[FAULTED]"));
        assert!(frame.contains("unknown instruction"));
    }

    #[test]
    fn narrow_frame_never_exceeds_its_width() {
        let debugger = demo();
        let frame = render(&debugger, 60, 20);
        let lines: Vec<&str> = frame.lines().collect();
        assert_eq!(lines.len(), 20);
        assert!(lines.iter().all(|line| line.chars().count() <= 60));
        assert!(frame.contains("R0F=0000"));
        assert!(frame.contains("Program (5 words)"));
        assert!(frame.contains("Memory - 16-bit words"));
    }

    #[test]
    fn empty_program_still_draws() {
        let debugger = Debugger::from_image("empty.mc", &[]);
        let frame = render(&debugger, 80, 24);
        assert!(frame.contains("(no program loaded)"));
        assert!(frame.contains("HEAD"));
    }

    #[test]
    fn help_replaces_the_program_pane() {
        let mut debugger = demo();
        debugger.toggle_help();
        let frame = render(&debugger, 100, 30);
        assert!(frame.contains("Help - press any key to close"));
        assert!(frame.contains("PC -> MAR"));
    }

    #[test]
    fn transfer_stepping_marks_the_status_and_last_transfer() {
        let mut debugger = demo();
        debugger.step_transfer();
        let frame = render(&debugger, 100, 30);
        assert!(frame.contains("[fetch 1/4]  PC -> MAR"));
    }
}
