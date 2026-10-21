//! Context aware completion for `.mc` source files.
//!
//! The engine looks at the text in front of the cursor, works out which operand
//! of which instruction is being typed, and offers exactly the things that are
//! legal there:
//!
//! | position | offered |
//! |---|---|
//! | start of a line | mnemonics, directives |
//! | `LOAD ▮` | registers |
//! | `LOAD R1, ▮` | registers, `[label]`, constants |
//! | `LOAD R1, [▮` | labels and constants that name an address |
//! | `SHIFT R1 ▮` | `LEFT`, `RIGHT` |
//! | `JUMP ▮` | `[label]` |
//! | `JUMP [0A] IF ▮` | registers |
//! | `JUMP [0A] IF R3=▮` | `R0` |
//! | `R1 = R2 ▮` | `+`, `AND`, `OR`, `XOR` |
//!
//! Every entry carries documentation taken from the course notes, so the
//! completion popup doubles as the instruction set reference.

use crate::asm::Program;
use crate::highlight::{tokenize, Token, TokenKind};
use crate::isa::{register_name, Opcode, REGISTER_COUNT};

/// What kind of thing a completion inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompletionKind {
    /// An instruction mnemonic such as `LOAD`.
    Mnemonic,
    /// A directive such as `.word`.
    Directive,
    /// A register such as `R1`.
    Register,
    /// A label defined somewhere in the file.
    Label,
    /// A `.equ` constant.
    Constant,
    /// A reserved word such as `LEFT` or `IF`.
    Keyword,
    /// An operator such as `+` or `AND`.
    Operator,
}

impl CompletionKind {
    /// A short name for the popup's type column.
    pub const fn label(self) -> &'static str {
        match self {
            CompletionKind::Mnemonic => "instruction",
            CompletionKind::Directive => "directive",
            CompletionKind::Register => "register",
            CompletionKind::Label => "label",
            CompletionKind::Constant => "constant",
            CompletionKind::Keyword => "keyword",
            CompletionKind::Operator => "operator",
        }
    }
}

/// One suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Text shown in the list.
    pub label: String,
    /// Text inserted into the document.
    pub insert: String,
    /// What kind of suggestion this is.
    pub kind: CompletionKind,
    /// Short right hand annotation, e.g. the encoding or an address.
    pub detail: String,
    /// Long description, shown beside the list.
    pub documentation: String,
    /// Sorting priority; lower sorts first.
    pub rank: u8,
}

/// Where the cursor is, in terms of the grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionContext {
    /// Nothing typed yet, or a fresh statement after a label.
    Statement,
    /// The first operand: always a register for every MASSEY instruction.
    FirstOperand,
    /// The second operand of a two-operand instruction.
    SecondOperand,
    /// Inside `[` ... `]`: an address.
    Address,
    /// `LEFT` or `RIGHT` after `SHIFT Rx`.
    ShiftDirection,
    /// A bit count after `SHIFT Rx LEFT`.
    ShiftCount,
    /// The register of a `JUMP ... IF Rx=R0` condition.
    Condition,
    /// The `R0` of a `JUMP ... IF Rx=R0` condition.
    ConditionValue,
    /// An arithmetic or logic operator inside an assignment.
    AssignmentOperator,
    /// A register operand of an assignment.
    AssignmentOperand,
    /// A value: an immediate, a `.word` argument or a `.org` argument.
    Value,
    /// The cursor is inside a comment; there is nothing to complete.
    Comment,
}

impl CompletionContext {
    /// A short description of the position, shown in the popup header.
    pub const fn label(self) -> &'static str {
        match self {
            CompletionContext::Statement => "instruction",
            CompletionContext::FirstOperand => "register",
            CompletionContext::SecondOperand => "operand",
            CompletionContext::Address => "address",
            CompletionContext::ShiftDirection => "direction",
            CompletionContext::ShiftCount => "bit count",
            CompletionContext::Condition => "register",
            CompletionContext::ConditionValue => "R0",
            CompletionContext::AssignmentOperator => "operator",
            CompletionContext::AssignmentOperand => "register",
            CompletionContext::Value => "value",
            CompletionContext::Comment => "comment",
        }
    }
}

/// The result of asking for completions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionList {
    /// Suggestions, best first.
    pub items: Vec<Completion>,
    /// Byte column where the replaced text starts.
    pub replace_start: usize,
    /// Byte column where the replaced text ends (the cursor).
    pub replace_end: usize,
    /// The grammatical position the cursor is in.
    pub context: CompletionContext,
}

impl CompletionList {
    /// The text that would be replaced when a suggestion is accepted.
    pub fn replaced<'a>(&self, line: &'a str) -> &'a str {
        let start = self.replace_start.min(line.len());
        let end = self.replace_end.min(line.len()).max(start);
        &line[start..end]
    }

    /// True when there is nothing to show.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The first suggestion, which is the one the popup selects.
    pub fn first(&self) -> Option<&Completion> {
        self.items.first()
    }
}

/// A description of what the cursor is in the middle of typing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The assembler syntax, e.g. `LOAD Rx, [YZ]`.
    pub syntax: String,
    /// The encoding pattern, e.g. `3xYZ`.
    pub encoding: String,
    /// The explanation from the course notes.
    pub description: String,
}

/// Compute the completions available at a cursor position.
///
/// `line` and `column` are zero-based; `column` is a byte offset into the line.
pub fn complete(source: &str, program: &Program, line: usize, column: usize) -> CompletionList {
    let line_text = source.split('\n').nth(line).unwrap_or("");
    let cursor = clamp_to_char_boundary(line_text, column.min(line_text.len()));

    // Inside a comment there is nothing to offer.
    if let Some(comment_start) = line_text.find([';', '#']) {
        if cursor > comment_start {
            return CompletionList {
                items: Vec::new(),
                replace_start: cursor,
                replace_end: cursor,
                context: CompletionContext::Comment,
            };
        }
    }

    let word_start = scan_word_start(line_text, cursor);
    let typed = &line_text[word_start..cursor];
    let mut before = &line_text[..word_start];
    let mut in_bracket = false;
    if before.ends_with('[') {
        in_bracket = true;
        before = &before[..before.len() - 1];
    }

    let tokens: Vec<Token> = tokenize(before)
        .into_iter()
        .filter(|token| {
            token.kind != TokenKind::Punctuation || {
                matches!(token.slice(before), "[" | "]" | "," | "=")
            }
        })
        .collect();

    let context = classify(before, &tokens, in_bracket, typed);
    let mut items = suggestions(context, program, in_bracket, typed);
    items.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| a.label.len().cmp(&b.label.len()))
            .then_with(|| a.label.cmp(&b.label))
    });
    items.dedup_by(|a, b| a.label == b.label && a.insert == b.insert);

    CompletionList {
        items,
        replace_start: word_start,
        replace_end: cursor,
        context,
    }
}

/// Describe the instruction the cursor is inside, for a status bar hint.
pub fn signature(source: &str, line: usize, column: usize) -> Option<SignatureHelp> {
    let line_text = source.split('\n').nth(line)?;
    let cursor = clamp_to_char_boundary(line_text, column.min(line_text.len()));
    let code = match line_text.find([';', '#']) {
        Some(index) if cursor > index => &line_text[..index],
        _ => &line_text[..cursor],
    };
    let upper = code.to_ascii_uppercase();
    let (opcode, _) = Opcode::ALL
        .iter()
        .find(|opcode| {
            let syntax = opcode.syntax();
            let mnemonic = syntax.split_whitespace().next().unwrap_or("");
            !mnemonic.is_empty() && upper.contains(mnemonic) && !syntax.contains(" = ")
        })
        .map(|opcode| (*opcode, ()))?;
    Some(SignatureHelp {
        syntax: opcode.syntax().to_string(),
        encoding: opcode.encoding().to_string(),
        description: opcode.description().to_string(),
    })
}

fn clamp_to_char_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn scan_word_start(line: &str, cursor: usize) -> usize {
    let bytes = line.as_bytes();
    let mut index = cursor;
    while index > 0 {
        let ch = bytes[index - 1] as char;
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '$' {
            index -= 1;
        } else {
            break;
        }
    }
    index
}

/// Words that can appear in front of the cursor, reduced to what matters.
fn classify(before: &str, tokens: &[Token], in_bracket: bool, typed: &str) -> CompletionContext {
    let words: Vec<&str> = tokens.iter().map(|token| token.slice(before)).collect();
    let upper: Vec<String> = words.iter().map(|word| word.to_ascii_uppercase()).collect();

    // A leading `LABEL:` does not change what may follow.
    let start = if upper.len() >= 2 && upper[1] == ":" {
        2
    } else {
        0
    };
    let tail: Vec<&str> = upper[start..].iter().map(|s| s.as_str()).collect();
    let raw_tail: Vec<&str> = words[start..].to_vec();

    if typed.starts_with('.') || tail.iter().any(|word| word.starts_with('.')) {
        // `.org`, `.word`, `.equ`, `.data`
        if let Some(first) = tail.first() {
            if first.eq_ignore_ascii_case(".org") || first.eq_ignore_ascii_case(".word") {
                return CompletionContext::Value;
            }
            if first.eq_ignore_ascii_case(".equ") && tail.len() >= 2 {
                return CompletionContext::Value;
            }
        }
        if typed.starts_with('.') || tail.is_empty() {
            return CompletionContext::Statement;
        }
    }

    if tail.is_empty() {
        return CompletionContext::Statement;
    }

    // Assignment forms:  R1 = R2 + R3 / AND / OR / XOR
    let mut equals_position = None;
    for (index, word) in tail.iter().enumerate() {
        if *word == "=" {
            equals_position = Some(index);
            break;
        }
    }
    if let Some(position) = equals_position {
        let left_is_register = raw_tail
            .first()
            .map(|word| crate::isa::parse_register(word).is_some())
            .unwrap_or(false);
        if left_is_register && !tail.contains(&"JUMP") {
            let after: Vec<&str> = tail[position + 1..].to_vec();
            return match after.len() {
                0 => CompletionContext::AssignmentOperand,
                1 => CompletionContext::AssignmentOperator,
                2 => CompletionContext::AssignmentOperand,
                _ => CompletionContext::AssignmentOperand,
            };
        }
    }

    if let Some(first) = tail.first() {
        match *first {
            "SHIFT" => {
                return match tail.len() {
                    1 => CompletionContext::FirstOperand,
                    2 => CompletionContext::ShiftDirection,
                    _ => CompletionContext::ShiftCount,
                };
            }
            "JUMP" => {
                let has_if = tail.contains(&"IF");
                if !has_if {
                    return CompletionContext::Address;
                }
                let condition: Vec<&str> = tail
                    .iter()
                    .skip_while(|word| **word != "IF")
                    .skip(1)
                    .copied()
                    .collect();
                return match condition.len() {
                    0 => CompletionContext::Condition,
                    1 => {
                        if condition[0] == "=" {
                            CompletionContext::ConditionValue
                        } else {
                            CompletionContext::Condition
                        }
                    }
                    _ => CompletionContext::ConditionValue,
                };
            }
            "LOAD" | "STORE" | "NEGATE" => {
                if in_bracket {
                    return CompletionContext::Address;
                }
                return match tail.len() {
                    1 => CompletionContext::FirstOperand,
                    _ => {
                        if tail.contains(&",") {
                            if *first == "STORE" {
                                CompletionContext::Address
                            } else {
                                CompletionContext::SecondOperand
                            }
                        } else {
                            CompletionContext::FirstOperand
                        }
                    }
                };
            }
            _ => {}
        }
    }

    if in_bracket {
        return CompletionContext::Address;
    }
    // A finished statement means the next thing on the line is a new label or a
    // new instruction, which the caller sees as a statement start.
    if tail.contains(&",") {
        return CompletionContext::SecondOperand;
    }
    CompletionContext::Statement
}

fn suggestions(
    context: CompletionContext,
    program: &Program,
    in_bracket: bool,
    typed: &str,
) -> Vec<Completion> {
    let mut items = Vec::new();
    let needs_bracket = |in_bracket: bool| !in_bracket;
    match context {
        CompletionContext::Comment => {}
        CompletionContext::Statement => {
            for opcode in Opcode::ALL {
                let syntax = opcode.syntax();
                if syntax.contains(" = ") || syntax.is_empty() {
                    continue;
                }
                let mnemonic = syntax.split_whitespace().next().unwrap_or("");
                items.push(Completion {
                    label: mnemonic.to_string(),
                    insert: format!("{mnemonic} "),
                    kind: CompletionKind::Mnemonic,
                    detail: format!("{}  {}", opcode.encoding(), opcode.group().label()),
                    documentation: format!(
                        "**{}**  —  encoding `{}`\n\n{}\n\nC++ equivalent: `{}`",
                        syntax,
                        opcode.encoding(),
                        opcode.description(),
                        opcode.cpp()
                    ),
                    rank: 0,
                });
            }
            // The `Rx = Ry + Rz` family has no mnemonic, so offer it as a
            // template keyed on the register the student types.
            for (template, opcode) in [
                ("R0 = R1 + R2", Opcode::Add),
                ("R0 = R1 AND R2", Opcode::And),
                ("R0 = R1 OR R2", Opcode::Or),
                ("R0 = R1 XOR R2", Opcode::Xor),
            ] {
                items.push(Completion {
                    label: template.to_string(),
                    insert: template.to_string(),
                    kind: CompletionKind::Mnemonic,
                    detail: format!("{}  {}", opcode.encoding(), opcode.group().label()),
                    documentation: format!(
                        "**{}**  —  encoding `{}`\n\n{}\n\nC++ equivalent: `{}`",
                        opcode.syntax(),
                        opcode.encoding(),
                        opcode.description(),
                        opcode.cpp()
                    ),
                    rank: 1,
                });
            }
            for directive in crate::highlight::DIRECTIVES {
                items.push(directive_completion(directive));
            }
            for (name, value) in &program.constants {
                items.push(Completion {
                    label: name.clone(),
                    insert: name.clone(),
                    kind: CompletionKind::Constant,
                    detail: format!("= {value:X}"),
                    documentation: format!(
                        "Constant `{name}` defined with `.equ`, value {value:X}."
                    ),
                    rank: 3,
                });
            }
        }
        CompletionContext::FirstOperand
        | CompletionContext::SecondOperand
        | CompletionContext::AssignmentOperand
        | CompletionContext::Condition => {
            items.extend(register_completions());
            if context == CompletionContext::SecondOperand {
                items.extend(label_completions(program, needs_bracket(in_bracket)));
                items.extend(constant_completions(program));
            }
        }
        CompletionContext::Address => {
            items.extend(label_completions(program, needs_bracket(in_bracket)));
            items.extend(constant_completions(program));
            items.push(Completion {
                label: "00..FF".to_string(),
                insert: if in_bracket {
                    "]".to_string()
                } else {
                    "[00]".to_string()
                },
                kind: CompletionKind::Keyword,
                detail: "address".to_string(),
                documentation:
                    "The MASSEY machine has 256 memory locations, addressed 00 to FF in \
                     hexadecimal."
                        .to_string(),
                rank: 5,
            });
        }
        CompletionContext::ShiftDirection => {
            for direction in ["LEFT", "RIGHT"] {
                items.push(Completion {
                    label: direction.to_string(),
                    insert: format!("{direction} "),
                    kind: CompletionKind::Keyword,
                    detail: if direction == "LEFT" { "9xYZ" } else { "8xYZ" }.to_string(),
                    documentation: format!(
                        "`SHIFT Rx {direction} YZ` shifts the number in Rx {direction} by YZ bits. \
                         C++ equivalent: `Rx = Rx {} 0xYZ;`",
                        if direction == "LEFT" { "<<" } else { ">>" }
                    ),
                    rank: 0,
                });
            }
        }
        CompletionContext::ShiftCount => {
            for count in [1u16, 2, 4, 8] {
                items.push(Completion {
                    label: format!("{count:X}"),
                    insert: format!("{count:X}"),
                    kind: CompletionKind::Keyword,
                    detail: format!("shift by {count} bit{}", if count == 1 { "" } else { "s" }),
                    documentation: format!(
                        "Shift by {count} bit{}.  The count is hexadecimal and must be small: \
                         registers are 16 bits wide, so shifting by 10 (16) or more clears the \
                         register.",
                        if count == 1 { "" } else { "s" }
                    ),
                    rank: 2,
                });
            }
        }
        CompletionContext::ConditionValue => {
            items.push(Completion {
                label: "R0".to_string(),
                insert: "R0".to_string(),
                kind: CompletionKind::Register,
                detail: "compare with R0".to_string(),
                documentation:
                    "A MASSEY JUMP can only compare a register with R0.  Keep R0 at zero (many \
                     programs start with `LOAD R0, 00`) and a JUMP tests for equality with zero."
                        .to_string(),
                rank: 0,
            });
        }
        CompletionContext::AssignmentOperator => {
            for (keyword, opcode) in [
                ("+", Opcode::Add),
                ("AND", Opcode::And),
                ("OR", Opcode::Or),
                ("XOR", Opcode::Xor),
            ] {
                items.push(Completion {
                    label: keyword.to_string(),
                    insert: format!("{keyword} "),
                    kind: CompletionKind::Operator,
                    detail: opcode.encoding().to_string(),
                    documentation: format!(
                        "`{}`  —  {}\n\nC++ equivalent: `{}`",
                        opcode.syntax(),
                        opcode.description(),
                        opcode.cpp()
                    ),
                    rank: 0,
                });
            }
        }
        CompletionContext::Value => {
            items.extend(constant_completions(program));
            items.extend(label_completions(program, false));
            for (label, detail) in [("0x00", "zero"), ("0x01", "one"), ("0000", "16-bit zero")] {
                items.push(Completion {
                    label: label.to_string(),
                    insert: label.to_string(),
                    kind: CompletionKind::Keyword,
                    detail: detail.to_string(),
                    documentation: "Values in `.mc` files are hexadecimal, as in the course notes."
                        .to_string(),
                    rank: 4,
                });
            }
        }
    }

    if !typed.is_empty() {
        let prefix = typed.to_ascii_lowercase();
        items.retain(|item| {
            item.label.to_ascii_lowercase().starts_with(&prefix)
                || item.insert.to_ascii_lowercase().starts_with(&prefix)
        });
    }
    items
}

fn register_completions() -> Vec<Completion> {
    (0..REGISTER_COUNT as u8)
        .map(|index| {
            let name = register_name(index);
            Completion {
                label: name.clone(),
                insert: name.clone(),
                kind: CompletionKind::Register,
                detail: format!("R{index:X}  ({index})"),
                documentation: register_documentation(index),
                rank: 0,
            }
        })
        .collect()
}

fn register_documentation(index: u8) -> String {
    let name = register_name(index);
    let mut text = format!(
        "**{name}** — one of the 16 registers of the MASSEY machine, 16 bits wide, numbered in \
         hexadecimal as {name} (register {index} in decimal)."
    );
    if index == 0 {
        text.push_str(
            "\n\n**R0 is special**: every JUMP compares a register with R0, so programs normally \
             load zero into R0 at the start (`LOAD R0, 00`) and use `IF R0=R0` to jump always.",
        );
    }
    if index == 15 {
        text.push_str("\n\nRF is often used to hold the constant 1.");
    }
    text
}

fn label_completions(program: &Program, wrap_in_brackets: bool) -> Vec<Completion> {
    program
        .labels
        .iter()
        .map(|label| {
            let insert = if wrap_in_brackets {
                format!("[{}]", label.name)
            } else {
                label.name.clone()
            };
            Completion {
                label: label.name.clone(),
                insert,
                kind: CompletionKind::Label,
                detail: format!("{:02X}", label.address),
                documentation: format!(
                    "Label `{}` names memory address {:02X} (defined on line {}).",
                    label.name,
                    label.address,
                    label.line + 1
                ),
                rank: if wrap_in_brackets { 1 } else { 0 },
            }
        })
        .collect()
}

fn constant_completions(program: &Program) -> Vec<Completion> {
    program
        .constants
        .iter()
        .map(|(name, value)| Completion {
            label: name.clone(),
            insert: name.clone(),
            kind: CompletionKind::Constant,
            detail: format!("= {value:04X}"),
            documentation: format!(
                "Constant `{name}`, defined with `.equ`, has the value {value:04X} hexadecimal."
            ),
            rank: 1,
        })
        .collect()
}

fn directive_completion(directive: &str) -> Completion {
    let (detail, documentation) = match directive {
        ".org" => (
            "set the next address",
            "`.org YZ` sets the address that the next word will be placed at.  Useful for \
             reserving a data area after the code, e.g. `.org 40`.",
        ),
        ".word" => (
            "16-bit data word",
            "`.word YZ` places one 16-bit word in memory.  Use it for the data your program \
             works on, e.g. `COUNT: .word 0000`.",
        ),
        ".equ" => (
            "define a constant",
            "`.equ NAME value` defines a constant that can be used anywhere a value or address \
             is expected.",
        ),
        ".data" => (
            "following words are data",
            "`.data` marks the rest of the file as data.  It does not change the machine code, \
             it only tells the listing which lines are data.",
        ),
        _ => (
            "following words are code",
            "`.code` marks the rest of the file as instructions.",
        ),
    };
    Completion {
        label: directive.to_string(),
        insert: format!("{directive} "),
        kind: CompletionKind::Directive,
        detail: detail.to_string(),
        documentation: documentation.to_string(),
        rank: 2,
    }
}

/// Complete a partial line the way a teaching tool should: if the line is a bare
/// mnemonic, show the full syntax.
pub fn line_hint(program: &Program, line: usize) -> Option<String> {
    let info = program.lines.get(line)?;
    match (info.address, info.word) {
        (Some(address), Some(word)) => Some(format!(
            "{address:02X}  {word:04X}  {}",
            crate::disasm::disassemble(word)
        )),
        _ => None,
    }
}
