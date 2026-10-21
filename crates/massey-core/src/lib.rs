//! Core of MasseyCode: the MASSEY machine, its assembler, its disassembler and
//! the editor support that a teaching demonstrator needs.
//!
//! The crate has no user interface dependencies at all, so the machine can be
//! tested headlessly and embedded anywhere.
//!
//! ```no_run
//! use massey_core::{Document, Machine, SourceMode};
//!
//! // A `.mc` file stores machine code; the assembler view is derived from it.
//! let mut document = Document::from_source("demo.mc", "LOAD R1, 02\nLOAD R2, 03\nR0 = R1 + R2\nHALT\n");
//! assert!(document.program().is_ok());
//! assert_eq!(document.image(), &[0x1102, 0x1203, 0x6012, 0xE000]);
//!
//! // What gets written to disk is the machine code, one word per line.
//! assert!(document.text(SourceMode::MachineCode).starts_with("1102\n"));
//!
//! let mut machine = Machine::new();
//! machine.load_image(document.image());
//! machine.run(10_000, &[]);
//! assert_eq!(machine.registers[0], 5);
//! ```

pub mod asm;
pub mod complete;
pub mod disasm;
pub mod examples;
pub mod highlight;
pub mod isa;
pub mod session;
pub mod vm;

use std::path::{Path, PathBuf};

pub use asm::{
    image_to_text, label_address, summarize, text_to_image, Diagnostic, Label, LineKind,
    ListingRow, Program, Severity, SourceLine,
};
pub use complete::{complete, Completion, CompletionContext, CompletionKind, CompletionList};
pub use disasm::{
    active_register, disassemble, disassemble_image, disassemble_labeled, read_register,
    written_register,
};
pub use highlight::{tokenize, tokenize_source, Token, TokenKind};
pub use isa::{
    binary_grouped, format_byte, format_hex, format_word, parse_register, register_name,
    Instruction, InstructionGroup, Opcode, MEMORY_WORDS, REGISTER_COUNT,
};
pub use session::{
    AssembleReport, LogEntry, LogLevel, RegisterRow, Session, DEFAULT_SPEED, MAX_SPEED, MIN_SPEED,
    SPEED_STEPS,
};
pub use vm::{
    AluRecord, BusDirection, BusTransfer, Fault, FaultKind, InstructionReport, Machine, MicroOp,
    Phase, RunStop, Snapshot, TraceRecord,
};

/// Assemble a source string into a program image.
pub fn assemble(source: &str) -> Program {
    Program::assemble(source)
}

/// Which of the two views of a program the editor is showing.
///
/// The two views are two spellings of the same thing: a program is a list of
/// 16-bit machine words, and assembler code is a readable way of writing those
/// words down.  The `.mc` file always stores the machine code; the assembler
/// view is what the editor shows when you want to read or write it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SourceMode {
    /// Assembler code, the notation of the course notes.
    #[default]
    Assembly,
    /// Machine code: one hexadecimal word per line.
    MachineCode,
}

impl SourceMode {
    /// Both modes, in the order the toggle shows them.
    pub const ALL: [SourceMode; 2] = [SourceMode::Assembly, SourceMode::MachineCode];

    /// Short label for buttons and tabs.
    pub const fn label(self) -> &'static str {
        match self {
            SourceMode::Assembly => "Assembly",
            SourceMode::MachineCode => "Machine code",
        }
    }

    /// The other mode.
    pub const fn toggled(self) -> SourceMode {
        match self {
            SourceMode::Assembly => SourceMode::MachineCode,
            SourceMode::MachineCode => SourceMode::Assembly,
        }
    }
}

/// A `.mc` document.
///
/// A document holds
///
/// * the **machine code image** — the canonical content, exactly what a `.mc`
///   file contains and exactly what the machine executes;
/// * the **assembly text** — the readable form, kept so that labels and comments
///   survive while you switch between the two views;
/// * the **assembly result** — labels, the source map and the diagnostics.
///
/// Editing the assembly reassembles the image.  Editing the machine code
/// rebuilds the assembly by disassembling the image.  Either way the two views
/// never disagree.
#[derive(Clone, Debug)]
pub struct Document {
    /// Where the document was loaded from; `None` for a fresh buffer.
    pub path: Option<PathBuf>,
    /// Which view the editor is showing.
    mode: SourceMode,
    /// The assembler spelling of the program.
    assembly: String,
    /// The machine code, one word per memory location from address 00.
    image: Vec<u16>,
    /// The last assembly result.
    program: Program,
    /// True when the machine code was edited directly, so the assembly text has
    /// to be regenerated before it can be shown.
    machine_edited: bool,
    /// True when the document has been edited since the last save.
    dirty: bool,
}

impl Default for Document {
    fn default() -> Self {
        Document::from_source("untitled.mc", "")
    }
}

impl Document {
    /// A document from assembler source.  The image is assembled immediately.
    pub fn from_source(name: impl Into<String>, source: impl Into<String>) -> Self {
        let path = PathBuf::from(name.into());
        let assembly = source.into();
        let program = Program::assemble(&assembly);
        let image = program.image().to_vec();
        Document {
            path: Some(path),
            mode: SourceMode::Assembly,
            assembly,
            image,
            program,
            machine_edited: false,
            dirty: false,
        }
    }

    /// A document from a machine code image.  The assembly view is generated by
    /// disassembling the image.
    pub fn from_image(name: impl Into<String>, image: &[u16]) -> Self {
        let path = PathBuf::from(name.into());
        let assembly = disassemble_image(image);
        let program = Program::assemble(&assembly);
        Document {
            path: Some(path),
            mode: SourceMode::MachineCode,
            assembly,
            image: image.to_vec(),
            program,
            machine_edited: false,
            dirty: false,
        }
    }

    /// A document holding the first worked example.
    pub fn untitled() -> Self {
        Document::from_source("untitled.mc", examples::default_example().source)
    }

    /// Read a `.mc` file.
    ///
    /// A `.mc` file holds machine code, but a file full of assembler source is
    /// accepted too — the assembler notation of the course notes is what the
    /// assembly view is for.  The file is inspected and the right view chosen.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let text = std::fs::read_to_string(&path)?;
        let mut document = Document::from_file_text(path, &text);
        document.dirty = false;
        Ok(document)
    }

    /// Build a document from file text, choosing the view that fits.
    pub fn from_file_text(path: PathBuf, text: &str) -> Self {
        if looks_like_machine_code(text) {
            let mut document =
                Document::from_image(path.to_string_lossy().to_string(), &parse_image(text));
            document.path = Some(path);
            document
        } else {
            let mut document = Document::from_source(path.to_string_lossy().to_string(), text);
            document.path = Some(path);
            document
        }
    }

    /// Read a `.mc` file, or start from an empty buffer when it cannot be read.
    pub fn open_or_empty(path: impl AsRef<Path>) -> Self {
        match Document::open(path.as_ref()) {
            Ok(document) => document,
            Err(_) => Document::from_source(path.as_ref().to_string_lossy().to_string(), ""),
        }
    }

    // -- views --------------------------------------------------------------

    /// Which view the editor is showing.
    pub fn mode(&self) -> SourceMode {
        self.mode
    }

    /// The text of one view.
    pub fn text(&self, mode: SourceMode) -> String {
        match mode {
            SourceMode::Assembly => self.assembly.clone(),
            SourceMode::MachineCode => self.machine_code_text(),
        }
    }

    /// The machine code as a file: one hexadecimal word per line.
    pub fn machine_code_text(&self) -> String {
        image_to_text(&self.image)
    }

    /// Switch the view and return the text to show.
    ///
    /// Switching to the assembly view after the machine code was edited
    /// regenerates the assembly by disassembling the image, which is the only
    /// honest way to spell machine words in assembler code.
    pub fn switch_to(&mut self, mode: SourceMode) -> String {
        if mode == SourceMode::Assembly && self.machine_edited {
            self.assembly = disassemble_image(&self.image);
            self.program = Program::assemble(&self.assembly);
            self.machine_edited = false;
        }
        self.mode = mode;
        self.text(mode)
    }

    /// Replace the text of the given view.
    pub fn set_text(&mut self, mode: SourceMode, text: impl Into<String>) {
        match mode {
            SourceMode::Assembly => {
                self.assembly = text.into();
                self.machine_edited = false;
                self.reassemble();
            }
            SourceMode::MachineCode => {
                let text = text.into();
                self.image = parse_image(&text);
                self.machine_edited = true;
            }
        }
        self.mode = mode;
        self.dirty = true;
    }

    // -- content ------------------------------------------------------------

    /// The assembler spelling of the program.
    pub fn source(&self) -> &str {
        &self.assembly
    }

    /// The assembler text split into lines, without line terminators.
    pub fn lines(&self) -> Vec<&str> {
        self.assembly
            .split('\n')
            .map(|line| line.trim_end_matches('\r'))
            .collect()
    }

    /// The machine code image: what the machine executes and what is saved.
    pub fn image(&self) -> &[u16] {
        &self.image
    }

    /// The last assembly result.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// True when the assembly is valid.
    pub fn is_ok(&self) -> bool {
        self.program.is_ok()
    }

    /// Replace the assembler text and reassemble.
    pub fn set_source(&mut self, source: impl Into<String>) {
        self.assembly = source.into();
        self.machine_edited = false;
        self.dirty = true;
        self.reassemble();
    }

    /// Reassemble the assembler text and update the image when it is valid.
    ///
    /// When the assembly has errors the previous image is kept, so the machine
    /// still has something to run and the machine code view does not flicker
    /// while a line is half typed.
    pub fn reassemble(&mut self) {
        self.program = Program::assemble(&self.assembly);
        if self.program.is_ok() {
            self.image = self.program.image().to_vec();
        }
    }

    /// Disassemble the machine code back into the assembly view.
    pub fn rebuild_assembly_from_image(&mut self) {
        self.assembly = disassemble_image(&self.image);
        self.program = Program::assemble(&self.assembly);
        self.machine_edited = false;
    }

    // -- files --------------------------------------------------------------

    /// True when the document has been edited since the last save.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the document as saved.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// The file name to show in the title bar, with a `*` when unsaved.
    pub fn display_name(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled.mc".to_string());
        if self.dirty {
            format!("{name} *")
        } else {
            name
        }
    }

    /// Write the machine code back to the file it came from.
    pub fn save(&mut self) -> std::io::Result<()> {
        match self.path.clone() {
            Some(path) => self.save_as(path),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "this document has no file name yet",
            )),
        }
    }

    /// Write the machine code to a new path and remember it.
    ///
    /// A `.mc` file holds machine code: one hexadecimal word per line, exactly
    /// the format the command line `massey` program loads.
    pub fn save_as(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref().to_path_buf();
        std::fs::write(&path, self.machine_code_text().as_bytes())?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    /// Write the assembler spelling of the program to a file, for handing in a
    /// readable listing next to the machine code.
    pub fn export_assembly(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path.as_ref(), self.assembly.as_bytes())
    }

    /// Write the raw machine code image, one hexadecimal word per line.
    pub fn export_image(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path.as_ref(), self.machine_code_text().as_bytes())
    }

    /// Look up which source line produced the word at an address.
    pub fn source_line_for_address(&self, address: u8) -> Option<usize> {
        self.program.source_line_for_address(address)
    }
}

/// True when every content line of the text is a hexadecimal machine word.
pub fn looks_like_machine_code(text: &str) -> bool {
    let mut words = 0usize;
    for raw in text.lines() {
        let line = asm::split_comment(raw).0.trim();
        if line.is_empty() {
            continue;
        }
        let cleaned = line.trim_start_matches("0x").trim_start_matches("0X");
        if cleaned.is_empty()
            || cleaned.len() > 4
            || !cleaned.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return false;
        }
        words += 1;
    }
    words > 0
}

/// Parse machine code text: one hexadecimal word per line, comments allowed.
pub fn parse_image(text: &str) -> Vec<u16> {
    let mut image = Vec::new();
    for raw in text.lines() {
        let line = asm::split_comment(raw).0.trim();
        if line.is_empty() {
            continue;
        }
        let cleaned = line.trim_start_matches("0x").trim_start_matches("0X");
        if let Ok(word) = u16::from_str_radix(cleaned, 16) {
            if image.len() < MEMORY_WORDS {
                image.push(word);
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_assembles_on_creation() {
        let document = Document::from_source("t.mc", "LOAD R1, 02\nHALT\n");
        assert!(document.program().is_ok());
        assert_eq!(document.program().used, 2);
        assert_eq!(document.image(), &[0x1102, 0xE000]);
    }

    #[test]
    fn document_tracks_dirty_state() {
        let mut document = Document::from_source("t.mc", "HALT\n");
        assert!(!document.is_dirty());
        document.set_source("HALT\nHALT\n");
        assert!(document.is_dirty());
        assert_eq!(document.program().used, 2);
    }

    #[test]
    fn the_machine_code_view_is_what_a_mc_file_holds() {
        let document = Document::from_source("t.mc", "LOAD R1, 02\nHALT\n");
        assert_eq!(document.text(SourceMode::MachineCode), "1102\nE000\n");
    }

    #[test]
    fn editing_machine_code_rebuilds_the_assembly() {
        let mut document = Document::from_image("t.mc", &[0x1102, 0xE000]);
        assert_eq!(document.mode(), SourceMode::MachineCode);
        document.set_text(SourceMode::MachineCode, "1000\nE000\n");
        assert_eq!(document.image(), &[0x1000, 0xE000]);
        let assembly = document.switch_to(SourceMode::Assembly);
        assert!(assembly.contains("LOAD"), "{assembly}");
        assert!(assembly.contains("R0"), "{assembly}");
    }

    #[test]
    fn a_disassembled_image_assembles_back_to_itself() {
        let image = vec![
            0x3508u16, 0x3609, 0x6056, 0x400A, 0xE000, 0x001B, 0x0008, 0x0000,
        ];
        let assembly = disassemble_image(&image);
        let program = Program::assemble(&assembly);
        assert!(program.is_ok(), "{:?}", program.diagnostics);
        assert_eq!(program.image(), image.as_slice(), "{assembly}");
    }

    #[test]
    fn machine_code_files_are_recognised() {
        assert!(looks_like_machine_code("1032\n11FF\n2071\n"));
        assert!(looks_like_machine_code("; comment\n1032\n\n11FF\n"));
        assert!(!looks_like_machine_code("LOAD R1, 02\n"));
        assert!(!looks_like_machine_code(""));
    }

    #[test]
    fn saving_writes_machine_code_only() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mc-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("save-format.mc");

        let mut document = Document::from_source(
            path.to_string_lossy().to_string(),
            "; a comment that must not reach the file\nLOAD R5, [08]\nHALT\nNUM: .word 001B\n",
        );
        document.save_as(&path).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "3508\nE000\n001B\n");
        assert!(!written.contains(';'), "a .mc file holds machine code only");

        // ... and reading it back gives the same program.
        let reopened = Document::open(&path).unwrap();
        assert_eq!(reopened.image(), &[0x3508, 0xE000, 0x001B]);
        assert_eq!(reopened.mode(), SourceMode::MachineCode);
        let assembly = reopened.text(SourceMode::Assembly);
        assert!(assembly.contains("LOAD"), "{assembly}");
    }
}
