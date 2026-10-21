//! Syntax highlighting for `.mc` source files.
//!
//! The token kinds are deliberately small: an editor theme only needs to know
//! whether a run of characters is a mnemonic, a register, a label, a number or
//! a comment.

use crate::asm::is_identifier;
use crate::isa::parse_register;

/// The class of a highlighted run of characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// `; comment` and `# comment`.
    Comment,
    /// `START:` — a label definition.
    Label,
    /// `LOAD`, `STORE`, `NEGATE`, `SHIFT`, `JUMP`, `HALT`.
    Mnemonic,
    /// `R0` .. `RF`.
    Register,
    /// `.org`, `.word`, `.equ`, `.data`.
    Directive,
    /// `LEFT`, `RIGHT`, `IF`, `AND`, `OR`, `XOR`, `TO`.
    Keyword,
    /// A hexadecimal number, with or without `0x`/`$`.
    Number,
    /// `=`, `+`, `-`, `*`.
    Operator,
    /// `[`, `]`, `,`, `(`, `)`.
    Punctuation,
    /// Anything else that is not whitespace.
    Plain,
}

impl TokenKind {
    /// True when the token kind should be drawn with the monospace body style.
    pub const fn is_code(self) -> bool {
        !matches!(self, TokenKind::Comment)
    }
}

/// A highlighted span, as byte offsets into the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
    /// What the span is.
    pub kind: TokenKind,
}

impl Token {
    /// The text of the token inside the line it came from.
    pub fn slice<'a>(&self, line: &'a str) -> &'a str {
        &line[self.start..self.end]
    }
}

/// The mnemonics the assembler understands.
pub const MNEMONICS: [&str; 6] = ["LOAD", "STORE", "NEGATE", "SHIFT", "JUMP", "HALT"];

/// The directives the assembler understands.
pub const DIRECTIVES: [&str; 5] = [".org", ".word", ".equ", ".data", ".code"];

/// The reserved words used inside instructions.
pub const KEYWORDS: [&str; 7] = ["LEFT", "RIGHT", "IF", "TO", "AND", "OR", "XOR"];

/// Split a line into highlighted spans.  Offsets are byte offsets into `line`.
pub fn tokenize(line: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let comment_start = line.find([';', '#']);
    let (code, comment) = match comment_start {
        Some(index) => (&line[..index], Some(index)),
        None => (line, None),
    };

    let bytes = code.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        if ch == '$' {
            let start = index;
            index += 1;
            while index < bytes.len() && (bytes[index] as char).is_ascii_hexdigit() {
                index += 1;
            }
            tokens.push(Token {
                start,
                end: index,
                kind: TokenKind::Number,
            });
            continue;
        }
        if ch.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && (bytes[index] as char).is_ascii_alphanumeric() {
                index += 1;
            }
            let text = &code[start..index];
            let lowered = text.to_ascii_lowercase();
            let kind = if lowered.starts_with("0x")
                || lowered.starts_with("0d")
                || text.chars().all(|c| c.is_ascii_hexdigit())
            {
                TokenKind::Number
            } else {
                TokenKind::Plain
            };
            tokens.push(Token {
                start,
                end: index,
                kind,
            });
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' || ch == '.' {
            let start = index;
            while index < bytes.len() {
                let ch = bytes[index] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                    index += 1;
                } else {
                    break;
                }
            }
            let text = &code[start..index];
            let kind = classify_word(code, start, index, text);
            tokens.push(Token {
                start,
                end: index,
                kind,
            });
            continue;
        }
        let kind = match ch {
            '=' | '+' | '-' | '*' => TokenKind::Operator,
            '[' | ']' | ',' | '(' | ')' => TokenKind::Punctuation,
            ':' => TokenKind::Punctuation,
            _ => TokenKind::Plain,
        };
        let start = index;
        index += ch.len_utf8();
        tokens.push(Token {
            start,
            end: index,
            kind,
        });
    }

    if let Some(start) = comment {
        tokens.push(Token {
            start,
            end: line.len(),
            kind: TokenKind::Comment,
        });
    }
    tokens
}

fn classify_word(code: &str, start: usize, end: usize, text: &str) -> TokenKind {
    let upper = text.to_ascii_uppercase();
    if code[end..].starts_with(':') && is_identifier(text) {
        return TokenKind::Label;
    }
    if parse_register(text).is_some() && text.len() == 2 {
        return TokenKind::Register;
    }
    if text.starts_with('.') && DIRECTIVES.iter().any(|d| d.eq_ignore_ascii_case(text)) {
        return TokenKind::Directive;
    }
    if text.starts_with('.') && !is_identifier(text) {
        return TokenKind::Plain;
    }
    if text.starts_with('.') {
        return TokenKind::Directive;
    }
    if MNEMONICS.contains(&upper.as_str()) {
        return TokenKind::Mnemonic;
    }
    if KEYWORDS.contains(&upper.as_str()) {
        return TokenKind::Keyword;
    }
    let _ = start;
    TokenKind::Label
}

/// The tokens of a whole source file, one vector per line.
pub fn tokenize_source(source: &str) -> Vec<Vec<Token>> {
    source.split('\n').map(tokenize).collect()
}

/// Human readable name of a token kind, used in legends and tests.
pub fn token_kind_name(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Comment => "comment",
        TokenKind::Label => "label",
        TokenKind::Mnemonic => "mnemonic",
        TokenKind::Register => "register",
        TokenKind::Directive => "directive",
        TokenKind::Keyword => "keyword",
        TokenKind::Number => "number",
        TokenKind::Operator => "operator",
        TokenKind::Punctuation => "punctuation",
        TokenKind::Plain => "plain",
    }
}
