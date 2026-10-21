# The MASSEY machine

MasseyCode is a visual debugger for the **MASSEY machine**, the 16-bit
teaching computer described in Sections 4 and 5 of the 159.102 course notes.
This document is the reference for that machine: its state, its instruction
encoding, the machine cycle, and the two source notations MasseyCode can read.

The debugger never has to guess at any of it: the same definitions in
`crates/massey-core/src/isa.rs` drive the assembler, the disassembler, the
machine and the diagram.

## Contents

1. [The model](#1-the-model)
2. [Instruction format](#2-instruction-format)
3. [Instruction set](#3-instruction-set)
4. [The machine cycle](#4-the-machine-cycle)
5. [Assembly language](#5-assembly-language)
6. [Worked example](#6-worked-example)
7. [Faults and run limits](#7-faults-and-run-limits)
8. [Quick reference](#8-quick-reference)

## 1. The model

The machine is small enough to hold every part in your head, which is the
point.

| Part | Size | Contents |
| --- | --- | --- |
| Memory | 256 × 16 bits | program and data, addresses `00`–`FF` |
| Registers | 16 × 16 bits | `R0` … `RF` |
| Program counter | 8 bits | `PC`: address of the next instruction |
| Instruction register | 16 bits | `IR`: the instruction being executed |
| Address register | 8 bits | `MAR`: drives the address lines |
| Data register | 16 bits | `MDR`: drives the data lines |
| ALU | 16 bits | arithmetic and logic |

Memory and registers are `u16`. Values are interpreted as **16-bit two's
complement** when they represent negative numbers, so `FFFF` is −1 and `8000`
is −32768. There is no status register and there are no flags: the only
conditional instruction compares two registers directly.

The machine has one bus. Every memory access goes through `MAR` (address) and
`MDR` (data), which is why those two registers, and not the general registers,
appear on the bus between the control unit and memory.

```
            ┌──────────────────────────────────────────┐
            │ control unit                             │
   PC ──────┤  PC ──▶ MAR                              │
   IR ◀─────┤  IR ◀── MDR                              │
            │           │                              │
            │  registers R0 … RF ──▶ ALU ──▶ registers  │
            └───────────┬───────────────▲──────────────┘
                 MAR ───┘  address      │ data
                        ┌───────────────┴──┐
                        │ memory 00 … FF   │
                        └──────────────────┘
```

## 2. Instruction format

Every instruction is one 16-bit word, written as four hexadecimal digits. The
first digit is the opcode; the other three are operands.

```
 bit  15 14 13 12 │ 11 10  9  8 │ 7  6  5  4 │ 3  2  1  0
      ────────────┼─────────────┼────────────┼───────────
       opcode     │   first     │  second    │   third
                  │  operand    │  operand   │  operand
```

* **Opcode** (bits 15–12): one of the thirteen opcodes below. Nibbles `0`, `5`
  and `F` are not instructions.
* **Registers** are a single hexadecimal digit: `0`–`9` then `A`–`F`, giving
  `R0`–`RF`. There is no two-digit register name, because the machine only has
  four operand bits to spare.
* **`YZ`** is the 8-bit value formed from the last two digits
  (`(second << 4) | third`). Depending on the instruction it is an immediate
  value, an address, or a shift count.
* **`xyz`** is the three operand digits used by the register-to-register
  instructions.

A word therefore reads as `OP` `d1` `d2` `d3`, for example `3508`:

```
 3  5  0  8
 │  │  └──┴── 08: memory address
 │  └──────── 5: destination register R5
 └─────────── 3: LOAD from memory
```

which is `LOAD R5, [08]`.

## 3. Instruction set

There are thirteen instructions in three groups. `d` is a destination
register digit, `s`/`s1`/`s2` are source register digits, and `r` is the single
register an instruction reads and writes.

The *notes* column repeats the shorthand the course notes use; the letters in
it are operand positions, not separate operands.

### 3.1 Summary

| Op. | Group | Instruction | Digits | Notes | Assembly |
| --- | --- | --- | --- | --- | --- |
| `1` | Data transfer | load immediate | `1 r YZ` | `1xYZ` | `LOAD Rx, YZ` |
| `2` | Data transfer | load register | `2 0 d s` | `20xy` | `LOAD Rx, Ry` |
| `3` | Data transfer | load from memory | `3 r YZ` | `3xYZ` | `LOAD Rx, [YZ]` |
| `4` | Data transfer | store to memory | `4 r YZ` | `4xYZ` | `STORE Rx, [YZ]` |
| `6` | Arithmetic / logic | add | `6 d s1 s2` | `6xyz` | `Rx = Ry + Rz` |
| `7` | Arithmetic / logic | negate | `7 0 r 0` | `70x0` | `NEGATE Rx` |
| `8` | Arithmetic / logic | shift right | `8 r YZ` | `8xYZ` | `SHIFT Rx RIGHT YZ` |
| `9` | Arithmetic / logic | shift left | `9 r YZ` | `9xYZ` | `SHIFT Rx LEFT YZ` |
| `A` | Arithmetic / logic | bitwise AND | `A d s1 s2` | `Axyz` | `Rx = Ry AND Rz` |
| `B` | Arithmetic / logic | bitwise OR | `B d s1 s2` | `Bxyz` | `Rx = Ry OR Rz` |
| `C` | Arithmetic / logic | bitwise XOR | `C d s1 s2` | `Cxyz` | `Rx = Ry XOR Rz` |
| `D` | Control | conditional jump | `D r YZ` | `DxYZ` | `JUMP [YZ] IF Rx=R0` |
| `E` | Control | halt | `E 0 0 0` | `E000` | `HALT` |

### 3.2 Data transfer

| Instruction | Encoding | Effect | Equivalent C++ |
| --- | --- | --- | --- |
| `LOAD Rx, YZ` | `1 r YZ` | `Rx = YZ` | `Rx = 0xYZ;` |
| `LOAD Rx, Ry` | `2 0 d s` | `Rx = Ry` | `Rx = Ry;` |
| `LOAD Rx, [YZ]` | `3 r YZ` | `Rx = memory[YZ]` | `Rx = memory[YZ];` |
| `STORE Rx, [YZ]` | `4 r YZ` | `memory[YZ] = Rx` | `memory[YZ] = Rx;` |

`LOAD Rx, YZ` uses a literal, not an address. `LOAD Rx, [YZ]` is the only
form that reads memory. There is no relative addressing and no indirect
addressing: `[YZ]` always means the literal address `YZ`.

`STORE` does not disturb `Rx`; it only writes memory.

### 3.3 Arithmetic and logic

| Instruction | Encoding | Effect | Equivalent C++ |
| --- | --- | --- | --- |
| `Rx = Ry + Rz` | `6 d s1 s2` | 16-bit two's complement addition | `Rx = Ry + Rz;` |
| `NEGATE Rx` | `7 0 r 0` | `Rx = ~Rx + 1` | `Rx = -Rx;` |
| `SHIFT Rx RIGHT YZ` | `8 r YZ` | logical right shift by `YZ` bits | `Rx = Rx >> 0xYZ;` |
| `SHIFT Rx LEFT YZ` | `9 r YZ` | left shift by `YZ` bits | `Rx = Rx << 0xYZ;` |
| `Rx = Ry AND Rz` | `A d s1 s2` | bitwise AND | `Rx = Ry & Rz;` |
| `Rx = Ry OR Rz` | `B d s1 s2` | bitwise OR | `Rx = Ry \| Rz;` |
| `Rx = Ry XOR Rz` | `C d s1 s2` | bitwise XOR | `Rx = Ry ^ Rz;` |

Arithmetic wraps modulo 2¹⁶ and never faults, matching two's complement
hardware. A shift count of 16 or more produces `0`; the count is the full 8-bit
`YZ`, so `SHIFT R1 LEFT 08` shifts by eight and `SHIFT R1 LEFT 80` by 128.

There is **no subtract instruction**. Subtraction is written as negate and add:

```asm
NEGATE R2
R0 = R5 + R2
```

### 3.4 Control

| Instruction | Encoding | Effect | Equivalent C++ |
| --- | --- | --- | --- |
| `JUMP [YZ] IF Rx=R0` | `D r YZ` | if `Rx == R0` then `PC = YZ` | `if (Rx == R0) goto YZ;` |
| `HALT` | `E 0 0 0` | stop the machine | `exit();` |

The jump is taken by writing `YZ` into `PC`. By then the fetch has already
incremented `PC`, so a taken jump discards the incremented value; a jump that
is **not** taken leaves `PC` exactly where the fetch left it, pointing at the
instruction after the jump. `R0` is the fixed comparison register, so a loop
counter lives in `R0` or is copied into it.

The other three digits of `HALT` are not used, and any `E` word behaves the
same way, but write `E000` so the program stays readable.

### 3.5 What the opcodes do not cover

Opcodes `0`, `5` and `F` have no instruction. Executing one is a fault (see
[section 7](#7-faults-and-run-limits)), not a no-op. This matters for data:
a data word that happens to be executed will fault, which is a useful
diagnostic when a program jumps to the wrong address.

## 4. The machine cycle

One instruction is a **fetch** of four register transfers, followed by
**execute** transfers. MasseyCode steps one transfer at a time by default, so
the cycle is visible rather than implied.

### 4.1 Fetch

Every instruction begins with the same four transfers:

| # | Transfer | What it does |
| --- | --- | --- |
| 1 | `PC -> MAR` | put the instruction address on the address bus |
| 2 | `M[MAR] -> MDR` | read the instruction word from memory |
| 3 | `MDR -> IR` | hand the word to the control unit |
| 4 | `PC + 1 -> PC` | advance to the next instruction |

The instruction address is remembered separately, so it is still readable
after `PC` has moved on.

### 4.2 Decode

Decoding the word in `IR` needs no bus transfer: it is combinational logic
inside the control unit. The model has a `decode` phase label, but the transfer
stream goes straight from the fourth fetch transfer to the first execute
transfer.

### 4.3 Execute

The execute transfers depend on the opcode:

| Instruction | Execute transfers |
| --- | --- |
| `LOAD Rx, YZ` | `YZ -> Rx` |
| `LOAD Rx, Ry` | `Ry -> Rx` |
| `LOAD Rx, [YZ]` | `YZ -> MAR`, `M[MAR] -> MDR`, `MDR -> Rx` |
| `STORE Rx, [YZ]` | `YZ -> MAR`, `Rx -> MDR`, `MDR -> M[MAR]` |
| `Rx = Ry + Rz` | `Ry + Rz -> Rx` |
| `NEGATE Rx` | `~Rx + 1 -> Rx` |
| `SHIFT Rx RIGHT YZ` | `Rx >> YZ -> Rx` |
| `SHIFT Rx LEFT YZ` | `Rx << YZ -> Rx` |
| `Rx = Ry AND Rz` | `Ry AND Rz -> Rx` |
| `Rx = Ry OR Rz` | `Ry OR Rz -> Rx` |
| `Rx = Ry XOR Rz` | `Ry XOR Rz -> Rx` |
| `JUMP [YZ] IF Rx=R0` | `if Rx = R0: YZ -> PC` |
| `HALT` | none — fetching it was the whole instruction |

An instruction is therefore between four transfers (`HALT`, and every
one-transfer execute) and seven (`LOAD`/`STORE` through memory). A word that
fetches but has no execute transfer, such as `HALT`, stops the machine as soon
as the fetch completes.

Memory instructions take three transfers because the address has to be put on
the bus, the data has to cross the bus, and the data has to reach its
destination. Reading `LOAD` is `YZ -> MAR`, `M[MAR] -> MDR`, `MDR -> Rx`;
writing `STORE` is the mirror image, `Rx -> MDR`, `MDR -> M[MAR]`.

### 4.4 A transfer trace

The first instruction of `examples/01-addition.asm` is `LOAD R5, [08]`:

```
tick 1   PC -> MAR            PC=00, MAR=00
tick 2   M[MAR] -> MDR        MDR=3508
tick 3   MDR -> IR            IR=3508
tick 4   PC + 1 -> PC         PC=01
tick 5   08 -> MAR            MAR=08
tick 6   M[MAR] -> MDR        MDR=001B
tick 7   MDR -> R5            R5=001B
```

Seven transfers, one instruction, one memory read for the instruction and one
for its operand. The debugger's footer shows the same sequence as
`[exec 1/3] 08 -> MAR · 00 3508 LOAD R5, [08]`.

## 5. Assembly language

MasseyCode deals with two spellings of the same program. Both assemble to the
same list of 16-bit words; the machine never sees the text.

### 5.1 The Assembler view (PDF notation)

The **Assembler** view in the window shows and edits the notation of the course
notes, and nothing else:

* one instruction per line, in one of the thirteen forms above;
* operands are **literal** hexadecimal: registers as `R0`–`RF`, values and
  addresses as one or two hexadecimal digits;
* memory operands are bracketed: `[08]`, `[0A]`;
* keywords and register names are case insensitive (`load r5, [08]` works);
* a blank line is not an instruction: it is a data row, and the existing word
  at that address is kept;
* there are no labels, no directives and no comments. `LOOP:`, `.org 08`,
  `LOAD R1, [NUM1]` and `; comment` are reported as errors, because the course
  notes do not define them.

Each line is one 16-bit word, and line *n* is address *n − 1*, which is exactly
what the address gutter shows. The view refuses to switch back to machine code
while any line is invalid, so the two views can never disagree.

The thirteen forms are:

```text
LOAD  R5, [08]          LOAD  R1, FC         LOAD  R2, R1
STORE R0, [0A]          NEGATE R2
R0 = R5 + R6            R3 = R1 AND R2      R3 = R1 OR R2
R3 = R1 XOR R2          SHIFT R6 RIGHT 4    SHIFT R0 LEFT 8
JUMP  [0E] IF R3=R0     HALT
```

The machine code for the first line is `3508`; the full mapping is in
[section 6](#6-worked-example).

### 5.2 The listing notation (example programs)

The example programs under `examples/` are written in a superset used by the
`massey-core` assembler: the same instructions plus the things a printed
listing needs. This notation is not accepted by the window's Assembler view,
but it is what the bundled examples are written in, and what the test suite
assembles.

A `.asm` listing may freely mix:

* **machine code**, as four hexadecimal digits;
* **assembler instructions**, in the notation above;
* **labels**, `NAME:` or `NAME: instruction`, case insensitive;
* **directives**: `.org ADDR` sets the assembly address, `.word VALUE` emits
  one word, `.equ NAME VALUE` defines a constant, `.data` and `.code` switch
  between data and instruction sections;
* **comments**, from `;` to the end of the line.

Numbers are hexadecimal by default (`001B` is 27). `$1B`, `0x1B` also mean
hexadecimal, `0d27` is decimal, and a name is a label or a `.equ` constant.

```asm
; ---- data ----------------------------------------------------------------
        .org  08
NUM1:   .word 001B              ; 27
NUM2:   .word 0008              ; 8
TOTAL:  .word 0000
```

The assembler produces three things the debugger uses: the program image, a
mapping from source line to address, and diagnostics. A line that cannot be
assembled becomes an error with a message, not a crash.

### 5.3 Machine code files (`.mc`)

A `.mc` file is the machine code: one 16-bit word per line, four hexadecimal
digits, from address `00` upwards. It is what the window opens and saves, and
the format is deliberately boring so a program can be read without the tool:

```text
3508
3609
6056
400A
E000
0000
```

A file shorter than 256 words leaves the rest of memory at zero. Loading a
longer file is an error: memory cannot be extended.

## 6. Worked example

`examples/01-addition.asm` adds the numbers at addresses `08` and `09` and
stores the total at `0A`. The listing is:

```asm
        LOAD  R5, [NUM1]        ; R5 = 27 decimal
        LOAD  R6, [NUM2]        ; R6 = 8  decimal
        R0 = R5 + R6            ; the adder in the ALU does the work
        STORE R0, [TOTAL]       ; write the result back to memory
        HALT

        .org  08
NUM1:   .word 001B
NUM2:   .word 0008
TOTAL:  .word 0000
```

and the image it assembles to (`examples/01-addition.mc`) is:

| Address | Word | Meaning |
| --- | --- | --- |
| `00` | `3508` | `LOAD R5, [08]` |
| `01` | `3609` | `LOAD R6, [09]` |
| `02` | `6056` | `R0 = R5 + R6` |
| `03` | `400A` | `STORE R0, [0A]` |
| `04` | `E000` | `HALT` |
| `05`–`07` | `0000` | padding from `.org 08` |
| `08` | `001B` | 27 decimal, the first input |
| `09` | `0008` | 8 decimal, the second input |
| `0A` | `0000` | the total, written when the program runs |

Execution, one instruction at a time:

1. `3508` fetches four transfers, then reads memory `08` into `R5`.
2. `3609` reads memory `09` into `R6`.
3. `6056` lights the ALU and writes `001B + 0008` into `R0`, so `R0 = 0023`.
4. `400A` writes `R0` back through `MDR` to memory `0A`.
5. `E000` halts; `PC` stops after the `HALT` and memory `0A` holds `0023`,
   which is 35 decimal.

In the diagram, step 3 is the ALU case (`0023` in `ALU` and `R0` highlighted)
and step 4 is the write case (the bus row flipped to `MDR -> data` and cell
`0A` highlighted). Both are the same transfers the footer names.

## 7. Faults and run limits

The machine stops in three ways:

| Stop | Meaning | What the window shows |
| --- | --- | --- |
| `HALT` | a `HALT` instruction completed its fetch | the halted state, no highlight |
| unknown instruction | the opcode nibble is `0`, `5` or `F` | a fault naming the word and the address |
| step budget | the program has not halted within the allowed number of steps | a fault saying it is still running |

A fault is not a crash: it is part of the machine state, and it can be stepped
back from. The message for an unknown instruction is for example:

```text
unknown instruction 5012 at address 03: the MASSEY machine has no opcode 5
```

The debugger also keeps a bounded history so `back` works; the limit is
20,000 saved states, after which the oldest are dropped. Running is bounded by
a step budget for the same reason: a program with a mistake in its loop should
stop and show where it is, not hang the window.

## 8. Quick reference

```text
opcode  assembly                      encoding     effect
1       LOAD  Rx, YZ                  1 r YZ       Rx = YZ
2       LOAD  Rx, Ry                  2 0 d s      Rx = Ry
3       LOAD  Rx, [YZ]                3 r YZ       Rx = memory[YZ]
4       STORE Rx, [YZ]                4 r YZ       memory[YZ] = Rx
6       Rx = Ry + Rz                  6 d s1 s2    Rx = Ry + Rz
7       NEGATE Rx                     7 0 r 0      Rx = ~Rx + 1
8       SHIFT Rx RIGHT YZ             8 r YZ       Rx = Rx >> YZ
9       SHIFT Rx LEFT YZ              9 r YZ       Rx = Rx << YZ
A       Rx = Ry AND Rz                A d s1 s2    Rx = Ry & Rz
B       Rx = Ry OR Rz                 B d s1 s2    Rx = Ry | Rz
C       Rx = Ry XOR Rz                C d s1 s2    Rx = Ry ^ Rz
D       JUMP [YZ] IF Rx=R0            D r YZ       if Rx == R0 then PC = YZ
E       HALT                          E 0 0 0      stop

fetch:  PC -> MAR, M[MAR] -> MDR, MDR -> IR, PC + 1 -> PC
        then the execute transfers of the opcode, if any
```

## See also

* [ARCHITECTURE.md](ARCHITECTURE.md) — how the model is implemented.
* [DESIGN.md](DESIGN.md) — how the machine cycle is drawn in the window.
* [BUILDING.md](BUILDING.md) — building and packaging MasseyCode.
