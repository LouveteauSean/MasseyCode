//! The `.mc` assembler.
//!
//! A `.mc` file is the source form of a MASSEY machine program.  Every content
//! line of a `.mc` file produces exactly one 16-bit memory word, because every
//! MASSEY machine instruction occupies exactly one memory location.  A file may
//! freely mix:
//!
//! * **machine code**, written as four hexadecimal digits (`1234`), which is
//!   assembled to itself.  This is what a plain machine code file holds.
//! * **assembler code**, written in the notation of the 159.102 course notes
//!   (`LOAD R1, 34`, `R0 = R1 + R2`, `JUMP [0A] IF R3=R0`, ...).
//! * **directives**, which control where the words go: `.org`, `.word`,
//!   `.equ`, `.data`, `.code`.
//!
//! ```text
//! ; two numbers are added and the total is stored at 0A
//!         LOAD  R5, [08]      ; R5 = memory[08]
//!         LOAD  R6, [09]
//!         R0 = R5 + R6
//!         STORE R0, [0A]
//!         HALT
//! .data
//! VALUE:  .word 001B          ; 27
//! OTHER:  .word 0008
//! TOTAL:  .word 0000
//! ```
//!
//! Labels are case insensitive and may be defined on a line of their own or in
//! front of an instruction.  Numbers are hexadecimal unless they carry an
//! explicit `0x` or `$` prefix, exactly like the course notes.

use std::collections::{btree_map::Entry, BTreeMap};
use std::fmt;

use crate::isa::{format_byte, format_word, parse_register, Instruction, Opcode, MEMORY_WORDS};

/// The highest address the machine can hold.
pub const MAX_ADDRESS: u8 = (MEMORY_WORDS - 1) as u8;

/// How serious a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Informational remark; the program still loads.
    Note,
    /// Suspicious source; the program still loads.
    Warning,
    /// The program cannot be assembled; the offending word is left as 0000.
    Error,
}

impl Severity {
    /// Short lowercase label for badges and status bars.
    pub const fn label(self) -> &'static str {
        match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One assembler message, positioned at a line and column so an editor can
/// underline the offending text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity of the message.
    pub severity: Severity,
    /// Zero-based line index in the source.
    pub line: usize,
    /// Zero-based column index (in characters) where the problem starts.
    pub column: usize,
    /// Number of characters the underline should cover (at least 1).
    pub length: usize,
    /// Stable machine readable code, e.g. `E-UNKNOWN-SYMBOL`.
    pub code: &'static str,
    /// Human readable message.
    pub message: String,
}

impl Diagnostic {
    fn new(
        severity: Severity,
        line: usize,
        column: usize,
        length: usize,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Diagnostic {
            severity,
            line,
            column,
            length: length.max(1),
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}:{}: {}: {}",
            self.line + 1,
            self.column + 1,
            self.severity,
            self.message
        )
    }
}

/// What kind of source line this is, used by listings and the editor gutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineKind {
    /// Empty line.
    Blank,
    /// Comment-only line.
    Comment,
    /// A bare label with nothing to assemble.
    LabelOnly,
    /// A machine instruction.
    Instruction,
    /// A data word (`.word`, or a raw hex word after `.data`).
    Data,
    /// A raw hexadecimal machine-code word.
    RawWord,
    /// A directive (`.org`, `.equ`, `.data`, ...).
    Directive,
    /// A constant definition.
    Equate,
    /// A line that failed to assemble.
    Error,
}

impl LineKind {
    /// Human readable name used by listings.
    pub const fn label(self) -> &'static str {
        match self {
            LineKind::Blank => "",
            LineKind::Comment => "comment",
            LineKind::LabelOnly => "label",
            LineKind::Instruction => "code",
            LineKind::Data => "data",
            LineKind::RawWord => "machine code",
            LineKind::Directive => "directive",
            LineKind::Equate => "constant",
            LineKind::Error => "error",
        }
    }

    /// True when the line contributes a word to the program image.
    pub const fn emits_word(self) -> bool {
        matches!(
            self,
            LineKind::Instruction | LineKind::Data | LineKind::RawWord
        )
    }
}

/// Per-line information produced by the assembler; this is what lets the editor
/// show the address of every line and sync the debugger with the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLine {
    /// Zero-based line index in the source text.
    pub index: usize,
    /// Address the line assembled to, when it emitted a word.
    pub address: Option<u8>,
    /// The 16-bit word the line produced.
    pub word: Option<u16>,
    /// Label defined on this line, if any.
    pub label: Option<String>,
    /// Classification of the line.
    pub kind: LineKind,
}

/// A label and the address it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// Label spelling as written in the source.
    pub name: String,
    /// Address the label resolves to.
    pub address: u8,
    /// Line the label was defined on.
    pub line: usize,
}

/// The result of assembling a `.mc` file.
#[derive(Clone, Debug)]
pub struct Program {
    /// The assembled program image; unused locations are zero.
    pub words: [u16; MEMORY_WORDS],
    /// Number of memory words the source actually produced.
    pub used: usize,
    /// Per-line assembly information, indexed by source line.
    pub lines: Vec<SourceLine>,
    /// Every label, in address order.
    pub labels: Vec<Label>,
    /// Every `.equ` constant, in definition order.
    pub constants: Vec<(String, u16)>,
    /// Assembler messages.
    pub diagnostics: Vec<Diagnostic>,
}

impl Default for Program {
    fn default() -> Self {
        Program {
            words: [0; MEMORY_WORDS],
            used: 0,
            lines: Vec::new(),
            labels: Vec::new(),
            constants: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

impl Program {
    /// Assemble a source string.
    pub fn assemble(source: &str) -> Program {
        Assembler::new(source).run()
    }

    /// True when no error-severity diagnostics were produced.
    pub fn is_ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Number of error-severity diagnostics.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    /// Number of warning-severity diagnostics.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    /// The program image as a slice of the first `used` words.
    pub fn image(&self) -> &[u16] {
        &self.words[..self.used]
    }

    /// The source line that produced the word at `address`, if any.
    pub fn source_line_for_address(&self, address: u8) -> Option<usize> {
        self.lines
            .iter()
            .find(|line| line.address == Some(address) && line.word.is_some())
            .map(|line| line.index)
    }

    /// The address of the word produced by a source line, if any.
    pub fn address_of_line(&self, line: usize) -> Option<u8> {
        self.lines.get(line).and_then(|info| info.address)
    }

    /// The label defined at an address, if any.
    pub fn label_at(&self, address: u8) -> Option<&str> {
        self.labels
            .iter()
            .find(|label| label.address == address)
            .map(|label| label.name.as_str())
    }

    /// One row per address in the image, for a memory/listing view.
    pub fn listing(&self) -> Vec<ListingRow> {
        (0..self.used)
            .map(|address| {
                let word = self.words[address];
                ListingRow {
                    address: address as u8,
                    word,
                    text: crate::disasm::disassemble(word),
                    label: self.label_at(address as u8).map(|s| s.to_string()),
                    source_line: self.source_line_for_address(address as u8),
                    is_code: crate::disasm::written_register(word).is_some()
                        || matches!(
                            Instruction::from_word(word).opcode(),
                            Some(Opcode::StoreMemory) | Some(Opcode::Jump) | Some(Opcode::Halt)
                        ),
                }
            })
            .collect()
    }
}

/// One row of a disassembly listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListingRow {
    /// Memory address.
    pub address: u8,
    /// Raw 16-bit word.
    pub word: u16,
    /// Disassembled text.
    pub text: String,
    /// Label defined at this address.
    pub label: Option<String>,
    /// Source line that produced this word.
    pub source_line: Option<usize>,
    /// True when the word decodes as an instruction.
    pub is_code: bool,
}

impl ListingRow {
    /// `0025  1102  LOAD R1, 02` —the classic three column listing.
    pub fn render(&self) -> String {
        format!(
            "{:02X}  {}  {}",
            self.address,
            format_word(self.word),
            self.text
        )
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Number(i64),
    Symbol(String),
    Unary(Box<Expr>),
    Binary(char, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn describe(&self) -> String {
        match self {
            Expr::Number(value) => format!("{value:X}"),
            Expr::Symbol(name) => name.clone(),
            Expr::Unary(inner) => format!("-{}", inner.describe()),
            Expr::Binary(op, left, right) => {
                format!("{} {} {}", left.describe(), op, right.describe())
            }
        }
    }
}

struct ExprParser<'a> {
    text: &'a str,
    bytes: Vec<char>,
    position: usize,
}

impl<'a> ExprParser<'a> {
    fn new(text: &'a str) -> Self {
        ExprParser {
            text,
            bytes: text.chars().collect(),
            position: 0,
        }
    }

    fn parse(mut self) -> Result<Expr, String> {
        self.skip_space();
        if self.position >= self.bytes.len() {
            return Err("expected a value".to_string());
        }
        let expr = self.parse_sum()?;
        self.skip_space();
        if self.position < self.bytes.len() {
            return Err(format!(
                "unexpected `{}` in value",
                self.bytes[self.position..].iter().collect::<String>()
            ));
        }
        Ok(expr)
    }

    fn skip_space(&mut self) {
        while self.position < self.bytes.len() && self.bytes[self.position].is_whitespace() {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.bytes.get(self.position).copied()
    }

    fn parse_sum(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_product()?;
        loop {
            self.skip_space();
            match self.peek() {
                Some('+') | Some('-') => {
                    let op = self.bytes[self.position];
                    self.position += 1;
                    let right = self.parse_product()?;
                    left = Expr::Binary(op, Box::new(left), Box::new(right));
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_product(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            self.skip_space();
            match self.peek() {
                Some('*') => {
                    self.position += 1;
                    let right = self.parse_unary()?;
                    left = Expr::Binary('*', Box::new(left), Box::new(right));
                }
                _ => return Ok(left),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.skip_space();
        match self.peek() {
            Some('-') => {
                self.position += 1;
                Ok(Expr::Unary(Box::new(self.parse_unary()?)))
            }
            Some('+') => {
                self.position += 1;
                self.parse_unary()
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, String> {
        self.skip_space();
        match self.peek() {
            Some('(') => {
                self.position += 1;
                let inner = self.parse_sum()?;
                self.skip_space();
                if self.peek() != Some(')') {
                    return Err("missing `)`".to_string());
                }
                self.position += 1;
                Ok(inner)
            }
            Some(ch) if ch.is_ascii_digit() || ch == '$' => self.parse_number(),
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' || ch == '.' => {
                let start = self.position;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                        self.position += 1;
                    } else {
                        break;
                    }
                }
                let name: String = self.bytes[start..self.position].iter().collect();
                Ok(Expr::Symbol(name))
            }
            Some(ch) => Err(format!("unexpected character `{ch}` in value")),
            None => Err("expected a value".to_string()),
        }
    }

    fn parse_number(&mut self) -> Result<Expr, String> {
        let start = self.position;
        if self.peek() == Some('$') {
            self.position += 1;
            let digits_start = self.position;
            while let Some(ch) = self.peek() {
                if ch.is_ascii_hexdigit() {
                    self.position += 1;
                } else {
                    break;
                }
            }
            if self.position == digits_start {
                return Err("`$` must be followed by hexadecimal digits".to_string());
            }
            let digits: String = self.bytes[digits_start..self.position].iter().collect();
            return i64::from_str_radix(&digits, 16)
                .map(Expr::Number)
                .map_err(|_| format!("`${digits}` is not a valid hexadecimal number"));
        }
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() {
                self.position += 1;
            } else {
                break;
            }
        }
        let token: String = self.bytes[start..self.position].iter().collect();
        let lowered = token.to_ascii_lowercase();
        if let Some(digits) = lowered.strip_prefix("0x") {
            return i64::from_str_radix(digits, 16)
                .map(Expr::Number)
                .map_err(|_| format!("`{token}` is not a valid hexadecimal number"));
        }
        if let Some(digits) = lowered.strip_prefix("0d") {
            return digits
                .parse::<i64>()
                .map(Expr::Number)
                .map_err(|_| format!("`{token}` is not a valid decimal number"));
        }
        if token.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return i64::from_str_radix(&token, 16)
                .map(Expr::Number)
                .map_err(|_| format!("`{token}` is not a valid hexadecimal number"));
        }
        if token.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') {
            return Ok(Expr::Symbol(token));
        }
        Err(format!("`{}` is not a valid value", self.text))
    }
}

// ---------------------------------------------------------------------------
// Statement parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Statement {
    Empty,
    Instruction(InstructionAst),
    Word(Expr),
    Org(Expr),
    Equ(String, Expr),
    DataMarker(bool),
    LabelOnly,
}

#[derive(Clone, Debug)]
enum InstructionAst {
    /// `LOAD Rx, <value>`
    LoadImmediate { register: u8, value: Expr },
    /// `LOAD Rx, Ry`
    LoadRegister { x: u8, z: u8 },
    /// `LOAD Rx, [<address>]`
    LoadMemory { register: u8, address: Expr },
    /// `STORE Rx, [<address>]`
    StoreMemory { register: u8, address: Expr },
    /// `Rx = Ry + Rz`
    Add { x: u8, y: u8, z: u8 },
    /// `NEGATE Rx`
    Negate { register: u8 },
    /// `SHIFT Rx LEFT|RIGHT <count>`
    Shift {
        register: u8,
        left: bool,
        count: Expr,
    },
    /// `Rx = Ry AND|OR|XOR Rz`
    Logic { opcode: Opcode, x: u8, y: u8, z: u8 },
    /// `JUMP [<address>] IF Rx=R0`
    Jump { register: u8, address: Expr },
    /// `HALT`
    Halt,
    /// A literal 16-bit word written as four hexadecimal digits.
    RawWord(u16),
}

#[derive(Clone, Debug)]
struct ParsedLine {
    label: Option<String>,
    label_column: usize,
    statement: Statement,
    /// Column where the statement text begins (for diagnostics).
    column: usize,
    /// True when the source line is a comment or blank.
    comment_only: bool,
}

struct Assembler<'a> {
    source: &'a str,
    program: Program,
    labels: BTreeMap<String, (u8, usize)>,
    constants: BTreeMap<String, i64>,
    address: usize,
    in_data: bool,
    parsed: Vec<ParsedLine>,
}

impl<'a> Assembler<'a> {
    fn new(source: &'a str) -> Self {
        Assembler {
            source,
            program: Program::default(),
            labels: BTreeMap::new(),
            constants: BTreeMap::new(),
            address: 0,
            in_data: false,
            parsed: Vec::new(),
        }
    }

    fn run(mut self) -> Program {
        let raw_lines: Vec<&str> = self.source.split('\n').collect();
        for (index, raw) in raw_lines.iter().enumerate() {
            let parsed = self.parse_line(index, raw);
            self.parsed.push(parsed);
        }
        self.layout();
        self.encode();
        self.finish_diagnostics();
        self.program
    }

    // -- parsing ------------------------------------------------------------

    fn parse_line(&mut self, index: usize, raw: &str) -> ParsedLine {
        let text = raw.strip_suffix('\r').unwrap_or(raw);
        let (code, comment) = split_comment(text);
        let mut line = ParsedLine {
            label: None,
            label_column: 0,
            statement: Statement::Empty,
            column: 0,
            comment_only: code.trim().is_empty() && !comment.is_empty(),
        };

        let mut rest = code;
        let mut offset = 0usize;

        // A leading label:  NAME:
        let trimmed = rest.trim_start();
        offset += rest.len() - trimmed.len();
        rest = trimmed;
        if let Some((name, after)) = split_label(rest) {
            let name = name.to_string();
            line.label_column = offset;
            line.label = Some(name.clone());
            let consumed = rest.len() - after.len();
            offset += consumed;
            rest = after;
        }

        let body = rest.trim_start();
        offset += rest.len() - body.len();
        let body = body.trim_end();
        line.column = offset;

        if body.is_empty() {
            line.statement = if line.label.is_some() {
                Statement::LabelOnly
            } else {
                Statement::Empty
            };
            return line;
        }

        match self.parse_statement(index, body, offset) {
            Ok(statement) => line.statement = statement,
            Err(message) => {
                self.program.diagnostics.push(Diagnostic::new(
                    Severity::Error,
                    index,
                    offset,
                    body.len(),
                    "E-SYNTAX",
                    message,
                ));
                line.statement = Statement::Empty;
            }
        }
        line
    }

    fn parse_statement(
        &mut self,
        line: usize,
        body: &str,
        column: usize,
    ) -> Result<Statement, String> {
        let lowered = body.to_ascii_lowercase();

        // Directives.
        if let Some(rest) = strip_directive(&lowered, ".org").or_else(|| {
            if let Some(rest) = strip_keyword(&lowered, "org") {
                Some(rest)
            } else {
                None
            }
        }) {
            let argument = body[body.len() - rest.len()..].trim();
            return Ok(Statement::Org(self.parse_expr(argument)?));
        }
        if let Some(rest) = strip_directive(&lowered, ".word") {
            let argument = body[body.len() - rest.len()..].trim();
            return Ok(Statement::Word(self.parse_expr(argument)?));
        }
        if let Some(rest) = strip_directive(&lowered, ".equ") {
            let argument = body[body.len() - rest.len()..].trim();
            let (name, value) = argument
                .split_once([' ', '\t', ','])
                .ok_or_else(|| "`.equ` needs a name and a value".to_string())?;
            let name = name.trim();
            if name.is_empty() {
                return Err("`.equ` needs a name".to_string());
            }
            return Ok(Statement::Equ(
                name.to_string(),
                self.parse_expr(value.trim())?,
            ));
        }
        if strip_directive(&lowered, ".data").is_some() {
            return Ok(Statement::DataMarker(true));
        }
        if strip_directive(&lowered, ".code").is_some() {
            return Ok(Statement::DataMarker(false));
        }
        if lowered.starts_with('.') {
            return Err(format!(
                "unknown directive `{}`",
                body.split_whitespace().next().unwrap_or(body)
            ));
        }

        // Assignment forms:  Rx = Ry + Rz / AND / OR / XOR
        if let Some(statement) = self.parse_assignment(line, body)? {
            return Ok(statement);
        }

        // Mnemonics.
        let first = body
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        match first.as_str() {
            "LOAD" => self.parse_load(line, body),
            "STORE" => self.parse_store(line, body),
            "NEGATE" => self.parse_negate(line, body),
            "SHIFT" => self.parse_shift(line, body),
            "JUMP" => self.parse_jump(line, body),
            "HALT" => Ok(Statement::Instruction(InstructionAst::Halt)),
            _ => {
                // A bare four digit hexadecimal word is machine code.
                if let Some(word) = parse_raw_word(body) {
                    return Ok(Statement::Instruction(InstructionAst::RawWord(word)));
                }
                let _ = column;
                Err(format!(
                    "unknown instruction `{}`; expected LOAD, STORE, NEGATE, SHIFT, JUMP, HALT, \
                     a `Rx = Ry op Rz` assignment, a four digit machine word or a directive",
                    body.split_whitespace().next().unwrap_or(body)
                ))
            }
        }
    }

    fn parse_expr(&self, text: &str) -> Result<Expr, String> {
        if text.trim().is_empty() {
            return Err("expected a value".to_string());
        }
        ExprParser::new(text).parse()
    }

    fn parse_assignment(&self, _line: usize, body: &str) -> Result<Option<Statement>, String> {
        let Some((lhs, rhs)) = body.split_once('=') else {
            return Ok(None);
        };
        // `JUMP ... IF Rx=R0` also contains `=`, but it is handled later; an
        // assignment always starts with a register name.
        let Some(x) = parse_register(lhs.trim()) else {
            let left = lhs.trim();
            if left.len() > 2 && left.to_ascii_uppercase().starts_with('R') {
                return Err(format!(
                    "`{left}` is not a register name: the machine encodes registers in one \
                     hexadecimal digit, so register 10 is RA and register 15 is RF"
                ));
            }
            return Ok(None);
        };
        let rhs = rhs.trim();
        let rhs_upper = rhs.to_ascii_uppercase();
        for (keyword, opcode) in [
            (" AND ", Opcode::And),
            (" OR ", Opcode::Or),
            (" XOR ", Opcode::Xor),
        ] {
            if let Some(position) = rhs_upper.find(keyword) {
                let left = rhs[..position].trim();
                let right = rhs[position + keyword.len()..].trim();
                let y = parse_register(left)
                    .ok_or_else(|| format!("`{left}` is not a register (use R0..RF)"))?;
                let z = parse_register(right)
                    .ok_or_else(|| format!("`{right}` is not a register (use R0..RF)"))?;
                return Ok(Some(Statement::Instruction(InstructionAst::Logic {
                    opcode,
                    x,
                    y,
                    z,
                })));
            }
        }
        if let Some(position) = rhs.find('+') {
            let left = rhs[..position].trim();
            let right = rhs[position + 1..].trim();
            let y = parse_register(left)
                .ok_or_else(|| format!("`{left}` is not a register (use R0..RF)"))?;
            let z = parse_register(right)
                .ok_or_else(|| format!("`{right}` is not a register (use R0..RF)"))?;
            return Ok(Some(Statement::Instruction(InstructionAst::Add {
                x,
                y,
                z,
            })));
        }
        Err(format!(
            "`{}` is not a MASSEY instruction; the arithmetic form is `Rx = Ry + Rz`",
            body
        ))
    }

    fn parse_load(&self, line: usize, body: &str) -> Result<Statement, String> {
        let operands = split_operands(after_keyword(body));
        if operands.len() != 2 {
            return Err(
                "LOAD needs two operands: `LOAD Rx, YZ`, `LOAD Rx, Ry` or `LOAD Rx, [YZ]`"
                    .to_string(),
            );
        }
        let _ = line;
        let register_text = operands[0].trim();
        let register = parse_register(register_text).ok_or_else(|| {
            format!("`{register_text}` is not a register; the MASSEY machine has R0..RF")
        })?;
        let second = operands[1].trim();
        if let Some(inner) = strip_brackets(second) {
            return Ok(Statement::Instruction(InstructionAst::LoadMemory {
                register,
                address: self.parse_expr(inner)?,
            }));
        }
        if let Some(source) = parse_register(second) {
            return Ok(Statement::Instruction(InstructionAst::LoadRegister {
                x: register,
                z: source,
            }));
        }
        Ok(Statement::Instruction(InstructionAst::LoadImmediate {
            register,
            value: self.parse_expr(second)?,
        }))
    }

    fn parse_store(&self, _line: usize, body: &str) -> Result<Statement, String> {
        let operands = split_operands(after_keyword(body));
        if operands.len() != 2 {
            return Err("STORE needs two operands: `STORE Rx, [YZ]`".to_string());
        }
        let register_text = operands[0].trim();
        let register = parse_register(register_text).ok_or_else(|| {
            format!("`{register_text}` is not a register; the MASSEY machine has R0..RF")
        })?;
        let second = operands[1].trim();
        let inner = strip_brackets(second).ok_or_else(|| {
            "STORE writes to memory, so the second operand must be an address in brackets, \
             e.g. `STORE R1, [0A]`"
                .to_string()
        })?;
        Ok(Statement::Instruction(InstructionAst::StoreMemory {
            register,
            address: self.parse_expr(inner)?,
        }))
    }

    fn parse_negate(&self, _line: usize, body: &str) -> Result<Statement, String> {
        let argument = body
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| "NEGATE needs a register: `NEGATE R1`".to_string())?;
        let register = parse_register(argument.trim()).ok_or_else(|| {
            format!("`{argument}` is not a register; the MASSEY machine has R0..RF")
        })?;
        Ok(Statement::Instruction(InstructionAst::Negate { register }))
    }

    fn parse_shift(&self, _line: usize, body: &str) -> Result<Statement, String> {
        let parts: Vec<&str> = body.split_whitespace().collect();
        if parts.len() < 4 {
            return Err("SHIFT needs a direction and a bit count: `SHIFT R1 LEFT 2`".to_string());
        }
        let register = parse_register(parts[1]).ok_or_else(|| {
            format!(
                "`{}` is not a register; the MASSEY machine has R0..RF",
                parts[1]
            )
        })?;
        let left = match parts[2].to_ascii_uppercase().as_str() {
            "LEFT" => true,
            "RIGHT" => false,
            other => {
                return Err(format!(
                    "`{other}` is not a shift direction; use LEFT or RIGHT"
                ))
            }
        };
        let count_text = parts[3..].join(" ");
        Ok(Statement::Instruction(InstructionAst::Shift {
            register,
            left,
            count: self.parse_expr(count_text.trim())?,
        }))
    }

    fn parse_jump(&self, _line: usize, body: &str) -> Result<Statement, String> {
        let upper = body.to_ascii_uppercase();
        let Some(if_position) = upper.find(" IF ") else {
            return Err(
                "JUMP needs a condition: `JUMP [YZ] IF Rx=R0` (use `IF R0=R0` to jump always)"
                    .to_string(),
            );
        };
        let head = body[..if_position].trim();
        let condition = body[if_position + 4..].trim();

        let mut head_parts = head.split_whitespace();
        let _mnemonic = head_parts.next();
        let mut target_text = head_parts.next().unwrap_or_default();
        if target_text.eq_ignore_ascii_case("TO") {
            target_text = head_parts.next().unwrap_or_default();
        }
        if target_text.is_empty() {
            return Err("JUMP needs an address: `JUMP [YZ] IF Rx=R0`".to_string());
        }
        let address_text = strip_brackets(target_text).unwrap_or(target_text);

        let condition_compact: String = condition.chars().filter(|c| !c.is_whitespace()).collect();
        let Some((register_text, rest)) = condition_compact.split_once('=') else {
            return Err("the JUMP condition must look like `R3=R0`".to_string());
        };
        let register = parse_register(register_text).ok_or_else(|| {
            format!("`{register_text}` is not a register; the MASSEY machine has R0..RF")
        })?;
        if !rest.eq_ignore_ascii_case("R0") {
            return Err(format!(
                "a MASSEY JUMP can only compare against R0, not `{rest}`"
            ));
        }
        Ok(Statement::Instruction(InstructionAst::Jump {
            register,
            address: self.parse_expr(address_text.trim())?,
        }))
    }

    // -- layout -------------------------------------------------------------

    fn layout(&mut self) {
        let parsed = std::mem::take(&mut self.parsed);
        for (index, line) in parsed.iter().enumerate() {
            let mut info = SourceLine {
                index,
                address: None,
                word: None,
                label: line.label.clone(),
                kind: LineKind::Blank,
            };

            if let Some(name) = &line.label {
                let key = name.to_ascii_uppercase();
                if self.labels.contains_key(&key) {
                    let previous = self.labels[&key].1;
                    self.program.diagnostics.push(Diagnostic::new(
                        Severity::Error,
                        index,
                        line.label_column,
                        name.len(),
                        "E-DUPLICATE-LABEL",
                        format!("label `{name}` is already defined on line {}", previous + 1),
                    ));
                } else if self.address > MAX_ADDRESS as usize {
                    self.program.diagnostics.push(Diagnostic::new(
                        Severity::Error,
                        index,
                        line.label_column,
                        name.len(),
                        "E-OUT-OF-MEMORY",
                        format!("label `{name}` is past the end of memory (address FF)"),
                    ));
                } else {
                    self.labels.insert(key, (self.address as u8, index));
                    self.program.labels.push(Label {
                        name: name.clone(),
                        address: self.address as u8,
                        line: index,
                    });
                }
            }

            info.kind = match &line.statement {
                Statement::Empty => {
                    if line.comment_only {
                        LineKind::Comment
                    } else {
                        LineKind::Blank
                    }
                }
                Statement::LabelOnly => LineKind::LabelOnly,
                Statement::DataMarker(data) => {
                    self.in_data = *data;
                    LineKind::Directive
                }
                Statement::Equ(name, expr) => {
                    match self.eval(expr) {
                        Ok(value) => {
                            let key = name.to_ascii_uppercase();
                            match self.constants.entry(key) {
                                Entry::Occupied(_) => {
                                    self.program.diagnostics.push(Diagnostic::new(
                                        Severity::Error,
                                        index,
                                        line.column,
                                        name.len(),
                                        "E-DUPLICATE-CONSTANT",
                                        format!("constant `{name}` is already defined"),
                                    ))
                                }
                                Entry::Vacant(slot) => {
                                    slot.insert(value);
                                    self.program.constants.push((name.clone(), value as u16));
                                }
                            }
                        }
                        Err(message) => self.program.diagnostics.push(Diagnostic::new(
                            Severity::Error,
                            index,
                            line.column,
                            name.len(),
                            "E-CONSTANT-VALUE",
                            format!("cannot evaluate `{name}`: {message}"),
                        )),
                    }
                    LineKind::Equate
                }
                Statement::Org(expr) => {
                    match self.eval(expr) {
                        Ok(value) => {
                            if !(0..=MAX_ADDRESS as i64).contains(&value) {
                                self.program.diagnostics.push(Diagnostic::new(
                                    Severity::Error,
                                    index,
                                    line.column,
                                    4,
                                    "E-ORG-RANGE",
                                    format!("`.org` address {value:X} is outside memory (00..FF)"),
                                ));
                            } else {
                                if value < self.address as i64 {
                                    self.program.diagnostics.push(Diagnostic::new(
                                        Severity::Warning,
                                        index,
                                        line.column,
                                        4,
                                        "W-ORG-BACKWARDS",
                                        format!(
                                            "`.org {value:X}` moves back over words that were \
                                             already placed in memory"
                                        ),
                                    ));
                                }
                                self.address = value as usize;
                            }
                        }
                        Err(message) => self.program.diagnostics.push(Diagnostic::new(
                            Severity::Error,
                            index,
                            line.column,
                            4,
                            "E-ORG-VALUE",
                            format!("`.org` needs a constant address: {message}"),
                        )),
                    }
                    LineKind::Directive
                }
                Statement::Word(_) => {
                    self.occupy(&mut info, index, LineKind::Data);
                    LineKind::Data
                }
                Statement::Instruction(InstructionAst::RawWord(_)) => {
                    let kind = if self.in_data {
                        LineKind::Data
                    } else {
                        LineKind::RawWord
                    };
                    self.occupy(&mut info, index, kind);
                    kind
                }
                Statement::Instruction(_) => {
                    let kind = if self.in_data {
                        LineKind::Data
                    } else {
                        LineKind::Instruction
                    };
                    self.occupy(&mut info, index, kind);
                    kind
                }
            };

            self.program.lines.push(info);
        }
        self.parsed = parsed;
    }

    fn occupy(&mut self, info: &mut SourceLine, index: usize, kind: LineKind) {
        if self.address > MAX_ADDRESS as usize {
            self.program.diagnostics.push(Diagnostic::new(
                Severity::Error,
                index,
                0,
                1,
                "E-OUT-OF-MEMORY",
                format!(
                    "line {} needs address {:02X} but the MASSEY machine only has 256 memory \
                     locations (00..FF)",
                    index + 1,
                    self.address
                ),
            ));
            info.kind = LineKind::Error;
            return;
        }
        info.address = Some(self.address as u8);
        info.kind = kind;
        self.address += 1;
        self.program.used = self.program.used.max(self.address);
    }

    // -- encoding -----------------------------------------------------------

    fn encode(&mut self) {
        let parsed = std::mem::take(&mut self.parsed);
        for (index, line) in parsed.iter().enumerate() {
            let Some(address) = self.program.lines[index].address else {
                continue;
            };
            let word = match &line.statement {
                Statement::Word(expr) => match self.eval(expr) {
                    Ok(value) => {
                        if !(0..=0xFFFF).contains(&value) {
                            self.error(
                                index,
                                line.column,
                                1,
                                "E-WORD-RANGE",
                                format!("`.word` value {value} does not fit in 16 bits"),
                            );
                            0
                        } else {
                            value as u16
                        }
                    }
                    Err(message) => {
                        self.error(index, line.column, 1, "E-WORD-VALUE", message);
                        0
                    }
                },
                Statement::Instruction(ast) => self.encode_instruction(index, line.column, ast),
                _ => continue,
            };
            self.program.lines[index].word = Some(word);
            self.program.words[address as usize] = word;
        }
        self.parsed = parsed;
    }

    fn encode_instruction(&mut self, line: usize, column: usize, ast: &InstructionAst) -> u16 {
        match ast {
            InstructionAst::RawWord(word) => *word,
            InstructionAst::Halt => Instruction::new(Opcode::Halt, 0, 0, 0).word(),
            InstructionAst::LoadRegister { x, z } => {
                Instruction::new(Opcode::LoadRegister, 0, *x, *z).word()
            }
            InstructionAst::Add { x, y, z } => Instruction::new(Opcode::Add, *x, *y, *z).word(),
            InstructionAst::Logic { opcode, x, y, z } => {
                Instruction::new(*opcode, *x, *y, *z).word()
            }
            InstructionAst::Negate { register } => {
                Instruction::new(Opcode::Negate, 0, *register, 0).word()
            }
            InstructionAst::LoadImmediate { register, value } => match self.eval_byte(value) {
                Ok(byte) => {
                    Instruction::new(Opcode::LoadImmediate, *register, byte >> 4, byte & 0x0F)
                        .word()
                }
                Err(message) => {
                    self.error(line, column, 1, "E-IMMEDIATE-VALUE", message);
                    0
                }
            },
            InstructionAst::LoadMemory { register, address } => match self.eval_byte(address) {
                Ok(byte) => {
                    Instruction::new(Opcode::LoadMemory, *register, byte >> 4, byte & 0x0F).word()
                }
                Err(message) => {
                    self.error(line, column, 1, "E-ADDRESS-VALUE", message);
                    0
                }
            },
            InstructionAst::StoreMemory { register, address } => match self.eval_byte(address) {
                Ok(byte) => {
                    Instruction::new(Opcode::StoreMemory, *register, byte >> 4, byte & 0x0F).word()
                }
                Err(message) => {
                    self.error(line, column, 1, "E-ADDRESS-VALUE", message);
                    0
                }
            },
            InstructionAst::Shift {
                register,
                left,
                count,
            } => match self.eval_byte(count) {
                Ok(byte) => {
                    let opcode = if *left {
                        Opcode::ShiftLeft
                    } else {
                        Opcode::ShiftRight
                    };
                    Instruction::new(opcode, *register, byte >> 4, byte & 0x0F).word()
                }
                Err(message) => {
                    self.error(line, column, 1, "E-SHIFT-VALUE", message);
                    0
                }
            },
            InstructionAst::Jump { register, address } => match self.eval_byte(address) {
                Ok(byte) => {
                    Instruction::new(Opcode::Jump, *register, byte >> 4, byte & 0x0F).word()
                }
                Err(message) => {
                    self.error(line, column, 1, "E-JUMP-VALUE", message);
                    0
                }
            },
        }
    }

    fn eval_byte(&self, expr: &Expr) -> Result<u8, String> {
        let value = self.eval(expr)?;
        if !(0..=0xFF).contains(&value) {
            return Err(format!(
                "`{}` is {value:X} hex, which does not fit in the 8 bits available for an \
                 address or immediate value (00..FF)",
                expr.describe()
            ));
        }
        Ok(value as u8)
    }

    fn eval(&self, expr: &Expr) -> Result<i64, String> {
        match expr {
            Expr::Number(value) => Ok(*value),
            Expr::Symbol(name) => {
                let key = name.to_ascii_uppercase();
                if let Some((address, _)) = self.labels.get(&key) {
                    return Ok(*address as i64);
                }
                if let Some(value) = self.constants.get(&key) {
                    return Ok(*value);
                }
                // The course notes write hexadecimal constants without a
                // prefix: `LOAD R1, FC`, `LOAD R9, D0`, `SHIFT RB LEFT 8`.
                // Such a token is only a number when it does not name a label
                // or a constant, so a label called `A` still beats the number A.
                if !name.is_empty() && name.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    if let Ok(value) = i64::from_str_radix(name, 16) {
                        return Ok(value);
                    }
                }
                Err(format!("unknown label or constant `{name}`"))
            }
            Expr::Unary(inner) => Ok(-self.eval(inner)?),
            Expr::Binary(op, left, right) => {
                let left = self.eval(left)?;
                let right = self.eval(right)?;
                match op {
                    '+' => Ok(left + right),
                    '-' => Ok(left - right),
                    '*' => Ok(left * right),
                    other => Err(format!("unsupported operator `{other}`")),
                }
            }
        }
    }

    fn error(
        &mut self,
        line: usize,
        column: usize,
        length: usize,
        code: &'static str,
        message: impl Into<String>,
    ) {
        self.program.diagnostics.push(Diagnostic::new(
            Severity::Error,
            line,
            column,
            length,
            code,
            message,
        ));
    }

    fn finish_diagnostics(&mut self) {
        for (index, info) in self.program.lines.iter().enumerate() {
            if info.word == Some(0) && info.kind == LineKind::Instruction {
                if let Some(word) = info.word {
                    if word == 0 && Instruction::from_word(word).opcode().is_none() {
                        // Encoded into an illegal word: report once.
                        let already = self
                            .program
                            .diagnostics
                            .iter()
                            .any(|d| d.line == index && d.severity == Severity::Error);
                        if !already {
                            self.program.diagnostics.push(Diagnostic::new(
                                Severity::Error,
                                index,
                                0,
                                1,
                                "E-BAD-ENCODING",
                                "this line did not produce a valid MASSEY instruction",
                            ));
                        }
                    }
                }
            }
        }

        let has_halt = self
            .program
            .lines
            .iter()
            .any(|line| line.word.map(|word| word >> 12 == 0xE).unwrap_or(false));
        if !has_halt && self.program.used > 0 {
            self.program.diagnostics.push(Diagnostic::new(
                Severity::Warning,
                0,
                0,
                1,
                "W-NO-HALT",
                "this program has no HALT instruction, so it will run past the end of memory",
            ));
        }
        if self.program.used == 0 {
            self.program.diagnostics.push(Diagnostic::new(
                Severity::Note,
                0,
                0,
                1,
                "N-EMPTY",
                "the file does not contain any instruction or data word yet",
            ));
        }
        self.program
            .diagnostics
            .sort_by_key(|d| (d.line, d.column, d.severity));
    }
}

// ---------------------------------------------------------------------------
// Small text helpers
// ---------------------------------------------------------------------------

/// Split a source line into code and comment text.  `;` starts a comment, and
/// so does `#` at the start of a word.
pub fn split_comment(line: &str) -> (&str, &str) {
    let bytes: Vec<char> = line.chars().collect();
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == ';' {
            let position = char_offset(line, index);
            return (&line[..position], &line[position..]);
        }
        if ch == '#' {
            let position = char_offset(line, index);
            return (&line[..position], &line[position..]);
        }
        index += 1;
    }
    (line, "")
}

fn char_offset(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(offset, _)| offset)
        .unwrap_or(text.len())
}

/// Split `NAME: rest` into the label name and the rest of the line.
fn split_label(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let name = text[..colon].trim_end();
    if name.is_empty() || !is_identifier(name) {
        return None;
    }
    Some((name, &text[colon + 1..]))
}

/// True when the text is a valid label/constant identifier.
pub fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(ch) if ch.is_ascii_alphabetic() || ch == '_' || ch == '.' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
}

fn strip_directive<'a>(lowered: &'a str, directive: &str) -> Option<&'a str> {
    let rest = lowered.strip_prefix(directive)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        // Return the *original* (non-lowercased) tail by length arithmetic.
        Some(&lowered[lowered.len() - rest.len()..])
    } else {
        None
    }
}

fn strip_keyword<'a>(lowered: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = lowered.strip_prefix(keyword)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(&lowered[lowered.len() - rest.len()..])
    } else {
        None
    }
}

/// The text of a statement after its leading keyword, e.g. `R1, [0A]` for
/// `LOAD R1, [0A]`.
fn after_keyword(body: &str) -> &str {
    match body.find(char::is_whitespace) {
        Some(index) => body[index..].trim(),
        None => "",
    }
}

fn strip_brackets(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner)
}

/// Split operands on commas that are not inside brackets.
fn split_operands(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    for ch in body.chars() {
        match ch {
            '[' | '(' => {
                depth += 1;
                current.push(ch);
            }
            ']' | ')' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() || !parts.is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Recognise a bare machine code word such as `1234` or `0x1234`.
fn parse_raw_word(body: &str) -> Option<u16> {
    let text = body.trim();
    let lowered = text.to_ascii_lowercase();
    if let Some(digits) = lowered.strip_prefix("0x") {
        if digits.len() == 4 && digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return u16::from_str_radix(digits, 16).ok();
        }
        return None;
    }
    if text.len() == 4 && text.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return u16::from_str_radix(text, 16).ok();
    }
    None
}

/// Render a program image as a machine code file, one word per line.
///
/// This is the format a `.mc` file holds and what the machine loads.
pub fn image_to_text(words: &[u16]) -> String {
    let mut out = String::new();
    for word in words {
        out.push_str(&format_word(*word));
        out.push('\n');
    }
    out
}

/// Parse a machine code text file (one hexadecimal word per line).
pub fn text_to_image(text: &str) -> Result<Vec<u16>, String> {
    let mut words = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = split_comment(raw).0.trim();
        if line.is_empty() {
            // Before the first word, blank lines and comments are skipped;
            // after it, anything that is not a word ends the image.
            if words.is_empty() {
                continue;
            }
            break;
        }
        let cleaned = line.trim_start_matches("0x").trim_start_matches("0X");
        if cleaned.len() > 4 || !cleaned.chars().all(|ch| ch.is_ascii_hexdigit()) {
            if words.is_empty() {
                continue;
            }
            break;
        }
        let word = u16::from_str_radix(cleaned, 16)
            .map_err(|_| format!("line {}: `{line}` is not a hexadecimal word", index + 1))?;
        if words.len() >= MEMORY_WORDS {
            return Err(format!(
                "line {}: the image is longer than the {} memory locations of the MASSEY machine",
                index + 1,
                MEMORY_WORDS
            ));
        }
        words.push(word);
    }
    Ok(words)
}

/// Format a short summary of an assembled program, used in the status bar.
pub fn summarize(program: &Program) -> String {
    format!(
        "{} word{} used, {} label{}, {} error{}, {} warning{}",
        program.used,
        if program.used == 1 { "" } else { "s" },
        program.labels.len(),
        if program.labels.len() == 1 { "" } else { "s" },
        program.error_count(),
        if program.error_count() == 1 { "" } else { "s" },
        program.warning_count(),
        if program.warning_count() == 1 {
            ""
        } else {
            "s"
        },
    )
}

/// Convenience: the address of a label, if it is defined.
pub fn label_address(program: &Program, name: &str) -> Option<u8> {
    program
        .labels
        .iter()
        .find(|label| label.name.eq_ignore_ascii_case(name))
        .map(|label| label.address)
}

/// Convenience: render `AA  BB` style hex addresses for a memory range.
pub fn render_address_range(start: u8, count: usize) -> Vec<String> {
    (0..count)
        .map(|offset| format_byte(start.wrapping_add(offset as u8)))
        .collect()
}
