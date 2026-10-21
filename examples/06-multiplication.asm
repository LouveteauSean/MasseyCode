; ===========================================================================
;  Multiplication by shift and add  --  Section 5, example 6
;
;  The MASSEY machine has no multiply instruction, so multiplication is done
;  the way you were taught long multiplication at school, but in binary:
;
;      * look at the last bit of the multiplier
;      * if it is 1, add the (shifted) multiplicand to the running total
;      * shift the multiplicand one bit left and the multiplier one bit right
;      * repeat until the multiplier is zero
;
;      Registers:  R0 = 0        needed by every JUMP
;                  R1 = 03       the multiplicand
;                  R2 = 05       the multiplier
;                  R3            the running total of the product
;                  R4            the last bit of R2
;                  R5 = 01       used to extract the last bit with AND
;
;      Address Contents  Assembler
;         00    1000      LOAD  R0, 00
;         01    1501      LOAD  R5, 01
;         02    1103      LOAD  R1, 03       3
;         03    1205      LOAD  R2, 05       5
;         04    1300      LOAD  R3, 00       running total
;         05    A425      R4 = R2 AND R5   last bit of the multiplier
;         06    D408      JUMP  [08] IF R4=R0   bit is zero: do not add
;         07    6331      R3 = R3 + R1     bit is one: add the multiplicand
;         08    9101      SHIFT R1 LEFT 1
;         09    8201      SHIFT R2 RIGHT 1
;         0A    D20C      JUMP  [0C] IF R2=R0   finished?
;         0B    D005      JUMP  [05] IF R0=R0   no: go round again
;         0C    E000      HALT
;
;  3 * 5 = 15, so R3 ends up holding 000F.  Change the two numbers on the
;  LOAD R1 / LOAD R2 lines and press Run again to try other products.
; ===========================================================================

        LOAD  R0, 00            ; R0 = 0, used by every JUMP
        LOAD  R5, 01            ; R5 = 1, the bit mask
        LOAD  R1, [FACTOR_A]    ; multiplicand
        LOAD  R2, [FACTOR_B]    ; multiplier
        LOAD  R3, 00            ; running total of the product

MUL_LOOP:
        R4 = R2 AND R5          ; last bit of the multiplier
        JUMP  [SKIP_ADD] IF R4=R0
        R3 = R3 + R1            ; that bit was 1: add the shifted multiplicand

SKIP_ADD:
        SHIFT R1 LEFT 1         ; ready for the next bit
        SHIFT R2 RIGHT 1
        JUMP  [DONE] IF R2=R0   ; all the bits have been shifted out
        JUMP  [MUL_LOOP] IF R0=R0

DONE:   STORE R3, [PRODUCT]
        HALT

; ---- data ----------------------------------------------------------------
FACTOR_A: .word 0003            ; 3
FACTOR_B: .word 0005            ; 5
PRODUCT:  .word 0000            ; 000F = 15 decimal
