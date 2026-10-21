; ===========================================================================
;  IF - ELSE  --  Section 5 of the 159.102 notes, example 4
;
;      int num = 19, result;
;      if (num >= 18) result = 2; else result = 1;
;
;  The machine has no compare instruction, so the test is done by subtracting
;  and looking at the sign bit:
;
;      * NEGATE R2 makes R2 = -18
;      * R3 = R1 + R2 is 19 - 18, a positive number
;      * the sign bit of a positive number is 0, so `R5 = R3 AND R4` is 0
;
;  R4 is the sign mask 8000, built with SHIFT R4 LEFT 8.
;
;      Address Contents  Assembler
;         00    1113      LOAD  R1, 13        R1 = 19 decimal
;         01    1212      LOAD  R2, 12        R2 = 18 decimal
;         02    7020      NEGATE R2           R2 = -18
;         03    6312      R3 = R1 + R2        R3 = 1  (19 - 18)
;         04    1480      LOAD  R4, 80
;         05    9408      SHIFT R4 LEFT 8     R4 = 8000, the sign bit
;         06    A534      R5 = R3 AND R4      R5 = 0 when R3 is not negative
;         07    1000      LOAD  R0, 00        R0 = 0, needed by every JUMP
;         08    D50B      JUMP  [0B] IF R5=R0   jump when num >= 18
;         09    1101      LOAD  R1, 01        else result = 1
;         0A    D00C      JUMP  [0C] IF R0=R0   always jump
;         0B    1102      LOAD  R1, 02        true section: result = 2
;         0C    410E      STORE R1, [0E]
;         0D    E000      HALT
;         0E    0000                           02
;
;  Notice how the "true" section (0B) sits after the "false" section (09) in
;  memory, which is why the JUMP instructions look backwards.
; ===========================================================================

        LOAD  R1, 13            ; num     = 19 decimal
        LOAD  R2, 12            ; compare = 18 decimal
        NEGATE R2               ; R2 = -18
        R3 = R1 + R2            ; R3 = num - 18
        LOAD  R4, 80
        SHIFT R4 LEFT 8         ; R4 = 8000: only the sign bit is set
        R5 = R3 AND R4          ; R5 = 0 when the result is not negative
        LOAD  R0, 00            ; every JUMP compares a register with R0

        JUMP  [TRUE] IF R5=R0   ; num - 18 >= 0, so take the true branch
        LOAD  R1, 01            ; else section: result = 1
        JUMP  [STORE_IT] IF R0=R0   ; R0 always equals R0: jump always

TRUE:   LOAD  R1, 02            ; true section: result = 2
STORE_IT:
        STORE R1, [RESULT]
        HALT

RESULT: .word 0000              ; becomes 0002 because 19 >= 18
