; ===========================================================================
;  Self modifying code  --  Section 5 of the 159.102 notes, example 3
;
;  Instructions are just numbers in memory, so a program can build an
;  instruction while it is running and then execute it.  The slot at address 07
;  starts life as 0000; the program writes 6645 into it, which is the
;  instruction
;
;      6645      R6 = R4 + R5
;
;  and the very next thing the machine does is execute it.
;
;      Address  Contents   Assembler
;         00     1404      LOAD  R4, 04        R4 = 4;
;         01     1505      LOAD  R5, 05        R5 = 5;
;         02     1066      LOAD  R0, 66        R0 = 0x66;
;         03     9008      SHIFT R0 LEFT 8     R0 = R0 << 8;   R0 = 6600
;         04     1145      LOAD  R1, 45        R1 = 0x45;
;         05     6201      R2 = R0 + R1        R2 = 6645
;         06     4207      STORE R2, [07]      memory[07] = R2;  builds 6645
;         07     0000      ???                 becomes R6 = R4 + R5
;         08     460A      STORE R6, [0A]      memory[0A] = R6;  R6 = 9
;         09     E000      HALT
;         0A     0000                          9 once the program has run
; ===========================================================================

        LOAD  R4, 04            ; 4
        LOAD  R5, 05            ; 5
        LOAD  R0, 66            ; the top two digits of the instruction to build
        SHIFT R0 LEFT 8         ; 6600 -- SHIFT moves the digits into place
        LOAD  R1, 45            ; the bottom two digits
        R2 = R0 + R1            ; 6645 = "R6 = R4 + R5"
        STORE R2, [SLOT]        ; write the new instruction into memory

SLOT:   .word 0000              ; executed next: R6 = R4 + R5 = 9

        STORE R6, [SCRATCH]     ; prove that the created instruction ran
        HALT

SCRATCH: .word 0000             ; becomes 0009
