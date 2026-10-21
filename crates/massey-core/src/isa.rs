//! The MASSEY machine instruction set.
//!
//! Every fact in this module comes straight from the 159.102 course notes
//! (Section 4 and Section 5).  An instruction is 16 bits, written as four
//! hexadecimal digits: the first digit is the opcode, the other three digits
//! are the operands.
//!
//! ```text
//! 1xYZ   LOAD Rx, YZ            load Rx with the value YZ
//! 20xy   LOAD Rx, Ry            load Rx with the value in Ry
//! 3xYZ   LOAD Rx, [YZ]          load Rx with memory[YZ]
//! 4xYZ   STORE Rx, [YZ]         store Rx in memory[YZ]
//! 6xyz   Rx = Ry + Rz           add (2's complement)
//! 70x0   NEGATE Rx              Rx = -Rx
//! 8xYZ   SHIFT Rx RIGHT YZ      Rx = Rx >> YZ bits
//! 9xYZ   SHIFT Rx LEFT YZ       Rx = Rx << YZ bits
//! Axyz   Rx = Ry AND Rz         bitwise AND
//! Bxyz   Rx = Ry OR Rz          bitwise OR
//! Cxyz   Rx = Ry XOR Rz         bitwise XOR
//! DxYZ   JUMP [YZ] IF Rx=R0     load YZ into the PC if Rx equals R0
//! E000   HALT                   stop the machine
//! ```
//!
//! Opcodes `0` and `5` are deliberately unused: the course notes never define
//! them, so the machine reports them as illegal instructions.

use std::fmt;

/// Memory locations in the MASSEY machine (addresses `00`..`FF`).
pub const MEMORY_WORDS: usize = 256;
/// Registers in the MASSEY machine, `R0`..`RF`.
pub const REGISTER_COUNT: usize = 16;
/// Every machine instruction occupies exactly one 16-bit memory location.
pub const WORD_BITS: u32 = 16;

/// The opcode of a MASSEY machine instruction.
///
/// `0` and `5` are absent on purpose; `Opcode::from_nibble` returns `None` for
/// them and for any other value that is not an opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Opcode {
    /// `1xYZ` — `LOAD Rx, YZ`.
    LoadImmediate,
    /// `20xy` — `LOAD Rx, Ry`.
    LoadRegister,
    /// `3xYZ` — `LOAD Rx, [YZ]`.
    LoadMemory,
    /// `4xYZ` — `STORE Rx, [YZ]`.
    StoreMemory,
    /// `6xyz` — `Rx = Ry + Rz`.
    Add,
    /// `70x0` — `NEGATE Rx`.
    Negate,
    /// `8xYZ` — `SHIFT Rx RIGHT YZ`.
    ShiftRight,
    /// `9xYZ` — `SHIFT Rx LEFT YZ`.
    ShiftLeft,
    /// `Axyz` — `Rx = Ry AND Rz`.
    And,
    /// `Bxyz` — `Rx = Ry OR Rz`.
    Or,
    /// `Cxyz` — `Rx = Ry XOR Rz`.
    Xor,
    /// `DxYZ` — `JUMP [YZ] IF Rx=R0`.
    Jump,
    /// `E000` — `HALT`.
    Halt,
}

impl Opcode {
    /// Every opcode, in encoding order.
    pub const ALL: [Opcode; 13] = [
        Opcode::LoadImmediate,
        Opcode::LoadRegister,
        Opcode::LoadMemory,
        Opcode::StoreMemory,
        Opcode::Add,
        Opcode::Negate,
        Opcode::ShiftRight,
        Opcode::ShiftLeft,
        Opcode::And,
        Opcode::Or,
        Opcode::Xor,
        Opcode::Jump,
        Opcode::Halt,
    ];

    /// Decode the first hexadecimal digit of an instruction.
    pub fn from_nibble(nibble: u8) -> Option<Opcode> {
        Some(match nibble & 0x0F {
            0x1 => Opcode::LoadImmediate,
            0x2 => Opcode::LoadRegister,
            0x3 => Opcode::LoadMemory,
            0x4 => Opcode::StoreMemory,
            0x6 => Opcode::Add,
            0x7 => Opcode::Negate,
            0x8 => Opcode::ShiftRight,
            0x9 => Opcode::ShiftLeft,
            0xA => Opcode::And,
            0xB => Opcode::Or,
            0xC => Opcode::Xor,
            0xD => Opcode::Jump,
            0xE => Opcode::Halt,
            _ => return None,
        })
    }

    /// The opcode nibble used in an encoded instruction.
    pub const fn nibble(self) -> u8 {
        match self {
            Opcode::LoadImmediate => 0x1,
            Opcode::LoadRegister => 0x2,
            Opcode::LoadMemory => 0x3,
            Opcode::StoreMemory => 0x4,
            Opcode::Add => 0x6,
            Opcode::Negate => 0x7,
            Opcode::ShiftRight => 0x8,
            Opcode::ShiftLeft => 0x9,
            Opcode::And => 0xA,
            Opcode::Or => 0xB,
            Opcode::Xor => 0xC,
            Opcode::Jump => 0xD,
            Opcode::Halt => 0xE,
        }
    }

    /// The assembler mnemonic, or `""` for the `Rx = Ry + Rz` family which has
    /// no mnemonic in the course notes.
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Opcode::LoadImmediate | Opcode::LoadRegister | Opcode::LoadMemory => "LOAD",
            Opcode::StoreMemory => "STORE",
            Opcode::Negate => "NEGATE",
            Opcode::ShiftRight => "SHIFT",
            Opcode::ShiftLeft => "SHIFT",
            Opcode::Jump => "JUMP",
            Opcode::Halt => "HALT",
            Opcode::Add | Opcode::And | Opcode::Or | Opcode::Xor => "",
        }
    }

    /// The encoding pattern as printed in the course notes, e.g. `1xYZ`.
    pub const fn encoding(self) -> &'static str {
        match self {
            Opcode::LoadImmediate => "1xYZ",
            Opcode::LoadRegister => "20xy",
            Opcode::LoadMemory => "3xYZ",
            Opcode::StoreMemory => "4xYZ",
            Opcode::Add => "6xyz",
            Opcode::Negate => "70x0",
            Opcode::ShiftRight => "8xYZ",
            Opcode::ShiftLeft => "9xYZ",
            Opcode::And => "Axyz",
            Opcode::Or => "Bxyz",
            Opcode::Xor => "Cxyz",
            Opcode::Jump => "DxYZ",
            Opcode::Halt => "E000",
        }
    }

    /// The assembler form as printed in the course notes, e.g. `LOAD Rx, YZ`.
    pub const fn syntax(self) -> &'static str {
        match self {
            Opcode::LoadImmediate => "LOAD Rx, YZ",
            Opcode::LoadRegister => "LOAD Rx, Ry",
            Opcode::LoadMemory => "LOAD Rx, [YZ]",
            Opcode::StoreMemory => "STORE Rx, [YZ]",
            Opcode::Add => "Rx = Ry + Rz",
            Opcode::Negate => "NEGATE Rx",
            Opcode::ShiftRight => "SHIFT Rx RIGHT YZ",
            Opcode::ShiftLeft => "SHIFT Rx LEFT YZ",
            Opcode::And => "Rx = Ry AND Rz",
            Opcode::Or => "Rx = Ry OR Rz",
            Opcode::Xor => "Rx = Ry XOR Rz",
            Opcode::Jump => "JUMP [YZ] IF Rx=R0",
            Opcode::Halt => "HALT",
        }
    }

    /// The one-line explanation from the course notes.
    pub const fn description(self) -> &'static str {
        match self {
            Opcode::LoadImmediate => "Load register Rx with the value YZ (2 hexadecimal digits).",
            Opcode::LoadRegister => "Load register Rx with the number in register Ry.",
            Opcode::LoadMemory => "Load register Rx with the number in memory location YZ.",
            Opcode::StoreMemory => "Store the number in register Rx in memory location YZ.",
            Opcode::Add => "Add Ry and Rz and load the result into Rx (2's complement addition).",
            Opcode::Negate => "Negate register Rx: complement and add 1, i.e. Rx = -Rx.",
            Opcode::ShiftRight => "Shift the number in register Rx right by YZ bits.",
            Opcode::ShiftLeft => "Shift the number in register Rx left by YZ bits.",
            Opcode::And => "Bitwise AND Ry and Rz, load the result into Rx.",
            Opcode::Or => "Bitwise OR Ry and Rz, load the result into Rx.",
            Opcode::Xor => "Bitwise XOR Ry and Rz, load the result into Rx.",
            Opcode::Jump => {
                "Jump to the instruction at address YZ if the value in Rx equals the value in R0."
            }
            Opcode::Halt => "Halt the machine.",
        }
    }

    /// The equivalent C++ statement from the course notes' comparison table.
    pub const fn cpp(self) -> &'static str {
        match self {
            Opcode::LoadImmediate => "Rx = 0xYZ;",
            Opcode::LoadRegister => "Rx = Ry;",
            Opcode::LoadMemory => "Rx = memory[YZ];",
            Opcode::StoreMemory => "memory[YZ] = Rx;",
            Opcode::Add => "Rx = Ry + Rz;",
            Opcode::Negate => "Rx = -Rx;",
            Opcode::ShiftRight => "Rx = Rx >> 0xYZ;",
            Opcode::ShiftLeft => "Rx = Rx << 0xYZ;",
            Opcode::And => "Rx = Ry & Rz;",
            Opcode::Or => "Rx = Ry | Rz;",
            Opcode::Xor => "Rx = Ry ^ Rz;",
            Opcode::Jump => "if (Rx == R0) goto YZ;",
            Opcode::Halt => "exit();",
        }
    }

    /// Which functional group of the machine the instruction belongs to, as
    /// classified in Section 4 of the notes.
    pub const fn group(self) -> InstructionGroup {
        match self {
            Opcode::LoadImmediate
            | Opcode::LoadRegister
            | Opcode::LoadMemory
            | Opcode::StoreMemory => InstructionGroup::DataTransfer,
            Opcode::Add
            | Opcode::Negate
            | Opcode::ShiftRight
            | Opcode::ShiftLeft
            | Opcode::And
            | Opcode::Or
            | Opcode::Xor => InstructionGroup::ArithmeticLogic,
            Opcode::Jump | Opcode::Halt => InstructionGroup::Control,
        }
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Opcode::LoadImmediate => "LOAD immediate",
            Opcode::LoadRegister => "LOAD register",
            Opcode::LoadMemory => "LOAD memory",
            Opcode::StoreMemory => "STORE",
            Opcode::Add => "ADD",
            Opcode::Negate => "NEGATE",
            Opcode::ShiftRight => "SHIFT right",
            Opcode::ShiftLeft => "SHIFT left",
            Opcode::And => "AND",
            Opcode::Or => "OR",
            Opcode::Xor => "XOR",
            Opcode::Jump => "JUMP",
            Opcode::Halt => "HALT",
        };
        f.write_str(name)
    }
}

/// The three groups of machine instruction described in Section 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InstructionGroup {
    /// Instructions that move data between memory and registers.
    DataTransfer,
    /// Instructions that activate circuits in the ALU.
    ArithmeticLogic,
    /// Instructions that change the flow of control.
    Control,
}

impl InstructionGroup {
    /// Every group, in the order the notes introduce them.
    pub const ALL: [InstructionGroup; 3] = [
        InstructionGroup::DataTransfer,
        InstructionGroup::ArithmeticLogic,
        InstructionGroup::Control,
    ];

    /// Human readable name.
    pub const fn label(self) -> &'static str {
        match self {
            InstructionGroup::DataTransfer => "Data transfer",
            InstructionGroup::ArithmeticLogic => "Arithmetic / logic",
            InstructionGroup::Control => "Control",
        }
    }
}

/// A decoded MASSEY machine instruction: a 16-bit word plus its four nibbles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
pub struct Instruction {
    raw: u16,
}

impl Instruction {
    /// Wrap a raw 16-bit machine code word.
    pub const fn from_word(raw: u16) -> Self {
        Instruction { raw }
    }

    /// Encode an instruction from its four hexadecimal digits.
    pub const fn from_nibbles(opcode: u8, x: u8, y: u8, z: u8) -> Self {
        Instruction {
            raw: ((opcode as u16 & 0x0F) << 12)
                | ((x as u16 & 0x0F) << 8)
                | ((y as u16 & 0x0F) << 4)
                | (z as u16 & 0x0F),
        }
    }

    /// Build an instruction from an opcode and its three operand nibbles.
    pub const fn new(opcode: Opcode, x: u8, y: u8, z: u8) -> Self {
        Instruction::from_nibbles(opcode.nibble(), x, y, z)
    }

    /// The raw 16-bit word.
    pub const fn word(self) -> u16 {
        self.raw
    }

    /// First hexadecimal digit (bits 15..12).
    pub const fn opcode_nibble(self) -> u8 {
        (self.raw >> 12) as u8 & 0x0F
    }

    /// The opcode, or `None` when the first digit is not a MASSEY opcode.
    pub fn opcode(self) -> Option<Opcode> {
        Opcode::from_nibble(self.opcode_nibble())
    }

    /// First operand digit (bits 11..8), called `x` in the notes.
    pub const fn x(self) -> u8 {
        (self.raw >> 8) as u8 & 0x0F
    }

    /// Second operand digit (bits 7..4), called `y` in the notes.
    pub const fn y(self) -> u8 {
        (self.raw >> 4) as u8 & 0x0F
    }

    /// Third operand digit (bits 3..0), called `z` in the notes.
    pub const fn z(self) -> u8 {
        self.raw as u8 & 0x0F
    }

    /// The 8-bit address or immediate `YZ` formed from the last two digits.
    pub const fn yz(self) -> u8 {
        (self.y() << 4) | self.z()
    }

    /// The `xyz` operand triple, used by the `Rx = Ry op Rz` family.
    pub const fn xyz(self) -> (u8, u8, u8) {
        (self.x(), self.y(), self.z())
    }

    /// The four hexadecimal digits of the word, most significant first.
    pub fn nibbles(self) -> [u8; 4] {
        [self.opcode_nibble(), self.x(), self.y(), self.z()]
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&format_word(self.raw))
    }
}

/// Format a 16-bit machine word as four uppercase hexadecimal digits.
pub fn format_word(word: u16) -> String {
    format!("{word:04X}")
}

/// Format a byte as two uppercase hexadecimal digits.
pub fn format_byte(value: u8) -> String {
    format!("{value:02X}")
}

/// Format a value as a hexadecimal number of exactly `digits` digits.
pub fn format_hex(value: u16, digits: usize) -> String {
    format!("{value:0width$X}", width = digits)
}

/// The name of a register: `R0`, `R1`, ... `RA`, `RF`.
pub fn register_name(index: u8) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let digit = DIGITS[(index & 0x0F) as usize] as char;
    format!("R{digit}")
}

/// Parse a register name such as `R0`, `Ra` or `RF` into its index.
///
/// Only the single-digit form is accepted, because that is what the machine
/// encodes: register 10 is `RA`, never `R10`.
pub fn parse_register(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !first.eq_ignore_ascii_case(&'r') {
        return None;
    }
    let digit = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    digit.to_digit(16).map(|value| value as u8)
}

/// Interpret a 16-bit register value as a signed 2's complement integer.
pub const fn to_signed(value: u16) -> i16 {
    value as i16
}

/// Render a 16-bit value as a binary string with a space every four bits.
pub fn binary_grouped(value: u16) -> String {
    let mut out = String::with_capacity(19);
    for bit in (0..16).rev() {
        out.push(if (value >> bit) & 1 == 1 { '1' } else { '0' });
        if bit % 4 == 0 && bit != 0 {
            out.push(' ');
        }
    }
    out
}
