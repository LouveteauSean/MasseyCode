//! Token spans for the source pane.
//!
//! The module is deliberately presentation-free: it returns byte spans and a
//! token kind, and `app.rs` turns them into GPUI text decorations.  That keeps
//! the highlighting rules testable without a window.

use massey_core::SourceMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// `LOAD`, `STORE`, `NEGATE`, `SHIFT`, `RIGHT`, `LEFT`, `JUMP`, `IF`, `HALT`,
    /// `AND`, `OR`, `XOR`.
    Mnemonic,
    /// `R0`..`RF`.
    Register,
    /// A hexadecimal literal.
    Number,
    /// `[` or `]`.
    Bracket,
    /// `,`, `=`, `+`, `-`.
    Punct,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub token: Token,
}

const MNEMONICS: &[&str] = &[
    "LOAD", "STORE", "NEGATE", "SHIFT", "RIGHT", "LEFT", "JUMP", "IF", "HALT", "AND", "OR", "XOR",
];

fn is_register(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(chars.next(), Some('R' | 'r'))
        && chars.next().is_some_and(|ch| ch.is_ascii_hexdigit())
        && chars.next().is_none()
}

fn is_number(word: &str) -> bool {
    !word.is_empty() && word.len() <= 2 && word.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Token spans for one line of the given view.
pub fn line_spans(mode: SourceMode, line: &str) -> Vec<Span> {
    match mode {
        SourceMode::MachineCode => {
            let trimmed = line.trim();
            if trimmed.len() == 4 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
                let start = line.len() - line.trim_start().len();
                vec![Span {
                    start,
                    end: start + trimmed.len(),
                    token: Token::Number,
                }]
            } else {
                Vec::new()
            }
        }
        SourceMode::Assembly => {
            let bytes = line.as_bytes();
            let mut spans = Vec::new();
            let mut index = 0;
            while index < bytes.len() {
                let ch = bytes[index] as char;
                if ch == '[' || ch == ']' {
                    spans.push(Span {
                        start: index,
                        end: index + 1,
                        token: Token::Bracket,
                    });
                    index += 1;
                    continue;
                }
                if matches!(ch, ',' | '=' | '+' | '-') {
                    spans.push(Span {
                        start: index,
                        end: index + 1,
                        token: Token::Punct,
                    });
                    index += 1;
                    continue;
                }
                if ch.is_ascii_alphanumeric() {
                    let start = index;
                    while index < bytes.len() && (bytes[index] as char).is_ascii_alphanumeric() {
                        index += 1;
                    }
                    let word = &line[start..index];
                    let token = if is_register(word) {
                        Some(Token::Register)
                    } else if MNEMONICS
                        .iter()
                        .any(|keyword| keyword.eq_ignore_ascii_case(word))
                    {
                        Some(Token::Mnemonic)
                    } else if is_number(word) {
                        Some(Token::Number)
                    } else {
                        None
                    };
                    if let Some(token) = token {
                        spans.push(Span {
                            start,
                            end: index,
                            token,
                        });
                    }
                    continue;
                }
                index += 1;
            }
            spans
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembler_words_get_the_right_tokens() {
        let spans = line_spans(SourceMode::Assembly, "LOAD R5, [08]");
        let tokens: Vec<Token> = spans.iter().map(|span| span.token).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Mnemonic,
                Token::Register,
                Token::Punct,
                Token::Bracket,
                Token::Number,
                Token::Bracket
            ]
        );
        assert_eq!(&"LOAD R5, [08]"[spans[0].start..spans[0].end], "LOAD");
        assert_eq!(&"LOAD R5, [08]"[spans[1].start..spans[1].end], "R5");
        assert_eq!(&"LOAD R5, [08]"[spans[4].start..spans[4].end], "08");
    }

    #[test]
    fn xor_is_not_split_into_or() {
        let spans = line_spans(SourceMode::Assembly, "R0 = R1 XOR R2");
        let mnemonics: Vec<&str> = spans
            .iter()
            .filter(|span| span.token == Token::Mnemonic)
            .map(|span| &"R0 = R1 XOR R2"[span.start..span.end])
            .collect();
        assert_eq!(mnemonics, vec!["XOR"]);
    }

    #[test]
    fn machine_code_marks_the_whole_word() {
        let spans = line_spans(SourceMode::MachineCode, "3508");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].token, Token::Number);
        assert_eq!((spans[0].start, spans[0].end), (0, 4));
    }

    #[test]
    fn unknown_words_stay_unstyled() {
        let spans = line_spans(SourceMode::Assembly, "FROB R1, ZZ");
        assert!(spans.iter().all(|span| span.token != Token::Mnemonic));
    }
}
