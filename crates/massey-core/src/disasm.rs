//! Disassembly: turning machine code back into the assembler notation used in
//! the 159.102 course notes.
//!
//! The operand layout differs between opcodes (`1xYZ`, `20xy`, `70x0`, ...), so
//! every opcode is spelled out explicitly.

use crate::isa::{format_byte, register_name, Instruction, Opcode};

/// Disassemble one machine word into assembler source.
///
/// Unknown opcodes disassemble to a `???` comment so that listing panes can
/// show every memory word without lying about what the machine would do.
pub fn disassemble(word: u16) -> String {
    let instruction = Instruction::from_word(word);
    let yz = format_byte(instruction.yz());
    match instruction.opcode() {
        Some(Opcode::LoadImmediate) => {
            format!("LOAD {}, {}", register_name(instruction.x()), yz)
        }
        Some(Opcode::LoadRegister) => format!(
            "LOAD {}, {}",
            register_name(instruction.y()),
            register_name(instruction.z())
        ),
        Some(Opcode::LoadMemory) => {
            format!("LOAD {}, [{}]", register_name(instruction.x()), yz)
        }
        Some(Opcode::StoreMemory) => {
            format!("STORE {}, [{}]", register_name(instruction.x()), yz)
        }
        Some(Opcode::Add) => format!(
            "{} = {} + {}",
            register_name(instruction.x()),
            register_name(instruction.y()),
            register_name(instruction.z())
        ),
        Some(Opcode::Negate) => format!("NEGATE {}", register_name(instruction.y())),
        Some(Opcode::ShiftRight) => {
            format!("SHIFT {} RIGHT {}", register_name(instruction.x()), yz)
        }
        Some(Opcode::ShiftLeft) => {
            format!("SHIFT {} LEFT {}", register_name(instruction.x()), yz)
        }
        Some(Opcode::And) => format!(
            "{} = {} AND {}",
            register_name(instruction.x()),
            register_name(instruction.y()),
            register_name(instruction.z())
        ),
        Some(Opcode::Or) => format!(
            "{} = {} OR {}",
            register_name(instruction.x()),
            register_name(instruction.y()),
            register_name(instruction.z())
        ),
        Some(Opcode::Xor) => format!(
            "{} = {} XOR {}",
            register_name(instruction.x()),
            register_name(instruction.y()),
            register_name(instruction.z())
        ),
        Some(Opcode::Jump) => format!("JUMP [{}] IF {}=R0", yz, register_name(instruction.x())),
        Some(Opcode::Halt) => "HALT".to_string(),
        None => format!("??? (not a MASSEY opcode) ; {word:04X}"),
    }
}

/// Disassemble a word, preferring a label when one is known for the address it
/// refers to.  `label_at` returns the label defined at an address, if any.
pub fn disassemble_labeled(word: u16, label_at: impl Fn(u8) -> Option<String>) -> String {
    let instruction = Instruction::from_word(word);
    match instruction.opcode() {
        Some(Opcode::LoadMemory) | Some(Opcode::StoreMemory) | Some(Opcode::Jump) => {
            match label_at(instruction.yz()) {
                Some(label) => {
                    let base = disassemble(word);
                    // Replace the first bracketed operand with the label.
                    match (base.find('['), base.find(']')) {
                        (Some(open), Some(close)) if close > open => {
                            format!("{}[{}]{}", &base[..open], label, &base[close + 1..])
                        }
                        _ => base,
                    }
                }
                None => disassemble(word),
            }
        }
        _ => disassemble(word),
    }
}

/// The register an instruction writes, or `None` when it writes none.
pub fn written_register(word: u16) -> Option<u8> {
    let instruction = Instruction::from_word(word);
    match instruction.opcode()? {
        Opcode::LoadImmediate
        | Opcode::LoadMemory
        | Opcode::Add
        | Opcode::ShiftRight
        | Opcode::ShiftLeft
        | Opcode::And
        | Opcode::Or
        | Opcode::Xor => Some(instruction.x()),
        Opcode::LoadRegister | Opcode::Negate => Some(instruction.y()),
        Opcode::StoreMemory | Opcode::Jump | Opcode::Halt => None,
    }
}

/// The register an instruction reads but does not write.
pub fn read_register(word: u16) -> Option<u8> {
    let instruction = Instruction::from_word(word);
    match instruction.opcode()? {
        Opcode::StoreMemory | Opcode::Jump => Some(instruction.x()),
        _ => None,
    }
}

/// The register the given instruction acts on, used to highlight a row in the
/// register pane: the written register when there is one, otherwise the read
/// register.
pub fn active_register(word: u16) -> Option<u8> {
    written_register(word).or_else(|| read_register(word))
}

/// Registers read by an instruction (operands only, not the written register).
pub fn source_registers(word: u16) -> Vec<u8> {
    let instruction = Instruction::from_word(word);
    match instruction.opcode() {
        Some(Opcode::LoadRegister) => vec![instruction.z()],
        Some(Opcode::Add) | Some(Opcode::And) | Some(Opcode::Or) | Some(Opcode::Xor) => {
            vec![instruction.y(), instruction.z()]
        }
        Some(Opcode::StoreMemory) | Some(Opcode::Jump) => vec![instruction.x()],
        Some(Opcode::Negate) => vec![instruction.y()],
        Some(Opcode::LoadImmediate)
        | Some(Opcode::LoadMemory)
        | Some(Opcode::ShiftRight)
        | Some(Opcode::ShiftLeft) => vec![instruction.x()],
        Some(Opcode::Halt) | None => vec![],
    }
}

/// The memory address an instruction touches, if any.
pub fn memory_operand(word: u16) -> Option<u8> {
    let instruction = Instruction::from_word(word);
    match instruction.opcode()? {
        Opcode::LoadMemory | Opcode::StoreMemory => Some(instruction.yz()),
        _ => None,
    }
}

/// Disassemble a whole program image into assembler source.
///
/// The result assembles back to exactly the same image: one line per memory
/// word, in order, with no `.org` directives.  Labels are invented for every
/// address the program refers to, so a machine code listing becomes readable
/// assembly:
///
/// ```text
/// ; Disassembled from 5 words of machine code.
/// ; Labels were generated for the addresses this program refers to.
///
/// L00:    LOAD  R5, [L08]        ; 00  3508
///         LOAD  R6, [L09]        ; 01  3609
///         R0 = R5 + R6           ; 02  6056
///         STORE R0, [L0A]        ; 03  400A
///         HALT                   ; 04  E000
///         .word 0000             ; 05  0000
/// ```
///
/// Words whose first hexadecimal digit is not a MASSEY opcode cannot be
/// instructions, so they are written as `.word` data.
pub fn disassemble_image(words: &[u16]) -> String {
    use std::collections::BTreeSet;

    // Which addresses does this program talk about?
    let mut referenced: BTreeSet<u8> = BTreeSet::new();
    referenced.insert(0);
    for word in words {
        let instruction = Instruction::from_word(*word);
        match instruction.opcode() {
            Some(Opcode::Jump) => {
                referenced.insert(instruction.yz());
            }
            Some(Opcode::LoadMemory) | Some(Opcode::StoreMemory) => {
                referenced.insert(instruction.yz());
            }
            _ => {}
        }
    }

    let label_for = |address: u8| format!("L{address:02X}");
    // Only addresses that are actually inside the image can carry a label; an
    // operand that points past the end of the listing stays hexadecimal.
    let name_for = |address: u8| {
        if (address as usize) < words.len() {
            label_for(address)
        } else {
            format!("{address:02X}")
        }
    };

    let mut out = String::new();
    out.push_str(&format!(
        "; Disassembled from {} word{} of machine code.\n",
        words.len(),
        if words.len() == 1 { "" } else { "s" }
    ));
    out.push_str(
        "; Labels were generated for the addresses this program refers to.\n\
         ; Edit this assembly and press Assemble to build a new machine code image.\n\n",
    );

    for (index, word) in words.iter().enumerate() {
        let address = index as u8;
        let instruction = Instruction::from_word(*word);
        let label = if referenced.contains(&address) && (address as usize) < words.len() {
            format!("{}:", label_for(address))
        } else {
            String::new()
        };
        let text = match instruction.opcode() {
            Some(Opcode::Jump) => format!(
                "JUMP  [{}] IF {}=R0",
                name_for(instruction.yz()),
                register_name(instruction.x())
            ),
            Some(Opcode::LoadMemory) => format!(
                "LOAD  {}, [{}]",
                register_name(instruction.x()),
                name_for(instruction.yz())
            ),
            Some(Opcode::StoreMemory) => format!(
                "STORE {}, [{}]",
                register_name(instruction.x()),
                name_for(instruction.yz())
            ),
            Some(_) => disassemble(*word),
            None => format!(".word {word:04X}"),
        };
        out.push_str(&format!(
            "{:<8}{:<26}; {address:02X}  {word:04X}\n",
            label, text
        ));
    }
    out
}
