//! Tests for the `.mc` assembler, checked against the encodings printed in the
//! 159.102 course notes.

use massey_core::asm::{split_comment, text_to_image};
use massey_core::isa::Opcode;
use massey_core::{assemble, disassemble, Severity};

/// Assemble a single instruction and return the machine word it produced.
fn word(source: &str) -> u16 {
    let program = assemble(source);
    assert!(
        program.is_ok(),
        "expected `{source}` to assemble, got {:?}",
        program.diagnostics
    );
    program.words[0]
}

#[test]
fn encodings_from_the_notes() {
    // The examples printed in Section 4 and Section 5 of the notes.
    assert_eq!(word("LOAD R2, 13"), 0x1213);
    assert_eq!(word("LOAD RA, R7"), 0x20A7);
    assert_eq!(word("LOAD R4, [AB]"), 0x34AB);
    assert_eq!(word("STORE R5, [B1]"), 0x45B1);
    assert_eq!(word("R8 = R2 + R3"), 0x6823);
    assert_eq!(word("NEGATE RA"), 0x70A0);
    assert_eq!(word("SHIFT R4 RIGHT 3"), 0x8403);
    assert_eq!(word("SHIFT R6 LEFT 2"), 0x9602);
    assert_eq!(word("R0 = R4 AND R5"), 0xA045);
    assert_eq!(word("R12 = R7 OR R4".replace("R12", "RC").as_str()), 0xBC74);
    assert_eq!(word("R5 = RF XOR R3"), 0xC5F3);
    assert_eq!(word("JUMP [3C] IF R4=R0"), 0xD43C);
    assert_eq!(word("HALT"), 0xE000);
}

#[test]
fn assembler_spellings_are_flexible() {
    assert_eq!(word("  load   r2 ,  13  "), 0x1213);
    assert_eq!(word("LOAD R2, 0x13 ; a comment"), 0x1213);
    assert_eq!(word("LOAD R2, $13 # another comment"), 0x1213);
    assert_eq!(word("JUMP TO 3C IF R4 = R0"), 0xD43C);
    assert_eq!(
        word(
            "Rx = Ry + Rz"
                .replace('x', "8")
                .replace('y', "2")
                .replace('z', "3")
                .as_str()
        ),
        0x6823
    );
}

#[test]
fn machine_code_words_pass_through() {
    // A machine code image: one hexadecimal word per line.
    let image = "1032 \n11FF \n2071 \n9108 \nB117 \nC201 \n1301 \n6423 \n6504 \nE000 \n";
    let program = assemble(image);
    assert!(program.is_ok(), "{:?}", program.diagnostics);
    assert_eq!(program.used, 10);
    assert_eq!(program.words[0], 0x1032);
    assert_eq!(program.words[2], 0x2071);
    assert_eq!(program.words[9], 0xE000);

    // ... and the same text can be loaded directly.
    let words = text_to_image(image).expect("image should parse");
    assert_eq!(words.len(), 10);
    assert_eq!(words[9], 0xE000);
}

#[test]
fn labels_resolve_to_addresses() {
    let source = "\
START:  LOAD R0, 00
        LOAD R1, [DATA]
        JUMP [START] IF R0=R0
        HALT
DATA:   .word 002A
";
    let program = assemble(source);
    assert!(program.is_ok(), "{:?}", program.diagnostics);
    assert_eq!(program.words[0], 0x1000);
    assert_eq!(program.words[1], 0x3104, "DATA is at address 04");
    assert_eq!(program.words[2], 0xD000, "START is at address 00");
    assert_eq!(program.words[3], 0xE000);
    assert_eq!(program.words[4], 0x002A);
    assert_eq!(program.label_at(0), Some("START"));
    assert_eq!(program.label_at(4), Some("DATA"));
}

#[test]
fn expressions_and_directives_work() {
    let source = "\
        .equ   BASE 10
        .org   BASE
        LOAD R1, [BASE + 2]
        .word  BASE * 2
        HALT
";
    let program = assemble(source);
    assert!(program.is_ok(), "{:?}", program.diagnostics);
    assert_eq!(program.words[0x10], 0x3112, "LOAD R1, [12]");
    assert_eq!(program.words[0x11], 0x0020);
    assert_eq!(program.words[0x12], 0xE000);
}

#[test]
fn source_map_links_lines_to_addresses() {
    let source = "; comment\nLOAD R1, 02\n\nHALT\n";
    let program = assemble(source);
    assert_eq!(program.lines[0].kind, massey_core::LineKind::Comment);
    assert_eq!(program.source_line_for_address(0), Some(1));
    assert_eq!(program.source_line_for_address(1), Some(3));
    assert_eq!(program.address_of_line(1), Some(0));
    assert_eq!(program.address_of_line(3), Some(1));
    assert_eq!(program.address_of_line(2), None);
}

#[test]
fn diagnostics_point_at_the_problem() {
    let program = assemble("LOAD R1, [NOPE]\n");
    assert!(!program.is_ok());
    let diagnostic = program
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("an error diagnostic");
    assert_eq!(diagnostic.line, 0);
    assert!(
        diagnostic.message.contains("NOPE"),
        "message was {:?}",
        diagnostic.message
    );
}

#[test]
fn range_and_register_errors_are_reported() {
    let too_big = assemble("LOAD R1, 1FF\n");
    assert!(!too_big.is_ok());
    assert!(
        too_big
            .diagnostics
            .iter()
            .any(|d| d.message.contains("8 bits")),
        "{:#?}",
        too_big.diagnostics
    );

    let bad_register = assemble("LOAD R15, 01\n");
    assert!(!bad_register.is_ok());
    assert!(
        bad_register
            .diagnostics
            .iter()
            .any(|d| d.message.contains("R0..RF") || d.message.contains("not a register")),
        "{:#?}",
        bad_register.diagnostics
    );

    let duplicate = assemble("A: .word 1\nA: .word 2\n");
    assert!(!duplicate.is_ok());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|d| d.code == "E-DUPLICATE-LABEL"),
        "{:#?}",
        duplicate.diagnostics
    );

    let missing = assemble("LOAD R1\n");
    assert!(!missing.is_ok());
}

#[test]
fn memory_overflow_is_reported() {
    let mut source = String::new();
    for _ in 0..257 {
        source.push_str("HALT\n");
    }
    let program = assemble(&source);
    assert!(!program.is_ok());
    assert!(
        program
            .diagnostics
            .iter()
            .any(|d| d.code == "E-OUT-OF-MEMORY"),
        "{:?}",
        program.diagnostics
    );
    assert_eq!(program.used, 256);
}

#[test]
fn programs_without_halt_are_warned_about() {
    let program = assemble("LOAD R1, 02\n");
    assert!(program.is_ok());
    assert_eq!(program.warning_count(), 1);
    assert_eq!(program.diagnostics[0].code, "W-NO-HALT");
}

#[test]
fn comment_splitting_is_robust() {
    assert_eq!(split_comment("LOAD R1, 02 ; hi").0.trim(), "LOAD R1, 02");
    assert_eq!(split_comment("# whole line").0, "");
    assert_eq!(split_comment("no comment").0, "no comment");
}

#[test]
fn disassembly_round_trips() {
    for source in [
        "LOAD R2, 13",
        "LOAD RA, R7",
        "LOAD R4, [AB]",
        "STORE R5, [B1]",
        "R8 = R2 + R3",
        "NEGATE RA",
        "SHIFT R4 RIGHT 03",
        "SHIFT R6 LEFT 02",
        "R0 = R4 AND R5",
        "RC = R7 OR R4",
        "R5 = RF XOR R3",
        "JUMP [3C] IF R4=R0",
        "HALT",
    ] {
        let program = assemble(source);
        let text = disassemble(program.words[0]);
        let again = assemble(&text);
        assert!(
            again.is_ok(),
            "`{text}` (from `{source}`) should assemble again: {:?}",
            again.diagnostics
        );
        assert_eq!(
            again.words[0], program.words[0],
            "disassembly of `{source}` was `{text}`"
        );
    }
}

#[test]
fn every_opcode_has_metadata() {
    for opcode in Opcode::ALL {
        assert!(!opcode.syntax().is_empty());
        assert!(!opcode.description().is_empty());
        assert!(!opcode.cpp().is_empty());
        assert_eq!(opcode.encoding().len(), 4);
    }
    assert_eq!(Opcode::ALL.len(), 13, "opcodes 0 and 5 do not exist");
}

#[test]
fn the_algorithm_suite_listing_assembles() {
    // `examples/09-algorithm-suite.asm` is the readable listing of a 235 word
    // program: 193 instructions followed by 42 data words.  It exercises labels,
    // `.data`, `.word`, every instruction form and self modifying code.
    let source = include_str!("../../../examples/09-algorithm-suite.asm");
    let program = assemble(source);
    assert!(
        program.is_ok(),
        "the algorithm suite should assemble: {:#?}",
        program
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect::<Vec<_>>()
    );
    assert_eq!(program.used, 235, "the listing describes 235 words");
    assert_eq!(program.words[0], 0x1000, "LOAD R0, 00");
    assert_eq!(program.words[1], 0x1F01, "LOAD RF, 01");
}

/// Write the 235 word algorithm suite out as machine code, for eyeballing the
/// demonstrator with a big program.
#[test]
#[ignore = "writes a file into the target directory"]
fn write_the_algorithm_suite_machine_code() {
    let source = include_str!("../../../examples/09-algorithm-suite.asm");
    let program = massey_core::assemble(source);
    assert!(program.is_ok());
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/mc-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("test-suite.mc");
    std::fs::write(&path, massey_core::image_to_text(program.image())).unwrap();
    println!("wrote {} ({} words)", path.display(), program.used);
}

/// The 235 word algorithm suite in `examples/09-algorithm-suite.asm` is the
/// biggest program in the repository.  Run it and check every result it
/// documents.
#[test]
fn the_algorithm_suite_produces_its_documented_results() {
    use massey_core::{label_address, Machine, RunStop};

    let program = massey_core::assemble(include_str!("../../../examples/09-algorithm-suite.asm"));
    assert!(program.is_ok());
    let mut machine = Machine::new();
    machine.load_image(program.image());
    let stop = machine.run(5_000_000, &[]);
    assert_eq!(stop, RunStop::Halted, "the suite should halt");

    let at = |name: &str| {
        let address = label_address(&program, name).expect(name) as usize;
        machine.memory[address]
    };

    // 1. multiplication: 222 * 291
    assert_eq!(at("MUL_A"), 0x00DE);
    assert_eq!(at("MUL_B"), 0x0123);
    assert_eq!(at("MUL_OUT"), 0xFC5A, "222 * 291 = 64602");

    // 2. division: 3000 / 23 = 130 remainder 10
    assert_eq!(at("DIV_Q"), 0x0082);
    assert_eq!(at("DIV_R"), 0x000A);

    // 3. greatest common divisor of 1071 and 462
    assert_eq!(at("GCD_OUT"), 0x0015, "gcd(1071, 462) = 21");

    // 4. factorial: 8! = 40320
    assert_eq!(at("FACT_OUT"), 0x9D80);

    // 5. sum of the squares 1..20 = 2870
    assert_eq!(at("SUMSQ_OUT"), 0x0B36);

    // 6. the first sixteen Fibonacci numbers, written through a pointer
    let fib = label_address(&program, "FIB").unwrap() as usize;
    let terms: Vec<u16> = (0..16).map(|offset| machine.memory[fib + offset]).collect();
    assert_eq!(
        terms,
        vec![1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987],
        "the Fibonacci terms the pointer walk wrote"
    );

    // 7. the bubble sort put the array in order and reported min, max and total
    let array = label_address(&program, "SORT_ARR").unwrap() as usize;
    let sorted: Vec<u16> = (0..10)
        .map(|offset| machine.memory[array + offset])
        .collect();
    assert_eq!(sorted, vec![3, 5, 11, 14, 26, 37, 51, 68, 80, 92]);
    assert_eq!(at("SORT_MIN"), 3);
    assert_eq!(at("SORT_MAX"), 92);
    assert_eq!(at("SORT_SUM"), 387);

    // 8. the XOR checksum of every result above
    assert_eq!(at("CHK_OUT"), 0x6BF2);
}
