; ===========================================================================
;  Subtraction  --  Section 5 of the 159.102 notes, example 2
;
;  The machine has no subtract instruction, so the second number is negated
;  (complement and add 1, i.e. 2's complement) and then added.
;
;      Machine code          Assembler            C++
;          00  11FC          LOAD  R1, FC        R1 = 0xFC;     252 decimal
;          01  1211          LOAD  R2, 11        R2 = 0x11;      17 decimal
;          02  7020          NEGATE R2           R2 = -R2;      EF hex
;          03  6012          R0 = R1 + R2        R0 = R1 + R2
;          04  4007          STORE R0, [07]      memory[07] = R0;
;          05  E000          HALT
;          07  0000                              result, EB hex = 235 decimal
; ===========================================================================

        LOAD  R1, FC            ; 252 decimal
        LOAD  R2, 11            ; 17 decimal
        NEGATE R2               ; 2's complement: -17 = FFEF hex
        R0 = R1 + R2            ; FC + FFEF = 00EB, the carry out is lost
        STORE R0, [RESULT]
        HALT

; ---- data ----------------------------------------------------------------
        .org  07
RESULT: .word 0000              ; EB hex
