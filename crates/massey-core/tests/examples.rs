//! Every bundled example program is assembled and run, and its documented
//! result is checked.  This is the regression suite that keeps the examples
//! honest: if the machine or the assembler changes, these tests fail.

use massey_core::asm::Program;
use massey_core::examples::{by_file_name, EXAMPLES};
use massey_core::{Machine, RunStop};

/// Assemble a source string, run it to a halt, and return the machine.
fn run_source(name: &str, source: &str, budget: u64) -> Machine {
    let program = Program::assemble(source);
    assert!(
        program.is_ok(),
        "`{name}` should assemble without errors:\n{:#?}",
        program
            .diagnostics
            .iter()
            .filter(|d| d.severity == massey_core::Severity::Error)
            .collect::<Vec<_>>()
    );
    let mut machine = Machine::new();
    machine.load_image(program.image());
    let stop = machine.run(budget, &[]);
    assert!(
        matches!(stop, RunStop::Halted),
        "`{name}` stopped with {stop:?} after {} instructions",
        machine.instructions
    );
    assert!(
        machine.fault.is_none(),
        "`{name}` faulted: {:?}",
        machine.fault
    );
    machine
}

/// Assemble and run one of the bundled examples.
fn run_example(file_name: &str, budget: u64) -> Machine {
    let example = by_file_name(file_name).expect("example should exist");
    run_source(example.file_name, example.source, budget)
}

#[test]
fn every_example_assembles_without_errors() {
    for example in EXAMPLES {
        let program = Program::assemble(example.source);
        let errors: Vec<_> = program
            .diagnostics
            .iter()
            .filter(|d| d.severity == massey_core::Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{} ({}) has assembler errors: {errors:#?}",
            example.title,
            example.file_name
        );
        assert!(program.used > 0, "{} assembled to nothing", example.title);
        assert!(
            program.used <= 256,
            "{} needs {} words, more than memory holds",
            example.title,
            program.used
        );
    }
}

#[test]
fn every_example_runs_to_a_halt() {
    for example in EXAMPLES {
        run_source(example.file_name, example.source, 2_000_000);
    }
}

#[test]
fn example_1_addition_stores_the_total() {
    let machine = run_example("01-addition.mc", 1000);
    // memory[0A] = 0x1B + 0x08 = 0x23
    assert_eq!(machine.memory[0x0A], 0x0023);
    assert_eq!(machine.registers[5], 0x001B);
    assert_eq!(machine.registers[6], 0x0008);
}

#[test]
fn example_2_subtraction_uses_twos_complement() {
    let machine = run_example("02-subtraction.mc", 1000);
    assert_eq!(machine.memory[0x07], 0x00EB, "0xFC - 0x11 = 0xEB");
    assert_eq!(machine.registers[2], 0xFFEF, "NEGATE 0x11 is 0xFFEF");
}

#[test]
fn example_3_builds_an_instruction_while_running() {
    let machine = run_example("03-self-modifying.mc", 1000);
    // The program wrote "R6 = R4 + R5" into the slot at address 07 ...
    assert_eq!(machine.memory[0x07], 0x6645, "the instruction it built");
    // ... executed it, and stored the result at SCRATCH.
    assert_eq!(machine.memory[0x0A], 0x0009, "4 + 5");
}

#[test]
fn example_4_if_else_takes_the_true_branch() {
    let machine = run_example("04-if-else.mc", 1000);
    assert_eq!(machine.memory[0x0E], 0x0002, "19 >= 18 so result = 2");
}

#[test]
fn example_5_loop_runs_until_x_reaches_twelve() {
    let machine = run_example("05-loop.mc", 100_000);
    assert_eq!(machine.memory[0x65], 0x000C, "x = 12 decimal");
    assert_eq!(machine.memory[0x75], 0x000C, "y = x");
}

#[test]
fn example_6_multiplies_by_shift_and_add() {
    let machine = run_example("06-multiplication.mc", 10_000);
    let product_address = 0x00;
    let program = Program::assemble(by_file_name("06-multiplication.mc").unwrap().source);
    let address = massey_core::label_address(&program, "PRODUCT").unwrap();
    let _ = product_address;
    assert_eq!(machine.memory[address as usize], 0x000F, "3 * 5 = 15");
}

#[test]
fn example_7_shows_off_the_alu() {
    let machine = run_example("07-bitwise.mc", 1000);
    let program = Program::assemble(by_file_name("07-bitwise.mc").unwrap().source);
    let at =
        |name: &str| machine.memory[massey_core::label_address(&program, name).unwrap() as usize];
    assert_eq!(at("AND_OUT"), 0x000F, "0F0F AND 00FF");
    assert_eq!(at("OR_OUT"), 0x0FFF, "0F0F OR 00FF");
    assert_eq!(at("XOR_OUT"), 0x0FF0, "0F0F XOR 00FF");
    assert_eq!(at("NEG_OUT"), 0xF0F1, "-0F0F in two's complement");
    assert_eq!(at("SHL_OUT"), 0x0F00, "0F0F << 8");
    assert_eq!(at("SHR_OUT"), 0x000F, "00FF >> 4");
}

#[test]
fn example_8_sorts_ten_numbers_and_reports_them() {
    let machine = run_example("08-sort.mc", 500_000);
    let program = Program::assemble(by_file_name("08-sort.mc").unwrap().source);
    let array = massey_core::label_address(&program, "ARRAY").unwrap() as usize;
    let sorted: Vec<u16> = (0..10)
        .map(|offset| machine.memory[array + offset])
        .collect();
    assert_eq!(
        sorted,
        vec![0x0003, 0x0005, 0x000B, 0x000E, 0x001A, 0x0025, 0x0033, 0x0044, 0x0050, 0x005C],
        "the array should be in ascending order"
    );

    let at =
        |name: &str| machine.memory[massey_core::label_address(&program, name).unwrap() as usize];
    assert_eq!(at("MINIMUM"), 0x0003);
    assert_eq!(at("MAXIMUM"), 0x005C);
    assert_eq!(at("TOTAL"), 0x0183, "the ten numbers add up to 387 decimal");
}

#[test]
fn the_machine_changed_its_own_program() {
    // The sort routine patches its own instruction slots, so after the run the
    // slots no longer hold the placeholder that was assembled.
    let program = Program::assemble(by_file_name("08-sort.mc").unwrap().source);
    let machine = {
        let mut machine = Machine::new();
        machine.load_image(program.image());
        machine.run(500_000, &[]);
        machine
    };
    let slot = massey_core::label_address(&program, "SI_SLOT").unwrap();
    assert_ne!(
        machine.memory[slot as usize], 0x4100,
        "the store slot should have been patched while the program ran"
    );
}

/// The `.mc` file next to each `.asm` listing must hold exactly the machine code
/// that the listing assembles to.
#[test]
fn the_shipped_machine_code_matches_the_listings() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for example in EXAMPLES {
        let path = directory.join(example.file_name);
        let expected = massey_core::examples::machine_code(example);
        match std::fs::read_to_string(&path) {
            Ok(actual) => assert_eq!(
                actual.replace("\r\n", "\n"),
                expected,
                "{} is out of date: regenerate it from {}",
                example.file_name,
                example.listing_name
            ),
            Err(error) => panic!("{} cannot be read: {error}", path.display()),
        }
    }
}

/// Regenerate the shipped `.mc` files from the `.asm` listings.
///
/// Run with `cargo test -p massey-core --test examples -- --ignored regenerate`
/// after editing a listing.
#[test]
#[ignore = "writes files into the repository; run explicitly"]
fn regenerate_shipped_machine_code() {
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for example in EXAMPLES {
        let path = directory.join(example.file_name);
        std::fs::write(&path, massey_core::examples::machine_code(example)).unwrap();
        println!("wrote {}", path.display());
    }
}
