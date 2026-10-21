//! A headless editing and debugging session.
//!
//! [`Session`] owns a [`Document`] and a [`Machine`] and provides exactly the
//! operations the demonstrator's toolbar offers — assemble, run, pause, step,
//! micro-step, step back, breakpoints, reset — with no user interface attached.
//! Keeping that logic here means it can be unit tested, and it keeps the GPUI
//! layer a pure rendering concern.

use std::collections::BTreeSet;

use crate::asm::{Diagnostic, ListingRow, Program, Severity};
use crate::disasm;
use crate::isa::{register_name, Instruction, REGISTER_COUNT};
use crate::vm::{Machine, MicroOp, RunStop, Snapshot};
use crate::Document;

/// How serious a log line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    /// Ordinary progress information.
    Info,
    /// A machine event worth pointing out (a halt, a breakpoint).
    Notice,
    /// Something suspicious.
    Warning,
    /// Something went wrong.
    Error,
}

impl LogLevel {
    /// Short lowercase label.
    pub const fn label(self) -> &'static str {
        match self {
            LogLevel::Info => "info",
            LogLevel::Notice => "notice",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
        }
    }
}

/// One line of the session log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// Severity of the line.
    pub level: LogLevel,
    /// Where the line came from, e.g. `assembler` or `machine`.
    pub source: &'static str,
    /// The message.
    pub message: String,
    /// Instruction counter when the line was written.
    pub instructions: u64,
}

/// What happened the last time the program was assembled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssembleReport {
    /// True when the program assembled without errors and was loaded.
    pub loaded: bool,
    /// Number of words in the image.
    pub words: usize,
    /// Number of errors.
    pub errors: usize,
    /// Number of warnings.
    pub warnings: usize,
    /// One line summary suitable for a status bar.
    pub summary: String,
}

/// The state the whole demonstrator is built around.
#[derive(Clone, Debug)]
pub struct Session {
    document: Document,
    machine: Machine,
    breakpoints: BTreeSet<u8>,
    speed: u32,
    micro_mode: bool,
    log: Vec<LogEntry>,
    last_micro_op: Option<MicroOp>,
    last_stop: Option<RunStop>,
    changed: Vec<u8>,
    previous_memory: [u16; 256],
    running: bool,
}

/// The default number of instructions per second the run loop aims for.
pub const DEFAULT_SPEED: u32 = 40;
/// The lowest and highest speeds the slider offers.
pub const MIN_SPEED: u32 = 1;
/// See [`MIN_SPEED`].
pub const MAX_SPEED: u32 = 20_000;
/// Keyboard shortcut-friendly speed steps.
pub const SPEED_STEPS: [u32; 9] = [1, 5, 20, 50, 200, 1_000, 5_000, 10_000, 20_000];

impl Session {
    /// Start a session with a document, assembled and ready to run.
    pub fn new(document: Document) -> Self {
        let mut machine = Machine::new();
        let program = document.program();
        if program.is_ok() {
            machine.load_image(document.image());
        }
        let mut session = Session {
            document,
            machine,
            breakpoints: BTreeSet::new(),
            speed: DEFAULT_SPEED,
            micro_mode: false,
            log: Vec::new(),
            last_micro_op: None,
            last_stop: None,
            changed: Vec::new(),
            previous_memory: [0; 256],
            running: false,
        };
        session.note_program_loaded();
        session
    }

    /// A session holding one of the bundled examples.
    pub fn with_example(example: &crate::examples::Example) -> Self {
        Session::new(Document::from_source(example.file_name, example.source))
    }

    // -- accessors ----------------------------------------------------------

    /// The open document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// The open document, for editing.
    pub fn document_mut(&mut self) -> &mut Document {
        &mut self.document
    }

    /// The machine.
    pub fn machine(&self) -> &Machine {
        &self.machine
    }

    /// The assembled program.
    pub fn program(&self) -> &Program {
        self.document.program()
    }

    /// Breakpoint addresses.
    pub fn breakpoints(&self) -> &BTreeSet<u8> {
        &self.breakpoints
    }

    /// Recent log lines, oldest first.
    pub fn log_entries(&self) -> &[LogEntry] {
        &self.log
    }

    /// True when the run loop is currently driving the machine.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// True when the machine has halted or faulted.
    pub fn is_halted(&self) -> bool {
        self.machine.halted
    }

    /// The speed the run loop aims for, in instructions per second.
    pub fn speed(&self) -> u32 {
        self.speed
    }

    /// True when the toolbar is in micro-step mode.
    pub fn micro_mode(&self) -> bool {
        self.micro_mode
    }

    /// The last micro-operation the machine performed.
    pub fn last_micro_op(&self) -> Option<&MicroOp> {
        self.last_micro_op.as_ref()
    }

    /// Why the machine last stopped.
    pub fn last_stop(&self) -> Option<RunStop> {
        self.last_stop
    }

    /// Memory addresses the running program has written to.
    pub fn changed_memory(&self) -> &[u8] {
        &self.changed
    }

    /// Addresses written by the most recent step.
    pub fn recently_changed(&self) -> Vec<u8> {
        (0..256u16)
            .filter(|index| {
                self.machine.memory[*index as usize] != self.previous_memory[*index as usize]
            })
            .map(|index| index as u8)
            .collect()
    }

    /// The diagnostics of the current assembly.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.document.program().diagnostics
    }

    /// The source line the program counter points at, if the address came from
    /// the source (rather than being written by the program at run time).
    pub fn current_source_line(&self) -> Option<usize> {
        self.document
            .source_line_for_address(self.machine.instruction_address)
    }

    // -- editing ------------------------------------------------------------

    /// Replace the assembler text, reassemble, and reload the machine when the
    /// program is valid.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.document.set_source(source);
        self.reload_if_valid();
    }

    /// Replace the text of one view of the document.
    ///
    /// Editing the assembly reassembles the machine code; editing the machine
    /// code rebuilds the assembly.  Either way the machine is reloaded when the
    /// result is valid.
    pub fn set_text(&mut self, mode: crate::SourceMode, text: impl Into<String>) {
        self.document.set_text(mode, text);
        if mode == crate::SourceMode::MachineCode {
            self.reload_image();
        } else {
            self.reload_if_valid();
        }
    }

    /// Switch the editor between the assembly and machine code views and return
    /// the text to show.
    pub fn switch_mode(&mut self, mode: crate::SourceMode) -> String {
        let text = self.document.switch_to(mode);
        if mode == crate::SourceMode::Assembly {
            // The assembly may have been regenerated from the machine code, so
            // the source map the debugger uses has changed too.
            self.reload_if_valid();
        }
        text
    }

    /// The view the editor is showing.
    pub fn mode(&self) -> crate::SourceMode {
        self.document.mode()
    }

    /// Open another document.
    pub fn open(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let document = Document::open(path)?;
        self.document = document;
        self.reload_if_valid();
        self.log(
            LogLevel::Info,
            "editor",
            format!("opened {}", self.document.display_name()),
        );
        Ok(())
    }

    /// Save the document.
    pub fn save(&mut self) -> std::io::Result<()> {
        self.document.save()?;
        let name = self.document.display_name();
        self.log(LogLevel::Info, "editor", format!("saved {name}"));
        Ok(())
    }

    /// Save the document under a new name.
    pub fn save_as(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        self.document.save_as(path)?;
        let name = self.document.display_name();
        self.log(LogLevel::Info, "editor", format!("saved {name}"));
        Ok(())
    }

    // -- assembling ---------------------------------------------------------

    /// Reassemble the document and load the image into the machine.
    pub fn assemble(&mut self) -> AssembleReport {
        self.document.reassemble();
        let report = self.report();
        if report.loaded {
            self.machine.load_image(self.document.image());
            self.previous_memory = self.machine.memory;
            self.changed.clear();
            self.last_micro_op = None;
            self.last_stop = None;
            self.note_program_loaded();
        } else {
            self.log(
                LogLevel::Error,
                "assembler",
                format!(
                    "{} error{}: the program was not loaded",
                    report.errors,
                    if report.errors == 1 { "" } else { "s" }
                ),
            );
            let messages: Vec<String> = self
                .document
                .program()
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .take(8)
                .map(|d| d.to_string())
                .collect();
            for message in messages {
                self.log(LogLevel::Error, "assembler", message);
            }
        }
        report
    }

    fn report(&self) -> AssembleReport {
        let program = self.document.program();
        AssembleReport {
            loaded: program.is_ok(),
            words: self.document.image().len(),
            errors: program.error_count(),
            warnings: program.warning_count(),
            summary: crate::summarize(program),
        }
    }

    fn reload_if_valid(&mut self) {
        if self.document.program().is_ok() {
            self.reload_image();
        }
    }

    /// Load the document's machine code into the machine and reset it.
    fn reload_image(&mut self) {
        self.machine.load_image(self.document.image());
        self.previous_memory = self.machine.memory;
        self.changed.clear();
        self.last_micro_op = None;
        self.last_stop = None;
        self.running = false;
        self.note_program_loaded();
    }

    fn note_program_loaded(&mut self) {
        let report = self.report();
        if report.loaded {
            self.log(
                LogLevel::Info,
                "assembler",
                format!("assembled {} words, ready at address 00", report.words),
            );
        }
    }

    /// Load a raw machine code image (the `.txt` format of the command line
    /// program) into the machine without touching the document.
    pub fn load_image(&mut self, image: &[u16]) {
        self.machine.load_image(image);
        self.previous_memory = self.machine.memory;
        self.changed.clear();
        self.running = false;
        self.log(
            LogLevel::Info,
            "machine",
            format!("loaded a {} word machine code image", image.len()),
        );
    }

    // -- running ------------------------------------------------------------

    /// Put the machine back to the state it had when the program was loaded.
    pub fn reset(&mut self) {
        self.machine.reset();
        self.previous_memory = self.machine.memory;
        self.changed.clear();
        self.last_micro_op = None;
        self.last_stop = None;
        self.running = false;
        self.log(
            LogLevel::Notice,
            "machine",
            "reset: PC = 00, registers cleared",
        );
    }

    /// Start (or resume) the run loop.  The caller drives it with
    /// [`Session::run_slice`] on a timer.
    pub fn start(&mut self) {
        if self.machine.halted {
            self.log(
                LogLevel::Warning,
                "machine",
                "the machine has halted; press Reset to run it again",
            );
            return;
        }
        self.running = true;
        self.last_stop = None;
        self.log(LogLevel::Notice, "machine", "running");
    }

    /// Pause the run loop.
    pub fn pause(&mut self) {
        if self.running {
            self.running = false;
            self.log(LogLevel::Notice, "machine", "paused");
        }
    }

    /// Stop the run loop after a halt, a fault or a breakpoint.
    pub fn stop_with(&mut self, stop: RunStop) {
        self.running = false;
        self.last_stop = Some(stop);
        match stop {
            RunStop::Halted => {
                self.log(
                    LogLevel::Notice,
                    "machine",
                    format!(
                        "HALT after {} instructions and {} register transfers",
                        self.machine.instructions, self.machine.micro_operations
                    ),
                );
            }
            RunStop::Breakpoint(address) => {
                self.log(
                    LogLevel::Notice,
                    "machine",
                    format!("breakpoint at address {address:02X}"),
                );
            }
            RunStop::Fault => {
                let message = self
                    .machine
                    .fault
                    .map(|fault| fault.message())
                    .unwrap_or_else(|| "the machine stopped".to_string());
                self.log(LogLevel::Error, "machine", message);
            }
            RunStop::Budget => {
                self.log(
                    LogLevel::Warning,
                    "machine",
                    "still running after the step budget: the program may never halt",
                );
            }
            RunStop::AlreadyHalted => {}
        }
    }

    /// Execute up to `budget` instructions.  Returns the reason for stopping, or
    /// `None` when the run loop should carry on.
    pub fn run_slice(&mut self, budget: u64) -> Option<RunStop> {
        if self.machine.halted {
            let stop = RunStop::AlreadyHalted;
            self.stop_with(stop);
            return Some(stop);
        }
        let mut executed = 0u64;
        while executed < budget {
            let pc = self.machine.pc;
            if executed > 0 && self.breakpoints.contains(&pc) {
                let stop = RunStop::Breakpoint(pc);
                self.stop_with(stop);
                return Some(stop);
            }
            self.machine.push_history();
            self.machine.step_instruction();
            executed += 1;
            if self.machine.halted {
                self.after_step();
                let stop = if self.machine.fault.is_some() {
                    RunStop::Fault
                } else {
                    RunStop::Halted
                };
                self.stop_with(stop);
                return Some(stop);
            }
        }
        self.after_step();
        None
    }

    /// Execute exactly one instruction.
    pub fn step_instruction(&mut self) {
        if self.machine.halted {
            self.log(
                LogLevel::Warning,
                "machine",
                "the machine has halted; press Reset to run it again",
            );
            return;
        }
        self.machine.push_history();
        let report = self.machine.step_instruction();
        self.after_step();
        self.log(
            LogLevel::Info,
            "step",
            format!(
                "{:02X}  {:04X}  {}",
                report.address, report.word, report.text
            ),
        );
        if report.faulted {
            let message = self
                .machine
                .fault
                .map(|fault| fault.message())
                .unwrap_or_default();
            self.log(LogLevel::Error, "machine", message);
        }
    }

    /// Execute exactly one register transfer.
    pub fn step_micro(&mut self) {
        if self.machine.halted {
            self.log(
                LogLevel::Warning,
                "machine",
                "the machine has halted; press Reset to run it again",
            );
            return;
        }
        self.machine.push_history();
        let op = self.machine.micro_step();
        self.last_micro_op = Some(op.clone());
        self.after_step();
        if op.faulted {
            let message = self
                .machine
                .fault
                .map(|fault| fault.message())
                .unwrap_or_default();
            self.log(LogLevel::Error, "machine", message);
        }
    }

    /// Undo the last step (instruction or register transfer).
    pub fn step_back(&mut self) {
        if self.machine.undo() {
            self.after_step();
            self.running = false;
            self.log(LogLevel::Info, "machine", "stepped back");
        } else {
            self.log(
                LogLevel::Warning,
                "machine",
                "there is no earlier state to step back to",
            );
        }
    }

    /// True when there is something to step back to.
    pub fn can_step_back(&self) -> bool {
        self.machine.history_len() > 0
    }

    fn after_step(&mut self) {
        self.changed = self.recently_changed();
        self.previous_memory = self.machine.memory;
    }

    /// Capture the current machine state.
    pub fn snapshot(&self) -> Snapshot {
        self.machine.snapshot()
    }

    /// Restore a captured machine state.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        self.machine.restore(snapshot);
        self.after_step();
    }

    // -- breakpoints --------------------------------------------------------

    /// Add or remove a breakpoint at an address.  Returns true when the address
    /// now has a breakpoint.
    pub fn toggle_breakpoint(&mut self, address: u8) -> bool {
        if self.breakpoints.remove(&address) {
            false
        } else {
            self.breakpoints.insert(address);
            true
        }
    }

    /// True when the address has a breakpoint.
    pub fn has_breakpoint(&self, address: u8) -> bool {
        self.breakpoints.contains(&address)
    }

    /// Remove every breakpoint.
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    // -- presentation helpers ----------------------------------------------

    /// Set the run loop speed, in instructions per second.
    pub fn set_speed(&mut self, speed: u32) {
        self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    }

    /// Move to the next speed step, up or down.
    pub fn nudge_speed(&mut self, up: bool) {
        let current = self.speed;
        let next = if up {
            SPEED_STEPS
                .iter()
                .copied()
                .find(|step| *step > current)
                .unwrap_or(MAX_SPEED)
        } else {
            SPEED_STEPS
                .iter()
                .rev()
                .copied()
                .find(|step| *step < current)
                .unwrap_or(MIN_SPEED)
        };
        self.set_speed(next);
    }

    /// Toggle between instruction stepping and micro stepping.
    pub fn toggle_micro_mode(&mut self) {
        self.micro_mode = !self.micro_mode;
    }

    /// The disassembly listing of the loaded program.
    pub fn listing(&self) -> Vec<ListingRow> {
        let mut rows = self.document.program().listing();
        // Addresses the program has written to may no longer be the words that
        // were assembled, so show what is in memory now.
        for row in rows.iter_mut() {
            let live = self.machine.memory[row.address as usize];
            if live != row.word {
                row.word = live;
                row.text = format!("{}  ; was {:04X}", disasm::disassemble(live), row.word);
            }
        }
        rows
    }

    /// One row per register, for the register pane.
    pub fn register_rows(&self) -> Vec<RegisterRow> {
        (0..REGISTER_COUNT as u8)
            .map(|index| {
                let value = self.machine.registers[index as usize];
                RegisterRow {
                    index,
                    name: register_name(index),
                    value,
                    decimal: value as i32,
                    signed: crate::isa::to_signed(value),
                    binary: crate::isa::binary_grouped(value),
                    written: self
                        .last_micro_op
                        .as_ref()
                        .map(|op| op.register_writes.iter().any(|(r, _)| *r == index))
                        .unwrap_or(false),
                }
            })
            .collect()
    }

    /// The decoded instruction the program counter points at.
    pub fn current_instruction(&self) -> Instruction {
        self.machine.current_instruction()
    }

    /// A one line description of where the machine is.
    pub fn status_line(&self) -> String {
        let machine = &self.machine;
        if let Some(fault) = machine.fault {
            return fault.message();
        }
        if machine.halted {
            return format!(
                "halted at {:02X} after {} instructions",
                machine.instruction_address, machine.instructions
            );
        }
        let decoded = machine.current_instruction();
        format!(
            "PC={:02X}  IR={:04X}  {}",
            machine.pc,
            machine.ir,
            disasm::disassemble(machine.memory[machine.pc as usize])
        )
        .to_string()
            + if decoded.opcode().is_none() {
                "  (illegal instruction)"
            } else {
                ""
            }
    }

    /// Append a line to the log, dropping the oldest lines when it grows.
    pub fn log(&mut self, level: LogLevel, source: &'static str, message: impl Into<String>) {
        if self.log.len() >= 500 {
            self.log.drain(..100);
        }
        self.log.push(LogEntry {
            level,
            source,
            message: message.into(),
            instructions: self.machine.instructions,
        });
    }
}

/// One row of the register pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterRow {
    /// Register index, 0..15.
    pub index: u8,
    /// `R0` .. `RF`.
    pub name: String,
    /// Raw 16-bit value.
    pub value: u16,
    /// Unsigned decimal value.
    pub decimal: i32,
    /// Signed (2's complement) value.
    pub signed: i16,
    /// Binary, grouped in fours.
    pub binary: String,
    /// True when the last register transfer wrote this register.
    pub written: bool,
}

impl RegisterRow {
    /// The value as four hexadecimal digits.
    pub fn hex(&self) -> String {
        crate::isa::format_word(self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples::default_example;

    fn example_session() -> Session {
        Session::with_example(default_example())
    }

    #[test]
    fn a_new_session_is_assembled_and_ready() {
        let session = example_session();
        assert!(session.program().is_ok());
        assert_eq!(session.machine().pc, 0);
        assert!(!session.is_halted());
        assert!(session.machine().image_len() > 0);
    }

    #[test]
    fn stepping_runs_one_instruction_at_a_time() {
        let mut session = example_session();
        session.step_instruction();
        assert_eq!(session.machine().instructions, 1);
        assert!(session.can_step_back());
        session.step_back();
        assert_eq!(session.machine().instructions, 0);
    }

    #[test]
    fn micro_stepping_walks_the_fetch_cycle() {
        let mut session = example_session();
        let expected = ["PC -> MAR", "M[MAR] -> MDR", "MDR -> IR", "PC + 1 -> PC"];
        for text in expected {
            session.step_micro();
            assert_eq!(
                session.last_micro_op().map(|op| op.text.as_str()),
                Some(text)
            );
        }
    }

    #[test]
    fn running_stops_at_a_halt() {
        let mut session = example_session();
        session.start();
        let mut guard = 0;
        while session.is_running() {
            if session.run_slice(16).is_some() {
                break;
            }
            guard += 1;
            assert!(guard < 10_000, "the run loop never finished");
        }
        assert!(session.is_halted());
        assert_eq!(session.machine().memory[0x0A], 0x0023);
    }

    #[test]
    fn breakpoints_stop_the_run() {
        let mut session = example_session();
        session.toggle_breakpoint(3);
        assert!(session.has_breakpoint(3));
        let stop = session.run_slice(100);
        assert_eq!(stop, Some(RunStop::Breakpoint(3)));
        session.toggle_breakpoint(3);
        assert!(!session.has_breakpoint(3));
        assert_eq!(session.run_slice(100), Some(RunStop::Halted));
    }

    #[test]
    fn editing_source_reloads_the_machine() {
        let mut session = example_session();
        session.set_source("LOAD R1, 07\nHALT\n");
        assert_eq!(session.machine().image_len(), 2);
        assert_eq!(session.machine().memory[0], 0x1107);
    }

    #[test]
    fn a_broken_program_is_not_loaded() {
        let mut session = example_session();
        let before = session.machine().image_len();
        session.set_source("LOAD R1, [NOPE]\n");
        assert_eq!(
            session.machine().image_len(),
            before,
            "the previous program should stay loaded"
        );
        let report = session.assemble();
        assert!(!report.loaded);
        assert!(report.errors > 0);
    }

    #[test]
    fn speed_steps_move_through_the_table() {
        let mut session = example_session();
        session.set_speed(40);
        session.nudge_speed(true);
        assert_eq!(session.speed(), 50);
        session.nudge_speed(false);
        assert_eq!(session.speed(), 20);
    }

    #[test]
    fn register_rows_describe_every_register() {
        let mut session = example_session();
        session.step_instruction();
        let rows = session.register_rows();
        assert_eq!(rows.len(), 16);
        assert_eq!(rows[5].name, "R5");
        assert_eq!(rows[0].hex(), "0000");
    }
}
