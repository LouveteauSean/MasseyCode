//! The headless debugger used by both the window and `--dump`.
//!
//! The model is always transfer-granular: one step advances exactly one
//! register transfer (`PC -> MAR`, `M[MAR] -> MDR`, ...), which is what the
//! diagram shows.  A separate helper runs transfers until the current
//! instruction completes.  There is deliberately no editor in this model.

use std::path::Path;

use massey_core::{disassemble, BusTransfer, Machine, MicroOp, Snapshot};

/// How many snapshots the back stack keeps.  One snapshot is a little over
/// 600 bytes, so this is about 6 MB.
const HISTORY_LIMIT: usize = 10_000;
/// Ceiling for a single continue run, so a runaway loop stops instead of
/// hanging the window.
#[allow(dead_code)]
const RUN_LIMIT: u64 = 10_000_000;
/// Every instruction fetches in four register transfers.
const FETCH_TICKS: u8 = 4;
/// Safety net for `step_instruction`: no instruction has more transfers.
const MAX_TRANSFERS_PER_INSTRUCTION: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// The budget ran out; the caller may run another slice.
    Running,
    Halted,
    Faulted,
    AlreadyStopped,
}

/// The debugger state.
pub struct Debugger {
    pub machine: Machine,
    /// The canonical machine-code image the editor edits.
    pub image: Vec<u16>,
    /// The file shown in the status line.
    pub name: String,
    /// Snapshot history; `history.last()` is the current state.
    pub history: Vec<Snapshot>,
    /// The state the last step or back came from, used for the changed markers.
    pub highlight_from: Option<Snapshot>,
    /// One transfer description per history entry; `micro_texts[0]` is empty.
    pub micro_texts: Vec<String>,
    /// The full micro-operation behind each `micro_texts` entry, when there is
    /// one.  The window uses it for the facts the text cannot be trusted to
    /// state exactly: real bus traffic, and whether the transfer belongs to the
    /// instruction's fetch/execute cycle at all.
    pub micro_ops: Vec<Option<MicroOp>>,
    pub show_help: bool,
    pub halted: bool,
    pub faulted: bool,
    pub message: String,
    pub mem_row0: usize,
    pub mem_col0: usize,
    /// Filled in by the view before it handles scrolling.
    pub view_rows: usize,
    pub view_cols: usize,
    pub history_trimmed: bool,
}

impl Debugger {
    /// Load a machine-code file, or the first worked example when there is no
    /// argument.  Only the PDF's machine-code format is accepted: one
    /// four-digit hexadecimal word per line.
    pub fn load(path: Option<&Path>) -> Result<Self, String> {
        let (name, text) = match path {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
                (path.display().to_string(), text)
            }
            None => (
                "01-addition.mc".to_string(),
                include_str!("../../../examples/01-addition.mc").to_string(),
            ),
        };
        let image = parse_image(&text)?;
        Ok(Self::from_image(name, &image))
    }

    /// A debugger over an in-memory program image, used by the tests.
    #[allow(dead_code)]
    pub fn from_image(name: impl Into<String>, image: &[u16]) -> Self {
        let mut machine = Machine::new();
        machine.load_image(image);
        Self::from_machine(name.into(), machine)
    }

    fn from_machine(name: String, machine: Machine) -> Self {
        let image = machine.image().to_vec();
        let initial = machine.snapshot();
        Self {
            machine,
            image,
            name,
            history: vec![initial],
            highlight_from: None,
            micro_texts: vec![String::new()],
            micro_ops: vec![None],
            show_help: false,
            halted: initial.halted,
            faulted: initial.fault.is_some(),
            message: String::new(),
            mem_row0: 0,
            mem_col0: 0,
            view_rows: 16,
            view_cols: 16,
            history_trimmed: false,
        }
    }

    // -- state -------------------------------------------------------------

    /// Put the machine back at the start of the loaded image, keeping the
    /// viewport.
    pub fn reset(&mut self) {
        let image = self.image.clone();
        self.reload(&image);
        self.message = "machine reset".to_string();
    }

    /// Replace the canonical image and reset the machine to its first word.
    pub fn set_image(&mut self, image: &[u16]) {
        self.image = image.to_vec();
        self.reload(&self.image.clone());
        self.message = "program changed".to_string();
    }

    fn reload(&mut self, image: &[u16]) {
        self.machine = Machine::new();
        self.machine.load_image(image);
        let initial = self.machine.snapshot();
        self.history.clear();
        self.history.push(initial);
        self.micro_texts.clear();
        self.micro_texts.push(String::new());
        self.micro_ops.clear();
        self.micro_ops.push(None);
        self.highlight_from = None;
        self.halted = initial.halted;
        self.faulted = initial.fault.is_some();
        self.history_trimmed = false;
    }

    pub fn current(&self) -> &Snapshot {
        self.history
            .last()
            .expect("history always has a current state")
    }

    pub fn step_count(&self) -> usize {
        self.history.len() - 1
    }

    pub fn program_len(&self) -> usize {
        self.machine.image_len()
    }

    /// The instruction the machine is on.  Before anything has run this is
    /// `None` (the HEAD row).  The machine keeps `instruction_address` while it
    /// fetches, executes and even after the instruction retires, so the
    /// highlight follows the instruction instead of the already-incremented PC.
    pub fn current_row(&self) -> Option<usize> {
        if self.history.len() < 2 {
            return None;
        }
        Some(self.machine.instruction_address as usize)
    }

    /// The register the last transfer wrote, if any.
    pub fn active_register(&self) -> Option<u8> {
        let previous = self.highlight_from.as_ref()?;
        (0..16).find(|index| {
            self.machine.registers[*index as usize] != previous.registers[*index as usize]
        })
    }

    pub fn memory_changed(&self, address: u8) -> bool {
        self.highlight_from.as_ref().is_some_and(|previous| {
            previous.memory[address as usize] != self.machine.memory[address as usize]
        })
    }

    pub fn field_changed(&self, field: CpuField) -> bool {
        let Some(previous) = self.highlight_from.as_ref() else {
            return false;
        };
        match field {
            CpuField::Pc => previous.pc != self.machine.pc,
            CpuField::Ir => previous.ir != self.machine.ir,
            CpuField::Mar => previous.mar != self.machine.mar,
            CpuField::Mdr => previous.mdr != self.machine.mdr,
        }
    }

    /// The last transfer as a compact line, e.g. `[fetch 2/4] M[MAR] -> MDR`.
    pub fn micro_text(&self) -> Option<&str> {
        self.micro_texts
            .last()
            .map(String::as_str)
            .filter(|text| !text.is_empty())
    }

    /// The machine record of the last transfer, with its bus traffic and the
    /// instruction phase.  `None` before the first step.
    pub fn last_op(&self) -> Option<&MicroOp> {
        self.micro_ops.last().and_then(Option::as_ref)
    }

    /// The bus traffic of the last transfer, if that transfer used the bus.
    pub fn bus(&self) -> Option<BusTransfer> {
        self.last_op().and_then(|op| op.bus)
    }

    /// A line of text for the instruction at `address`.
    pub fn instruction_text(&self, address: usize) -> String {
        if address >= 256 {
            return String::new();
        }
        let text = disassemble(self.machine.memory[address]);
        if text.starts_with("???") {
            "???".to_string()
        } else {
            text
        }
    }

    // -- stepping ----------------------------------------------------------

    /// Advance exactly one register transfer and remember it for `back`.
    pub fn step_transfer(&mut self) {
        if self.halted || self.faulted {
            return;
        }
        self.message.clear();
        let before = *self.current();
        self.highlight_from = Some(before);
        let op = self.machine.micro_step();
        let text = describe_tick(before.tick, before.execute_ticks, &op.text);
        if op.halts {
            self.halted = true;
        }
        if op.faulted || self.machine.fault.is_some() {
            self.faulted = true;
        }
        if let Some(fault) = self.machine.fault {
            self.message = fault.message();
        }
        self.history.push(self.machine.snapshot());
        self.micro_texts.push(text);
        self.micro_ops.push(Some(op));
        self.trim_history();
    }

    /// Advance transfers until the current instruction has finished, so one
    /// call equals one whole instruction.  Each transfer still lands in the
    /// history, so `back` keeps its transfer granularity.
    pub fn step_instruction(&mut self) {
        if self.halted || self.faulted {
            return;
        }
        self.message.clear();
        for _ in 0..MAX_TRANSFERS_PER_INSTRUCTION {
            self.step_transfer();
            if self.halted || self.faulted || self.machine.tick == 0 {
                return;
            }
        }
    }

    pub fn back(&mut self) {
        if self.history.len() <= 1 {
            self.message = "already at the start".to_string();
            return;
        }
        self.highlight_from = Some(*self.current());
        self.history.pop();
        self.micro_texts.pop();
        self.micro_ops.pop();
        let snapshot = *self.current();
        self.machine.restore(&snapshot);
        self.halted = false;
        self.faulted = false;
        self.message.clear();
    }

    // -- running -----------------------------------------------------------

    /// Run for at most `budget` instructions.  The window drives this from a
    /// timer so continue stays interruptible.
    pub fn run_slice(&mut self, budget: u64) -> RunOutcome {
        if self.halted {
            self.message = "machine already stopped".to_string();
            return RunOutcome::AlreadyStopped;
        }
        if self.faulted {
            self.message = "machine already stopped".to_string();
            return RunOutcome::Faulted;
        }
        let mut executed = 0u64;
        while executed < budget {
            self.step_instruction();
            executed += 1;
            if self.halted {
                self.message = "halted".to_string();
                return RunOutcome::Halted;
            }
            if self.faulted {
                return RunOutcome::Faulted;
            }
        }
        RunOutcome::Running
    }

    /// Run to a halt or the step ceiling.  Used by the tests; the window
    /// drives `run_slice` from a timer so it stays interruptible.
    #[allow(dead_code)]
    pub fn run_to_end(&mut self) {
        if self.halted || self.faulted {
            self.message = "machine already stopped".to_string();
            return;
        }
        let mut executed = 0u64;
        while executed < RUN_LIMIT {
            match self.run_slice(RUN_LIMIT - executed) {
                RunOutcome::Running => {
                    executed = RUN_LIMIT;
                }
                _ => return,
            }
        }
        self.message = "step limit reached".to_string();
    }

    // -- memory view -------------------------------------------------------

    pub fn set_viewport(&mut self, rows: usize, cols: usize) {
        self.view_rows = rows.clamp(1, 16);
        self.view_cols = cols.clamp(1, 16);
        self.clamp_memory();
    }

    pub fn scroll_memory(&mut self, rows: i32, cols: i32) {
        self.mem_row0 = (self.mem_row0 as i32 + rows).max(0) as usize;
        self.mem_col0 = (self.mem_col0 as i32 + cols).max(0) as usize;
        self.clamp_memory();
    }

    pub fn memory_home(&mut self) {
        self.mem_row0 = 0;
        self.mem_col0 = 0;
    }

    pub fn memory_end(&mut self) {
        self.mem_row0 = 16usize.saturating_sub(self.view_rows);
        self.mem_col0 = 16usize.saturating_sub(self.view_cols);
    }

    fn clamp_memory(&mut self) {
        self.mem_row0 = self.mem_row0.min(16usize.saturating_sub(self.view_rows));
        self.mem_col0 = self.mem_col0.min(16usize.saturating_sub(self.view_cols));
    }

    pub fn history_trimmed(&self) -> bool {
        self.history_trimmed
    }

    // -- help --------------------------------------------------------------

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn close_help(&mut self) {
        self.show_help = false;
    }

    fn trim_history(&mut self) {
        while self.history.len() > HISTORY_LIMIT + 1 {
            self.history.remove(0);
            if !self.micro_texts.is_empty() {
                self.micro_texts.remove(0);
            }
            if !self.micro_ops.is_empty() {
                self.micro_ops.remove(0);
            }
            self.history_trimmed = true;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuField {
    Pc,
    Ir,
    Mar,
    Mdr,
}

/// `[fetch 2/4]  M[MAR] -> MDR`, using the tick *before* the transfer.
pub fn describe_tick(tick: u8, execute_ticks: u8, text: &str) -> String {
    let label = if tick < FETCH_TICKS {
        format!("fetch {}/{}", tick + 1, FETCH_TICKS)
    } else {
        format!("exec {}/{}", tick - FETCH_TICKS + 1, execute_ticks)
    };
    format!("[{label}]  {text}")
}

/// The compact help shown by `--dump` and by the window's help pane.
pub const HELP_LINES: &[&str] = &[
    "n / Enter           step one register transfer",
    "i                   step one whole instruction",
    "b / Backspace       step back one transfer",
    "c / r               continue (stops at a halt or any key)",
    "j k h l / arrows    scroll the memory pane (rows / columns)",
    "Home / End          memory pane to the top / bottom",
    "Ctrl+M              switch Machine Code / Assembler",
    "Ctrl+R              reset the machine",
    "Ctrl+O              open a file",
    "?                   this help",
    "q / Esc             quit",
    "",
    "The diagram lights the parts used by the transfer just done.",
    "Every instruction starts with the same four transfers:",
    "  fetch 1/4  PC -> MAR",
    "  fetch 2/4  M[MAR] -> MDR",
    "  fetch 3/4  MDR -> IR",
    "  fetch 4/4  PC + 1 -> PC",
];

fn parse_image(text: &str) -> Result<Vec<u16>, String> {
    let mut image = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() != 4 || !line.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(format!(
                "line {}: machine code must be four hexadecimal digits (`{line}`)",
                index + 1
            ));
        }
        image.push(
            u16::from_str_radix(line, 16)
                .map_err(|_| format!("line {}: invalid word `{line}`", index + 1))?,
        );
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: [u16; 5] = [0x1102, 0x1203, 0x6012, 0x40FF, 0xE000];

    fn demo() -> Debugger {
        Debugger::from_image("tests/demo.mc", &DEMO)
    }

    #[test]
    fn starts_on_head_and_marks_pc() {
        let debugger = demo();
        assert_eq!(debugger.step_count(), 0);
        assert_eq!(debugger.current_row(), None);
        assert_eq!(debugger.machine.pc, 0);
        assert_eq!(debugger.program_len(), 5);
        assert_eq!(debugger.active_register(), None);
    }

    #[test]
    fn step_instruction_advances_the_machine() {
        let mut debugger = demo();
        debugger.step_instruction();
        debugger.step_instruction();
        debugger.step_instruction();
        assert_eq!(debugger.machine.pc, 3);
        assert_eq!(debugger.machine.registers[0], 5);
        assert_eq!(debugger.current_row(), Some(2));
        assert_eq!(debugger.active_register(), Some(0));
        assert!(debugger.step_count() >= 3);
    }

    #[test]
    fn halt_stops_the_machine_and_marks_the_halt_row() {
        let mut debugger = demo();
        for _ in 0..5 {
            debugger.step_instruction();
        }
        assert!(debugger.halted);
        assert_eq!(debugger.machine.pc, 5);
        assert_eq!(debugger.current_row(), Some(4));
        assert_eq!(debugger.machine.memory[0xFF], 5);
        assert_eq!(debugger.machine.instructions, 5);
    }

    #[test]
    fn back_restores_the_previous_transfer() {
        let mut debugger = demo();
        for _ in 0..3 {
            debugger.step_transfer();
        }
        assert_eq!(debugger.step_count(), 3);
        debugger.back();
        assert_eq!(debugger.step_count(), 2);
        debugger.back();
        debugger.back();
        assert_eq!(debugger.step_count(), 0);
        debugger.back();
        assert_eq!(debugger.message, "already at the start");
    }

    #[test]
    fn first_load_needs_four_fetch_transfers_plus_one_execute() {
        let mut debugger = demo();
        for _ in 0..4 {
            debugger.step_transfer();
        }
        assert_eq!(debugger.machine.pc, 1);
        assert_eq!(debugger.machine.ir, 0x1102);
        assert_eq!(debugger.machine.registers[1], 0);
        debugger.step_transfer();
        assert_eq!(debugger.machine.registers[1], 2);
        assert_eq!(debugger.machine.tick, 0);
        assert!(debugger.micro_text().unwrap_or("").starts_with("[exec 1/"));
    }

    #[test]
    fn micro_ops_record_the_bus_and_the_direction() {
        let mut debugger = demo();

        debugger.step_transfer(); // PC -> MAR
        assert!(debugger.last_op().is_some());
        assert!(debugger.bus().is_none());

        debugger.step_transfer(); // M[MAR] -> MDR
        let read = debugger.bus().expect("the fetch reads memory");
        assert_eq!(read.direction, massey_core::BusDirection::Read);
        assert_eq!(read.address, 0);
        assert_eq!(read.value, 0x1102);

        // Finish LOAD R1, then run LOAD R2, ADD R0 and STORE R0, [FF].
        for _ in 0..4 {
            debugger.step_instruction();
        }
        let write = debugger.bus().expect("the store writes memory");
        assert_eq!(write.direction, massey_core::BusDirection::Write);
        assert_eq!(write.address, 0xFF);
        assert_eq!(write.value, 5);

        // Stepping back drops the matching record.
        debugger.back();
        assert!(debugger.bus().is_none());
    }

    #[test]
    fn transfer_stepping_runs_to_the_same_halt() {
        let mut debugger = demo();
        for _ in 0..60 {
            if debugger.halted {
                break;
            }
            debugger.step_transfer();
        }
        assert!(debugger.halted);
        assert_eq!(debugger.machine.memory[0xFF], 5);
        assert_eq!(debugger.machine.instructions, 5);
    }

    #[test]
    fn run_slice_runs_to_halt() {
        let mut debugger = demo();
        let outcome = debugger.run_slice(100);
        assert_eq!(outcome, RunOutcome::Halted);
        assert!(debugger.halted);
        assert_eq!(debugger.machine.memory[0xFF], 5);
    }

    #[test]
    fn run_to_end_halts_and_writes_memory() {
        let mut debugger = demo();
        debugger.run_to_end();
        assert!(debugger.halted);
        assert_eq!(debugger.machine.memory[0xFF], 5);
    }

    #[test]
    fn fault_is_recoverable_with_back() {
        let mut debugger = Debugger::from_image("fault.mc", &[0x5000]);
        debugger.step_instruction();
        assert!(debugger.faulted);
        assert!(!debugger.message.is_empty());
        while debugger.step_count() > 0 {
            debugger.back();
        }
        assert!(!debugger.faulted);
        assert_eq!(debugger.step_count(), 0);
    }

    #[test]
    fn empty_program_keeps_the_head_entry() {
        let mut debugger = Debugger::from_image("empty.mc", &[]);
        assert_eq!(debugger.program_len(), 0);
        assert_eq!(debugger.current_row(), None);
        debugger.step_transfer();
        assert_eq!(debugger.step_count(), 1);
    }

    #[test]
    fn memory_scroll_is_clamped() {
        let mut debugger = demo();
        debugger.set_viewport(8, 8);
        debugger.memory_end();
        assert_eq!(debugger.mem_row0, 8);
        assert_eq!(debugger.mem_col0, 8);
        debugger.scroll_memory(100, 100);
        assert_eq!(debugger.mem_row0, 8);
        assert_eq!(debugger.mem_col0, 8);
        debugger.memory_home();
        assert_eq!(debugger.mem_row0, 0);
    }

    #[test]
    fn help_is_a_pane_not_a_modal() {
        let mut debugger = demo();
        debugger.toggle_help();
        assert!(debugger.show_help);
        debugger.close_help();
        assert!(!debugger.show_help);
    }
}
