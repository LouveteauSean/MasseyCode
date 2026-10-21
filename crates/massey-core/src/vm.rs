//! The MASSEY machine itself: memory, registers, the control unit, the ALU and
//! the bus, stepped one register transfer at a time.
//!
//! The machine is faithful to the 159.102 notes:
//!
//! * 256 memory locations, each 16 bits, addressed `00`..`FF`;
//! * 16 registers of 16 bits, `R0`..`RF`;
//! * 2's complement integer arithmetic;
//! * a program counter and an instruction register in the control unit;
//! * a four step fetch (`PC -> MAR`, `M[MAR] -> MDR`, `MDR -> IR`, `PC + 1 -> PC`)
//!   followed by the execute phase of the decoded instruction.
//!
//! The finest step the machine supports is a *micro-operation*, a single
//! register transfer such as `PC -> MAR`.  Executing one instruction is just
//! running micro-operations until the instruction completes, so the coarse and
//! fine stepping modes can never disagree.

use std::collections::VecDeque;

use crate::isa::{Instruction, Opcode, MEMORY_WORDS, REGISTER_COUNT};

/// The number of register transfers in the fetch phase of every instruction.
pub const FETCH_TICKS: u8 = 4;

/// Which part of the machine cycle a micro-operation belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Fetching the next instruction from memory.
    Fetch,
    /// Working out what the instruction in the instruction register means.
    Decode,
    /// Activating the circuits that carry the instruction out.
    Execute,
    /// The machine stopped.
    Halt,
}

impl Phase {
    /// Human readable name.
    pub const fn label(self) -> &'static str {
        match self {
            Phase::Fetch => "fetch",
            Phase::Decode => "decode",
            Phase::Execute => "execute",
            Phase::Halt => "halt",
        }
    }
}

/// Direction of a bus transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BusDirection {
    /// Memory towards the CPU.
    Read,
    /// CPU towards memory.
    Write,
}

/// One transfer across the bus, used by the datapath view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusTransfer {
    /// Read or write.
    pub direction: BusDirection,
    /// Address on the address lines.
    pub address: u8,
    /// Data on the data lines.
    pub value: u16,
}

/// A record of the last ALU operation, for the ALU pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AluRecord {
    /// Operation name, e.g. `ADD`, `AND`, `NOT`, `SHL`.
    pub operation: String,
    /// Left operand.
    pub left: u16,
    /// Right operand, if the operation takes two.
    pub right: Option<u16>,
    /// Result written back to the register file.
    pub result: u16,
}

/// One register transfer, as reported by [`Machine::micro_step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicroOp {
    /// The transfer written the way the course notes write it, e.g. `PC -> MAR`.
    pub text: String,
    /// Which part of the machine cycle this transfer belongs to.
    pub phase: Phase,
    /// True when this transfer finished the instruction.
    pub ends_instruction: bool,
    /// True when the machine halted.
    pub halts: bool,
    /// True when this transfer raised a fault.
    pub faulted: bool,
    /// Bus traffic caused by this transfer, if any.
    pub bus: Option<BusTransfer>,
    /// Registers written by this transfer.
    pub register_writes: Vec<(u8, u16)>,
    /// The instruction address this transfer belongs to.
    pub address: u8,
}

/// A fault raised by the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    /// What went wrong.
    pub kind: FaultKind,
    /// The instruction word that caused it, when there is one.
    pub word: u16,
    /// The address the instruction was read from.
    pub address: u8,
}

/// The kinds of fault the MASSEY machine can raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultKind {
    /// The opcode nibble is not one of the MASSEY opcodes (0 or 5, or an
    /// opcode with no execute phase).
    UnknownInstruction,
    /// The program ran without halting for longer than the step budget.
    StepBudgetExhausted,
}

impl Fault {
    /// A message suitable for the status bar or a log pane.
    pub fn message(&self) -> String {
        match self.kind {
            FaultKind::UnknownInstruction => format!(
                "unknown instruction {:04X} at address {:02X}: the MASSEY machine has no opcode \
                 {:X}",
                self.word,
                self.address,
                self.word >> 12
            ),
            FaultKind::StepBudgetExhausted => {
                "the program is still running: it has not reached a HALT instruction".to_string()
            }
        }
    }
}

/// A complete copy of the machine's mutable state, used for stepping back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Program counter.
    pub pc: u8,
    /// Instruction register.
    pub ir: u16,
    /// Register file.
    pub registers: [u16; REGISTER_COUNT],
    /// Memory address register.
    pub mar: u8,
    /// Memory data register.
    pub mdr: u16,
    /// Address of the instruction being executed.
    pub instruction_address: u8,
    /// Position inside the current instruction.
    pub tick: u8,
    /// Number of execute transfers the current instruction needs.
    pub execute_ticks: u8,
    /// Whether the machine has halted.
    pub halted: bool,
    /// Fault state, if any.
    pub fault: Option<Fault>,
    /// Instructions completed, so stepping back restores the counters too.
    pub instructions: u64,
    /// Register transfers executed.
    pub micro_operations: u64,
    /// A copy of memory.
    pub memory: [u16; MEMORY_WORDS],
}

impl Snapshot {
    /// Approximate size in bytes, used to explain the undo limit.
    pub const SIZE_BYTES: usize = MEMORY_WORDS * 2 + REGISTER_COUNT * 2 + 32;
}

/// A single line of the micro-operation log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRecord {
    /// Monotonic micro-operation index.
    pub index: u64,
    /// Instruction address the transfer belongs to.
    pub address: u8,
    /// Machine cycle phase.
    pub phase: Phase,
    /// The transfer text.
    pub text: String,
}

/// Why the machine stopped running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStop {
    /// A HALT instruction was executed.
    Halted,
    /// A breakpoint was reached.
    Breakpoint(u8),
    /// A fault stopped the machine.
    Fault,
    /// The step budget ran out.
    Budget,
    /// The machine was already halted.
    AlreadyHalted,
}

/// A summary of running one instruction to completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionReport {
    /// Address of the instruction that ran.
    pub address: u8,
    /// The instruction word.
    pub word: u16,
    /// Disassembly of the instruction.
    pub text: String,
    /// The register transfers it was made of.
    pub micro_ops: Vec<String>,
    /// True when the machine halted.
    pub halted: bool,
    /// True when the instruction faulted.
    pub faulted: bool,
}

/// The state of the MASSEY machine.
#[derive(Clone, Debug)]
pub struct Machine {
    /// 256 memory locations of 16 bits.
    pub memory: [u16; MEMORY_WORDS],
    /// 16 registers of 16 bits.
    pub registers: [u16; REGISTER_COUNT],
    /// Program counter (control unit).
    pub pc: u8,
    /// Instruction register (control unit).
    pub ir: u16,
    /// Memory address register (control unit).
    pub mar: u8,
    /// Memory data register (control unit).
    pub mdr: u16,
    /// Address of the instruction being worked on; does not move during a fetch.
    pub instruction_address: u8,
    /// Position inside the current instruction: 0 is between instructions.
    pub tick: u8,
    /// Number of execute transfers the decoded instruction needs.
    pub execute_ticks: u8,
    /// True once the machine has halted or faulted.
    pub halted: bool,
    /// The fault that stopped the machine, if any.
    pub fault: Option<Fault>,
    /// Instructions completed since the last reset.
    pub instructions: u64,
    /// Micro-operations executed since the last reset.
    pub micro_operations: u64,
    /// Reads performed across the bus.
    pub bus_reads: u64,
    /// Writes performed across the bus.
    pub bus_writes: u64,
    /// The most recent bus transfer.
    pub last_transfer: Option<BusTransfer>,
    /// The most recent ALU operation.
    pub last_alu: Option<AluRecord>,
    /// A short ring buffer of recent micro-operations.
    pub trace: VecDeque<TraceRecord>,
    /// The addresses of memory that the running program has written to.
    pub touched: Vec<u8>,
    /// The image the machine was loaded with, restored by [`Machine::reset`].
    image: [u16; MEMORY_WORDS],
    /// How many words of `image` are meaningful.
    image_len: usize,
    /// Saved states for stepping back, oldest first.
    history: Vec<Snapshot>,
    /// Maximum number of saved states.
    history_limit: usize,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new()
    }
}

impl Machine {
    /// An empty machine: all memory zero, all registers zero, PC at `00`.
    pub fn new() -> Self {
        Machine {
            memory: [0; MEMORY_WORDS],
            registers: [0; REGISTER_COUNT],
            pc: 0,
            ir: 0,
            mar: 0,
            mdr: 0,
            instruction_address: 0,
            tick: 0,
            execute_ticks: 0,
            halted: false,
            fault: None,
            instructions: 0,
            micro_operations: 0,
            bus_reads: 0,
            bus_writes: 0,
            last_transfer: None,
            last_alu: None,
            trace: VecDeque::new(),
            touched: Vec::new(),
            image: [0; MEMORY_WORDS],
            image_len: 0,
            history: Vec::new(),
            history_limit: 20_000,
        }
    }

    /// Load a program image and reset the machine.
    pub fn load_image(&mut self, image: &[u16]) {
        self.image = [0; MEMORY_WORDS];
        self.image_len = image.len().min(MEMORY_WORDS);
        self.image[..self.image_len].copy_from_slice(&image[..self.image_len]);
        self.reset();
    }

    /// The loaded image, as a slice.
    pub fn image(&self) -> &[u16] {
        &self.image[..self.image_len]
    }

    /// Number of words in the loaded image.
    pub fn image_len(&self) -> usize {
        self.image_len
    }

    /// Restore the loaded image and clear all registers, as if the machine had
    /// just been switched on with this program in memory.
    pub fn reset(&mut self) {
        self.memory = self.image;
        self.registers = [0; REGISTER_COUNT];
        self.pc = 0;
        self.ir = 0;
        self.mar = 0;
        self.mdr = 0;
        self.instruction_address = 0;
        self.tick = 0;
        self.execute_ticks = 0;
        self.halted = false;
        self.fault = None;
        self.instructions = 0;
        self.micro_operations = 0;
        self.bus_reads = 0;
        self.bus_writes = 0;
        self.last_transfer = None;
        self.last_alu = None;
        self.trace.clear();
        self.touched.clear();
    }

    /// Clear memory (including the loaded image) and all registers.
    pub fn clear(&mut self) {
        self.image = [0; MEMORY_WORDS];
        self.image_len = 0;
        self.reset();
        self.history.clear();
    }

    /// The instruction the program counter points at.
    pub fn current_instruction(&self) -> Instruction {
        Instruction::from_word(self.memory[self.pc as usize])
    }

    /// Disassembly of the instruction at an address.
    pub fn disassemble_at(&self, address: u8) -> String {
        crate::disasm::disassemble(self.memory[address as usize])
    }

    /// How many execute transfers an opcode needs.
    pub fn execute_tick_count(opcode: Option<Opcode>) -> u8 {
        match opcode {
            Some(Opcode::LoadMemory) | Some(Opcode::StoreMemory) => 3,
            Some(Opcode::Add) | Some(Opcode::And) | Some(Opcode::Or) | Some(Opcode::Xor) => 1,
            Some(Opcode::LoadImmediate)
            | Some(Opcode::LoadRegister)
            | Some(Opcode::Negate)
            | Some(Opcode::ShiftRight)
            | Some(Opcode::ShiftLeft)
            | Some(Opcode::Jump) => 1,
            // HALT has no execute transfer: fetching it was the whole thing.
            Some(Opcode::Halt) | None => 0,
        }
    }

    /// The register transfers an instruction is made of, in order: the four
    /// fetch transfers followed by the execute transfers the opcode needs.
    ///
    /// This is what the demonstrator draws while you step one transfer at a
    /// time, so the fetch/decode/execute cycle is visible rather than implied.
    pub fn transfer_plan(word: u16) -> Vec<String> {
        let instruction = Instruction::from_word(word);
        let mut plan: Vec<String> = vec![
            "PC -> MAR".to_string(),
            "M[MAR] -> MDR".to_string(),
            "MDR -> IR".to_string(),
            "PC + 1 -> PC".to_string(),
        ];
        let x = instruction.x();
        let y = instruction.y();
        let z = instruction.z();
        let yz = instruction.yz();
        match instruction.opcode() {
            Some(Opcode::LoadImmediate) => {
                plan.push(format!("{yz:02X} -> {}", crate::isa::register_name(x)));
            }
            Some(Opcode::LoadRegister) => {
                plan.push(format!(
                    "{} -> {}",
                    crate::isa::register_name(z),
                    crate::isa::register_name(y)
                ));
            }
            Some(Opcode::LoadMemory) => {
                plan.push(format!("{yz:02X} -> MAR"));
                plan.push("M[MAR] -> MDR".to_string());
                plan.push(format!("MDR -> {}", crate::isa::register_name(x)));
            }
            Some(Opcode::StoreMemory) => {
                plan.push(format!("{yz:02X} -> MAR"));
                plan.push(format!("{} -> MDR", crate::isa::register_name(x)));
                plan.push("MDR -> M[MAR]".to_string());
            }
            Some(Opcode::Add) => {
                plan.push(format!(
                    "{} + {} -> {}",
                    crate::isa::register_name(y),
                    crate::isa::register_name(z),
                    crate::isa::register_name(x)
                ));
            }
            Some(Opcode::Negate) => {
                plan.push(format!(
                    "~{} + 1 -> {}",
                    crate::isa::register_name(y),
                    crate::isa::register_name(y)
                ));
            }
            Some(Opcode::ShiftRight) => {
                plan.push(format!(
                    "{} >> {yz:02X} -> {}",
                    crate::isa::register_name(x),
                    crate::isa::register_name(x)
                ));
            }
            Some(Opcode::ShiftLeft) => {
                plan.push(format!(
                    "{} << {yz:02X} -> {}",
                    crate::isa::register_name(x),
                    crate::isa::register_name(x)
                ));
            }
            Some(opcode @ (Opcode::And | Opcode::Or | Opcode::Xor)) => {
                let name = match opcode {
                    Opcode::And => "AND",
                    Opcode::Or => "OR",
                    _ => "XOR",
                };
                plan.push(format!(
                    "{} {name} {} -> {}",
                    crate::isa::register_name(y),
                    crate::isa::register_name(z),
                    crate::isa::register_name(x)
                ));
            }
            Some(Opcode::Jump) => {
                plan.push(format!(
                    "if {} = R0: {yz:02X} -> PC",
                    crate::isa::register_name(x)
                ));
            }
            // HALT has no execute transfer: fetching it was the whole thing.
            Some(Opcode::Halt) | None => {}
        }
        plan
    }

    /// How far through the current instruction's transfer plan the machine is.
    ///
    /// None between instructions (and once halted), otherwise the index of
    /// the transfer that has just been performed.
    pub fn transfer_position(&self) -> Option<usize> {
        if self.halted || self.tick == 0 {
            return None;
        }
        Some((self.tick - 1) as usize)
    }

    /// A plain text rendering of where the machine is, for logs and tests.
    pub fn describe_state(&self) -> String {
        let registers: Vec<String> = self
            .registers
            .iter()
            .enumerate()
            .map(|(index, value)| format!("{}={value:04X}", crate::isa::register_name(index as u8)))
            .collect();
        format!(
            "PC={:02X} IR={:04X} MAR={:02X} MDR={:04X} tick={} {}",
            self.pc,
            self.ir,
            self.mar,
            self.mdr,
            self.tick,
            registers.join(" ")
        )
    }

    // -- history ------------------------------------------------------------

    /// Save the current state so it can be restored by [`Machine::undo`].
    pub fn push_history(&mut self) {
        if self.history.len() >= self.history_limit {
            let excess = self.history.len() - self.history_limit + 1;
            self.history.drain(..excess);
        }
        self.history.push(self.snapshot());
    }

    /// How many states are available to step back to.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    /// Drop all saved states.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Restore the most recent saved state.  Returns false when there is none.
    pub fn undo(&mut self) -> bool {
        match self.history.pop() {
            Some(snapshot) => {
                self.restore(&snapshot);
                true
            }
            None => false,
        }
    }

    /// Capture the complete machine state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pc: self.pc,
            ir: self.ir,
            registers: self.registers,
            mar: self.mar,
            mdr: self.mdr,
            instruction_address: self.instruction_address,
            tick: self.tick,
            execute_ticks: self.execute_ticks,
            halted: self.halted,
            fault: self.fault,
            instructions: self.instructions,
            micro_operations: self.micro_operations,
            memory: self.memory,
        }
    }

    /// Restore a previously captured state.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        self.pc = snapshot.pc;
        self.ir = snapshot.ir;
        self.registers = snapshot.registers;
        self.mar = snapshot.mar;
        self.mdr = snapshot.mdr;
        self.instruction_address = snapshot.instruction_address;
        self.tick = snapshot.tick;
        self.execute_ticks = snapshot.execute_ticks;
        self.halted = snapshot.halted;
        self.fault = snapshot.fault;
        self.instructions = snapshot.instructions;
        self.micro_operations = snapshot.micro_operations;
        self.memory = snapshot.memory;
        self.touched = self
            .image
            .iter()
            .zip(self.memory.iter())
            .enumerate()
            .filter(|(_, (image, live))| image != live)
            .map(|(index, _)| index as u8)
            .collect();
    }

    // -- the machine cycle --------------------------------------------------

    /// True when the machine can still run.
    pub fn is_running(&self) -> bool {
        !self.halted
    }

    /// Perform exactly one register transfer.
    ///
    /// This is the only place the machine changes state.  `step_instruction`
    /// simply runs micro-operations until the instruction is complete.
    pub fn micro_step(&mut self) -> MicroOp {
        if self.halted {
            return MicroOp {
                text: "halted".to_string(),
                phase: Phase::Halt,
                ends_instruction: true,
                halts: true,
                faulted: self.fault.is_some(),
                bus: None,
                register_writes: Vec::new(),
                address: self.pc,
            };
        }

        self.micro_operations += 1;

        let op = if self.tick < FETCH_TICKS {
            let op = self.fetch_tick(self.tick);
            self.tick += 1;
            if self.tick == FETCH_TICKS {
                let instruction = Instruction::from_word(self.ir);
                let opcode = instruction.opcode();
                self.execute_ticks = Machine::execute_tick_count(opcode);
                if opcode == Some(Opcode::Halt) {
                    self.halted = true;
                    self.instructions += 1;
                    self.tick = 0;
                    let op = MicroOp {
                        ends_instruction: true,
                        halts: true,
                        ..op
                    };
                    self.push_trace(&op);
                    return op;
                }
                if opcode.is_none() {
                    let fault = Fault {
                        kind: FaultKind::UnknownInstruction,
                        word: self.ir,
                        address: self.instruction_address,
                    };
                    self.fault = Some(fault);
                    self.halted = true;
                    self.tick = 0;
                    let op = MicroOp {
                        ends_instruction: true,
                        halts: true,
                        faulted: true,
                        ..op
                    };
                    self.push_trace(&op);
                    return op;
                }
            }
            op
        } else {
            let op = self.execute_tick(self.tick - FETCH_TICKS);
            self.tick += 1;
            if self.tick - FETCH_TICKS >= self.execute_ticks {
                self.tick = 0;
                self.instructions += 1;
                let op = MicroOp {
                    ends_instruction: true,
                    ..op
                };
                self.push_trace(&op);
                return op;
            }
            op
        };

        self.push_trace(&op);
        op
    }

    fn push_trace(&mut self, op: &MicroOp) {
        if self.trace.len() >= 512 {
            self.trace.pop_front();
        }
        self.trace.push_back(TraceRecord {
            index: self.micro_operations,
            address: op.address,
            phase: op.phase,
            text: op.text.clone(),
        });
    }

    fn fetch_tick(&mut self, index: u8) -> MicroOp {
        let address = self.instruction_address;
        match index {
            0 => {
                self.instruction_address = self.pc;
                self.mar = self.pc;
                MicroOp {
                    text: "PC -> MAR".to_string(),
                    phase: Phase::Fetch,
                    ends_instruction: false,
                    halts: false,
                    faulted: false,
                    bus: None,
                    register_writes: Vec::new(),
                    address: self.instruction_address,
                }
            }
            1 => {
                let value = self.bus_read(self.mar);
                self.mdr = value;
                MicroOp {
                    text: "M[MAR] -> MDR".to_string(),
                    phase: Phase::Fetch,
                    ends_instruction: false,
                    halts: false,
                    faulted: false,
                    bus: Some(BusTransfer {
                        direction: BusDirection::Read,
                        address: self.mar,
                        value,
                    }),
                    register_writes: Vec::new(),
                    address: self.instruction_address,
                }
            }
            2 => {
                self.ir = self.mdr;
                MicroOp {
                    text: "MDR -> IR".to_string(),
                    phase: Phase::Fetch,
                    ends_instruction: false,
                    halts: false,
                    faulted: false,
                    bus: None,
                    register_writes: Vec::new(),
                    address: self.instruction_address,
                }
            }
            _ => {
                self.pc = self.pc.wrapping_add(1);
                MicroOp {
                    text: "PC + 1 -> PC".to_string(),
                    phase: Phase::Fetch,
                    ends_instruction: false,
                    halts: false,
                    faulted: false,
                    bus: None,
                    register_writes: Vec::new(),
                    address,
                }
            }
        }
    }

    fn execute_tick(&mut self, index: u8) -> MicroOp {
        let instruction = Instruction::from_word(self.ir);
        let x = instruction.x();
        let y = instruction.y();
        let z = instruction.z();
        let yz = instruction.yz();
        let address = self.instruction_address;
        let base = MicroOp {
            text: String::new(),
            phase: Phase::Execute,
            ends_instruction: false,
            halts: false,
            faulted: false,
            bus: None,
            register_writes: Vec::new(),
            address,
        };
        macro_rules! op {
            ($text:expr) => {{
                let mut op = base.clone();
                op.text = $text.into();
                op
            }};
            ($text:expr, $reg:expr, $value:expr) => {{
                let mut op = base.clone();
                op.text = $text.into();
                self.registers[$reg as usize] = $value;
                op.register_writes.push(($reg, $value));
                op
            }};
        }

        match instruction.opcode() {
            Some(Opcode::LoadImmediate) => {
                let value = yz as u16;
                op!(
                    format!("{yz:02X} -> {}", crate::isa::register_name(x)),
                    x,
                    value
                )
            }
            Some(Opcode::LoadRegister) => {
                let value = self.registers[z as usize];
                op!(
                    format!(
                        "{} -> {}",
                        crate::isa::register_name(z),
                        crate::isa::register_name(y)
                    ),
                    y,
                    value
                )
            }
            Some(Opcode::LoadMemory) => match index {
                0 => {
                    self.mar = yz;
                    op!(format!("{yz:02X} -> MAR"))
                }
                1 => {
                    let value = self.bus_read(self.mar);
                    self.mdr = value;
                    let mut op = op!("M[MAR] -> MDR".to_string());
                    op.bus = Some(BusTransfer {
                        direction: BusDirection::Read,
                        address: self.mar,
                        value,
                    });
                    op
                }
                _ => {
                    let value = self.mdr;
                    op!(format!("MDR -> {}", crate::isa::register_name(x)), x, value)
                }
            },
            Some(Opcode::StoreMemory) => match index {
                0 => {
                    self.mar = yz;
                    op!(format!("{yz:02X} -> MAR"))
                }
                1 => {
                    let value = self.registers[x as usize];
                    self.mdr = value;
                    op!(format!("{} -> MDR", crate::isa::register_name(x)))
                }
                _ => {
                    let address = self.mar;
                    let value = self.mdr;
                    self.bus_write(address, value);
                    let mut op = op!("MDR -> M[MAR]".to_string());
                    op.bus = Some(BusTransfer {
                        direction: BusDirection::Write,
                        address,
                        value,
                    });
                    op
                }
            },
            Some(Opcode::Add) => {
                let left = self.registers[y as usize];
                let right = self.registers[z as usize];
                let value = left.wrapping_add(right);
                self.last_alu = Some(AluRecord {
                    operation: "ADD".to_string(),
                    left,
                    right: Some(right),
                    result: value,
                });
                op!(
                    format!(
                        "{} + {} -> {}",
                        crate::isa::register_name(y),
                        crate::isa::register_name(z),
                        crate::isa::register_name(x)
                    ),
                    x,
                    value
                )
            }
            Some(Opcode::Negate) => {
                let before = self.registers[y as usize];
                let value = (!before).wrapping_add(1);
                self.last_alu = Some(AluRecord {
                    operation: "NEGATE".to_string(),
                    left: before,
                    right: None,
                    result: value,
                });
                op!(
                    format!(
                        "~{} + 1 -> {}",
                        crate::isa::register_name(y),
                        crate::isa::register_name(y)
                    ),
                    y,
                    value
                )
            }
            Some(Opcode::ShiftRight) => {
                let before = self.registers[x as usize];
                let value = if yz >= 16 { 0 } else { before >> yz };
                self.last_alu = Some(AluRecord {
                    operation: "SHR".to_string(),
                    left: before,
                    right: Some(yz as u16),
                    result: value,
                });
                op!(
                    format!(
                        "{} >> {yz:02X} -> {}",
                        crate::isa::register_name(x),
                        crate::isa::register_name(x)
                    ),
                    x,
                    value
                )
            }
            Some(Opcode::ShiftLeft) => {
                let before = self.registers[x as usize];
                let value = if yz >= 16 { 0 } else { before << yz };
                self.last_alu = Some(AluRecord {
                    operation: "SHL".to_string(),
                    left: before,
                    right: Some(yz as u16),
                    result: value,
                });
                op!(
                    format!(
                        "{} << {yz:02X} -> {}",
                        crate::isa::register_name(x),
                        crate::isa::register_name(x)
                    ),
                    x,
                    value
                )
            }
            Some(opcode @ (Opcode::And | Opcode::Or | Opcode::Xor)) => {
                let left = self.registers[y as usize];
                let right = self.registers[z as usize];
                let (name, value) = match opcode {
                    Opcode::And => ("AND", left & right),
                    Opcode::Or => ("OR", left | right),
                    _ => ("XOR", left ^ right),
                };
                self.last_alu = Some(AluRecord {
                    operation: name.to_string(),
                    left,
                    right: Some(right),
                    result: value,
                });
                op!(
                    format!(
                        "{} {name} {} -> {}",
                        crate::isa::register_name(y),
                        crate::isa::register_name(z),
                        crate::isa::register_name(x)
                    ),
                    x,
                    value
                )
            }
            Some(Opcode::Jump) => {
                let taken = self.registers[x as usize] == self.registers[0];
                if taken {
                    self.pc = yz;
                }
                op!(format!(
                    "if {} = R0: {yz:02X} -> PC{}",
                    crate::isa::register_name(x),
                    if taken { "" } else { " (not taken)" }
                ))
            }
            Some(Opcode::Halt) | None => {
                let fault = Fault {
                    kind: FaultKind::UnknownInstruction,
                    word: self.ir,
                    address,
                };
                self.fault = Some(fault);
                self.halted = true;
                let mut op = op!("illegal instruction");
                op.faulted = true;
                op.ends_instruction = true;
                op.halts = true;
                op
            }
        }
    }

    /// Run micro-operations until the current instruction completes.
    pub fn step_instruction(&mut self) -> InstructionReport {
        let address = self.pc;
        let word = self.memory[self.pc as usize];
        let text = crate::disasm::disassemble(word);
        let mut micro_ops = Vec::new();
        let halted;
        let mut faulted = false;
        loop {
            let op = self.micro_step();
            micro_ops.push(op.text.clone());
            if op.faulted {
                faulted = true;
            }
            if op.ends_instruction {
                halted = op.halts;
                break;
            }
        }
        InstructionReport {
            address,
            word,
            text,
            micro_ops,
            halted,
            faulted,
        }
    }

    /// Run until a HALT, a fault, or a breakpoint.
    ///
    /// `breakpoints` is consulted before every instruction, so a program that
    /// never halts can still be stopped.  `budget` limits the number of
    /// instructions, guarding against an infinite loop in the classroom.
    pub fn run(&mut self, budget: u64, breakpoints: &[u8]) -> RunStop {
        if self.halted {
            return RunStop::AlreadyHalted;
        }
        let mut executed = 0u64;
        while !self.halted {
            if breakpoints.contains(&self.pc) && executed > 0 {
                return RunStop::Breakpoint(self.pc);
            }
            if executed >= budget {
                self.fault = Some(Fault {
                    kind: FaultKind::StepBudgetExhausted,
                    word: self.ir,
                    address: self.pc,
                });
                self.halted = true;
                return RunStop::Budget;
            }
            self.step_instruction();
            executed += 1;
        }
        if self.fault.is_some() {
            RunStop::Fault
        } else {
            RunStop::Halted
        }
    }

    // -- the bus ------------------------------------------------------------

    fn bus_read(&mut self, address: u8) -> u16 {
        self.bus_reads += 1;
        let value = self.memory[address as usize];
        self.last_transfer = Some(BusTransfer {
            direction: BusDirection::Read,
            address,
            value,
        });
        value
    }

    fn bus_write(&mut self, address: u8, value: u16) {
        self.bus_writes += 1;
        self.memory[address as usize] = value;
        self.last_transfer = Some(BusTransfer {
            direction: BusDirection::Write,
            address,
            value,
        });
        if !self.touched.contains(&address) {
            self.touched.push(address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_plan_matches_the_machine_cycle() {
        // The four fetch transfers come first for every instruction.
        for word in [0x1102u16, 0x3508, 0x6056, 0xE000] {
            let plan = Machine::transfer_plan(word);
            assert_eq!(
                &plan[..4],
                ["PC -> MAR", "M[MAR] -> MDR", "MDR -> IR", "PC + 1 -> PC"],
                "{word:04X}"
            );
        }
        // The execute transfers follow, one per opcode.
        assert_eq!(Machine::transfer_plan(0x1102).len(), 5, "LOAD immediate");
        assert_eq!(Machine::transfer_plan(0x3508).len(), 7, "LOAD from memory");
        assert_eq!(Machine::transfer_plan(0x400A).len(), 7, "STORE");
        assert_eq!(Machine::transfer_plan(0x6056).len(), 5, "add");
        assert_eq!(Machine::transfer_plan(0x7020).len(), 5, "negate");
        assert_eq!(Machine::transfer_plan(0x9101).len(), 5, "shift");
        assert_eq!(Machine::transfer_plan(0xA425).len(), 5, "AND");
        assert_eq!(Machine::transfer_plan(0xD408).len(), 5, "JUMP");
        assert_eq!(Machine::transfer_plan(0xE000).len(), 4, "HALT");
    }

    #[test]
    fn transfer_position_walks_the_plan() {
        let mut machine = Machine::new();
        machine.load_image(&[0x1102, 0xE000]);
        assert_eq!(machine.transfer_position(), None, "between instructions");
        // The four fetch transfers, then the single execute transfer of
        // LOAD R1, 02, which completes the instruction.
        for expected in 0..4 {
            machine.micro_step();
            assert_eq!(machine.transfer_position(), Some(expected));
        }
        machine.micro_step();
        assert_eq!(
            machine.transfer_position(),
            None,
            "the instruction is complete, so the machine is between instructions"
        );
        assert_eq!(machine.registers[1], 0x0002);
        // The next micro step is the first fetch of the HALT that follows.
        machine.micro_step();
        assert_eq!(machine.transfer_position(), Some(0));
        let plan = Machine::transfer_plan(machine.memory[machine.instruction_address as usize]);
        assert_eq!(plan.len(), 4);
    }
}
