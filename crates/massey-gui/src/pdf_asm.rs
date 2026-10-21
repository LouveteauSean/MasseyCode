//! The PDF-only MASSEY assembler notation.
//!
//! This is the notation from Section 5 of the 159.102 notes, and nothing else:
//! one instruction per line, literal hexadecimal operands, no labels, no
//! directives, no comments.  Each line is one 16-bit word.

use massey_core::{disassemble, parse_register, Instruction, Opcode};

/// Parse one PDF assembler line.
///
/// A blank line is not an instruction: it means the word at that address has
/// no assembler spelling (usually data), so the caller keeps the existing word.
/// Everything else must be one of the thirteen forms in the notes.
pub fn parse_line(line: &str) -> Result<Option<u16>, String> {
    let text = line.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.starts_with(';') || text.starts_with('#') {
        return Err("comments are not part of the PDF assembler".to_string());
    }
    if text.contains(':') {
        return Err("labels are not part of the PDF assembler".to_string());
    }
    if text.starts_with('.') {
        return Err("directives are not part of the PDF assembler".to_string());
    }

    let upper = text.to_ascii_uppercase();
    if upper == "HALT" {
        return Ok(Some(0xE000));
    }
    if let Some(rest) = strip_word(&upper, "LOAD") {
        return parse_load(rest).map(Some);
    }
    if let Some(rest) = strip_word(&upper, "STORE") {
        return parse_store(rest).map(Some);
    }
    if let Some(rest) = strip_word(&upper, "NEGATE") {
        let register = parse_register(rest.trim())
            .ok_or_else(|| "NEGATE needs a register such as R2".to_string())?;
        return Ok(Some(
            Instruction::new(Opcode::Negate, 0, register, 0).word(),
        ));
    }
    if let Some(rest) = strip_word(&upper, "SHIFT") {
        return parse_shift(rest).map(Some);
    }
    if let Some(rest) = strip_word(&upper, "JUMP") {
        return parse_jump(rest).map(Some);
    }
    parse_assignment(text).map(Some)
}

/// The assembler spelling the PDF gives to a word, or `None` for data and
/// illegal opcodes (the PDF leaves those rows of the Assembler column empty,
/// except for an unknown instruction, which it writes as `????`).
pub fn disassemble_line(word: u16) -> Option<String> {
    Instruction::from_word(word)
        .opcode()
        .map(|_| disassemble(word))
}

fn strip_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    let first = rest.chars().next()?;
    if !first.is_whitespace() {
        return None;
    }
    Some(rest.trim_start())
}

fn parse_load(rest: &str) -> Result<u16, String> {
    let (register, operand) = split_register_operand(rest, "LOAD")?;
    if let Some(inner) = bracket(operand) {
        let address = parse_byte(inner)?;
        return Ok(
            Instruction::new(Opcode::LoadMemory, register, address >> 4, address & 0x0F).word(),
        );
    }
    if let Some(source) = parse_register(operand) {
        return Ok(Instruction::new(Opcode::LoadRegister, 0, register, source).word());
    }
    let value = parse_byte(operand)?;
    Ok(Instruction::new(Opcode::LoadImmediate, register, value >> 4, value & 0x0F).word())
}

fn parse_store(rest: &str) -> Result<u16, String> {
    let (register, operand) = split_register_operand(rest, "STORE")?;
    let inner =
        bracket(operand).ok_or_else(|| "STORE needs an address such as [0A]".to_string())?;
    let address = parse_byte(inner)?;
    Ok(Instruction::new(Opcode::StoreMemory, register, address >> 4, address & 0x0F).word())
}

fn parse_shift(rest: &str) -> Result<u16, String> {
    let mut parts = rest.split_whitespace();
    let register = parts
        .next()
        .and_then(parse_register)
        .ok_or_else(|| "SHIFT needs a register such as R1".to_string())?;
    let direction = parts
        .next()
        .ok_or_else(|| "SHIFT needs LEFT or RIGHT".to_string())?;
    let amount = parts
        .next()
        .ok_or_else(|| "SHIFT needs a bit count".to_string())?;
    if parts.next().is_some() {
        return Err("too many operands after SHIFT".to_string());
    }
    let opcode = match direction {
        "LEFT" => Opcode::ShiftLeft,
        "RIGHT" => Opcode::ShiftRight,
        _ => return Err("SHIFT direction must be LEFT or RIGHT".to_string()),
    };
    let amount = parse_byte(amount)?;
    Ok(Instruction::new(opcode, register, amount >> 4, amount & 0x0F).word())
}

fn parse_jump(rest: &str) -> Result<u16, String> {
    let rest = rest.trim();
    let close = rest
        .find(']')
        .ok_or_else(|| "JUMP needs an address such as [0E]".to_string())?;
    if !rest.starts_with('[') {
        return Err("JUMP address must be in brackets".to_string());
    }
    let address = parse_byte(rest[1..close].trim())?;
    let condition = rest[close + 1..].trim();
    let condition = condition
        .strip_prefix("IF")
        .ok_or_else(|| "JUMP needs `IF Rx=R0`".to_string())?
        .trim();
    let (left, right) = condition
        .split_once('=')
        .ok_or_else(|| "JUMP condition must be `Rx=R0`".to_string())?;
    let register = parse_register(left.trim())
        .ok_or_else(|| "JUMP condition needs a register on the left".to_string())?;
    if parse_register(right.trim()) != Some(0) {
        return Err("JUMP can only compare with R0".to_string());
    }
    Ok(Instruction::new(Opcode::Jump, register, address >> 4, address & 0x0F).word())
}

fn parse_assignment(text: &str) -> Result<u16, String> {
    let (left, right) = text
        .split_once('=')
        .ok_or_else(|| "not a PDF assembler instruction".to_string())?;
    let destination = parse_register(left.trim())
        .ok_or_else(|| "assignment needs a register on the left".to_string())?;
    let right = right.trim();
    let (opcode, operator) = if let Some((a, b)) = right.split_once('+') {
        (Opcode::Add, (a, b))
    } else if let Some((a, b)) = split_keyword(right, "AND") {
        (Opcode::And, (a, b))
    } else if let Some((a, b)) = split_keyword(right, "XOR") {
        (Opcode::Xor, (a, b))
    } else if let Some((a, b)) = split_keyword(right, "OR") {
        (Opcode::Or, (a, b))
    } else {
        return Err("assignment must be `Rx = Ry +/AND/OR/XOR Rz`".to_string());
    };
    let source_a = parse_register(operator.0.trim())
        .ok_or_else(|| "assignment needs a register as the first operand".to_string())?;
    let source_b = parse_register(operator.1.trim())
        .ok_or_else(|| "assignment needs a register as the second operand".to_string())?;
    Ok(Instruction::new(opcode, destination, source_a, source_b).word())
}

fn split_register_operand<'a>(rest: &'a str, name: &str) -> Result<(u8, &'a str), String> {
    let (register, operand) = rest
        .split_once(',')
        .ok_or_else(|| format!("{name} needs a register and an operand, e.g. {name} R1, 08"))?;
    let register = parse_register(register.trim())
        .ok_or_else(|| format!("{name} needs a register such as R1"))?;
    Ok((register, operand.trim()))
}

fn bracket(text: &str) -> Option<&str> {
    text.strip_prefix('[')?.strip_suffix(']')
}

fn split_keyword<'a>(text: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let upper = text.to_ascii_uppercase();
    let index = upper.find(keyword)?;
    let end = index + keyword.len();
    Some((&text[..index], &text[end..]))
}

fn parse_byte(text: &str) -> Result<u8, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 2 || !text.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!(
            "`{text}` is not a one or two digit hexadecimal value"
        ));
    }
    u8::from_str_radix(text, 16).map_err(|_| format!("`{text}` is not a hexadecimal value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(line: &str) -> u16 {
        parse_line(line).unwrap().unwrap()
    }

    #[test]
    fn every_pdf_form_encodes_to_the_notes() {
        assert_eq!(word("LOAD R5, [08]"), 0x3508);
        assert_eq!(word("LOAD R6, [09]"), 0x3609);
        assert_eq!(word("LOAD R1, FC"), 0x11FC);
        assert_eq!(word("LOAD R2, R1"), 0x2021);
        assert_eq!(word("STORE R0, [0A]"), 0x400A);
        assert_eq!(word("R0 = R5 + R6"), 0x6056);
        assert_eq!(word("NEGATE R2"), 0x7020);
        assert_eq!(word("SHIFT R0 LEFT 8"), 0x9008);
        assert_eq!(word("SHIFT R6 RIGHT 4"), 0x8604);
        assert_eq!(word("R3 = R1 AND R2"), 0xA312);
        assert_eq!(word("R3 = R1 OR R2"), 0xB312);
        assert_eq!(word("R3 = R1 XOR R2"), 0xC312);
        assert_eq!(word("JUMP [0E] IF R3=R0"), 0xD30E);
        assert_eq!(word("HALT"), 0xE000);
    }

    #[test]
    fn blank_is_a_data_row_not_an_instruction() {
        assert_eq!(parse_line("   ").unwrap(), None);
    }

    #[test]
    fn extensions_are_rejected() {
        assert!(parse_line("LOOP: LOAD R1, 00").is_err());
        assert!(parse_line(".org 08").is_err());
        assert!(parse_line("LOAD R1, [NUM1]").is_err());
        assert!(parse_line("LOAD R1, 00 ; comment").is_err());
        assert!(parse_line("???").is_err());
    }

    #[test]
    fn disassembly_is_the_pdf_spelling() {
        assert_eq!(disassemble_line(0x3508).as_deref(), Some("LOAD R5, [08]"));
        assert_eq!(disassemble_line(0x6056).as_deref(), Some("R0 = R5 + R6"));
        assert_eq!(
            disassemble_line(0xD30E).as_deref(),
            Some("JUMP [0E] IF R3=R0")
        );
        assert_eq!(disassemble_line(0x001B), None);
    }
}
