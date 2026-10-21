//! Bundled example programs.
//!
//! Every example exists twice in the repository:
//!
//! * `examples/NN-name.asm` — the readable assembler listing, with the comments
//!   and labels of the course notes.  This is what the examples menu opens.
//! * `examples/NN-name.mc` — the machine code that listing assembles to, one
//!   hexadecimal word per line.  This is what a `.mc` file holds, and the test
//!   suite checks that the two always agree.
//!
//! The listing text is embedded in the binary, so the demonstrator always has
//! something to show even when it is run from somewhere else.  The document it
//! opens is named after the `.mc` file, so saving writes machine code.

/// One bundled example program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Example {
    /// File name of the machine code, e.g. `01-addition.mc`.
    pub file_name: &'static str,
    /// File name of the assembler listing, e.g. `01-addition.asm`.
    pub listing_name: &'static str,
    /// Title shown in the examples menu.
    pub title: &'static str,
    /// One line summary of what the program does.
    pub summary: &'static str,
    /// The assembler listing.
    pub source: &'static str,
}

/// Every bundled example, in teaching order.
pub const EXAMPLES: &[Example] = &[
    Example {
        file_name: "01-addition.mc",
        listing_name: "01-addition.asm",
        title: "1. Addition",
        summary: "Adds two numbers from memory and stores the total (Section 5, example 1).",
        source: include_str!("../../../examples/01-addition.asm"),
    },
    Example {
        file_name: "02-subtraction.mc",
        listing_name: "02-subtraction.asm",
        title: "2. Subtraction",
        summary: "Subtracts by negating and adding (Section 5, example 2).",
        source: include_str!("../../../examples/02-subtraction.asm"),
    },
    Example {
        file_name: "03-self-modifying.mc",
        listing_name: "03-self-modifying.asm",
        title: "3. Self modifying code",
        summary: "The program builds an instruction while it runs (Section 5, example 3).",
        source: include_str!("../../../examples/03-self-modifying.asm"),
    },
    Example {
        file_name: "04-if-else.mc",
        listing_name: "04-if-else.asm",
        title: "4. IF - ELSE",
        summary: "Compares two numbers and stores 1 or 2 (Section 5, example 4).",
        source: include_str!("../../../examples/04-if-else.asm"),
    },
    Example {
        file_name: "05-loop.mc",
        listing_name: "05-loop.asm",
        title: "5. Loop",
        summary: "while (x < 10) x = x + 3; written in machine code (Section 5, example 5).",
        source: include_str!("../../../examples/05-loop.asm"),
    },
    Example {
        file_name: "06-multiplication.mc",
        listing_name: "06-multiplication.asm",
        title: "6. Multiplication",
        summary: "Shift and add multiplication: 3 * 5 (Section 5, example 6).",
        source: include_str!("../../../examples/06-multiplication.asm"),
    },
    Example {
        file_name: "07-bitwise.mc",
        listing_name: "07-bitwise.asm",
        title: "7. AND, OR, XOR and SHIFT",
        summary: "Every ALU instruction of the machine in one short program.",
        source: include_str!("../../../examples/07-bitwise.asm"),
    },
    Example {
        file_name: "08-sort.mc",
        listing_name: "08-sort.asm",
        title: "8. Bubble sort",
        summary: "Sorts ten numbers in place, then finds the minimum, maximum and total.",
        source: include_str!("../../../examples/08-sort.asm"),
    },
    Example {
        file_name: "09-algorithm-suite.mc",
        listing_name: "09-algorithm-suite.asm",
        title: "9. The algorithm suite",
        summary: "235 words: multiplication, division, GCD, factorials, Fibonacci, a sort and a checksum.",
        source: include_str!("../../../examples/09-algorithm-suite.asm"),
    },
];

/// Look up an example by the name of either of its files.
pub fn by_file_name(file_name: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|example| {
        example.file_name.eq_ignore_ascii_case(file_name)
            || example.listing_name.eq_ignore_ascii_case(file_name)
    })
}

/// The example that the application opens on a cold start.
pub fn default_example() -> &'static Example {
    &EXAMPLES[0]
}

/// The machine code of an example, as a `.mc` file would hold it.
pub fn machine_code(example: &Example) -> String {
    crate::asm::image_to_text(crate::Program::assemble(example.source).image())
}
