; ===========================================================================
;  Addition  --  Section 5 of the 159.102 notes, example 1
;
;      R5 = memory[08]
;      R6 = memory[09]
;      R0 = R5 + R6
;      memory[0A] = R0
;
;  Machine code          Assembler            C++
;      00  3508          LOAD  R5, [08]      R5 = memory[08];
;      01  3609          LOAD  R6, [09]      R6 = memory[09];
;      02  6056          R0 = R5 + R6        R0 = R5 + R6;
;      03  400A          STORE R0, [0A]      memory[0A] = R0;
;      04  E000          HALT
;      08  001B                              27
;      09  0008                              8
;      0A  0000                              the total, 23 hex = 35 decimal
;
;  Press Assemble (F7) then Run (F5): the total 0023 appears at address 0A.
; ===========================================================================

        LOAD  R5, [NUM1]        ; R5 = 27 decimal
        LOAD  R6, [NUM2]        ; R6 = 8  decimal
        R0 = R5 + R6            ; the adder in the ALU does the work
        STORE R0, [TOTAL]       ; write the result back to memory
        HALT

; ---- data ----------------------------------------------------------------
        .org  08                ; the notes keep the data at 08 and above
NUM1:   .word 001B              ; 27
NUM2:   .word 0008              ; 8
TOTAL:  .word 0000              ; 23 hex = 35 decimal once the program has run
