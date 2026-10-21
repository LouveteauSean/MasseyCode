; ===========================================================================
;  A loop  --  Section 5 of the 159.102 notes, example 5
;
;      int x = 0, y;
;      while (x < 10) { x = x + 3; }
;      y = x;
;
;  Machine code has no loops: a loop is a JUMP that goes backwards.  The test
;  `x < 10` is done by computing 10 - x and looking at the sign bit.
;
;  Registers are set up once at the start and then never change:
;      R0 = 0     needed by every JUMP
;      RF = 3     the value added to x each time round
;      RE = 0A    ten, the loop limit
;      RD = 8000  the sign mask
;
;  The variables live in memory: x at 65, y at 75.
;
;      Address Contents  Assembler
;         00    1000      LOAD  R0, 00
;         01    1F03      LOAD  RF, 03
;         02    1E0A      LOAD  RE, 0A
;         03    1D80      LOAD  RD, 80
;         04    9D08      SHIFT RD LEFT 8
;         05    1100      LOAD  R1, 00
;         06    4165      STORE R1, [65]      x = 0
;         07    3165      LOAD  R1, [65]      start of the loop test
;         08    2021      LOAD  R2, R1
;         09    7020      NEGATE R2
;         0A    622E      R2 = R2 + RE        R2 = 10 - x
;         0B    A32D      R3 = R2 AND RD
;         0C    D30E      JUMP  [0E] IF R3=R0   x < 10, stay in the loop
;         0D    D011      JUMP  [11] IF R0=R0   x >= 10, leave the loop
;         0E    611F      R1 = R1 + RF        x = x + 3
;         0F    4165      STORE R1, [65]
;         10    D007      JUMP  [07] IF R0=R0   back to the test
;         11    4175      STORE R1, [75]      y = x
;         12    E000      HALT
; ===========================================================================

        LOAD  R0, 00            ; R0 = 0 for the JUMP comparisons
        LOAD  RF, 03            ; RF = 3, added to x each time round
        LOAD  RE, 0A            ; RE = 10, the loop limit
        LOAD  RD, 80
        SHIFT RD LEFT 8         ; RD = 8000, the sign mask

        LOAD  R1, 00            ; R1 is the variable x
        STORE R1, [X]

LOOP:   LOAD  R1, [X]           ; load x
        LOAD  R2, R1
        NEGATE R2               ; R2 = -x
        R2 = R2 + RE            ; R2 = 10 - x
        R3 = R2 AND RD          ; sign bit of (10 - x)
        JUMP  [BODY] IF R3=R0   ; 10 - x is positive, so x < 10: loop again
        JUMP  [DONE] IF R0=R0   ; otherwise leave the loop

BODY:   R1 = R1 + RF            ; x = x + 3
        STORE R1, [X]
        JUMP  [LOOP] IF R0=R0   ; back to the test

DONE:   STORE R1, [Y]           ; y = x
        HALT

        .org  65                ; the notes keep the variables far from the code
X:      .word 0000              ; x, ending up as 0C (12 decimal)
        .org  75
Y:      .word 0000              ; y, the same value
