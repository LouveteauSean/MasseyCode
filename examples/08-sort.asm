; ===========================================================================
;  Bubble sort of ten numbers  --  the machine changes its own program
;
;  This program sorts ten numbers into ascending order, in place, and then
;  reports the smallest, the largest and the total of the sorted array.
;
;  The interesting part is how it reaches the array at all.  The MASSEY
;  machine only has absolute addressing: LOAD R1, [0A] can only ever name
;  address 0A.  A loop over an array therefore has to build the instruction it
;  needs while it runs.  Two little subroutines do that:
;
;      STORE_IND   memory[R8] = R1        built from 4100 = "STORE R1, [00]"
;      LOAD_IND    R1 = memory[R8]        built from 3100 = "LOAD R1, [00]"
;
;  Both of them patch the address into their own instruction slot, patch a
;  JUMP into their return slot and then jump at the slot they have just
;  written.  RC carries the return address, R8 the address being accessed and
;  R1 the data.
;
;  Register conventions, set up once at the start and never changed:
;
;      R0 = 0000   every JUMP compares a register with R0
;      RF = 0001   to move a pointer on and to count
;      RD = FFFF   -1, to count down
;      RB = 8000   the sign bit, to test whether a difference is negative
;
;  After the run the array at ARRAY holds 0003 0005 000B 000E 001A 0025 0033
;  0044 0050 005C, MINIMUM holds 0003, MAXIMUM holds 005C and TOTAL holds
;  0183 (387 decimal).
; ===========================================================================

START:
        LOAD  R0, 00            ; R0 = 0
        LOAD  RF, 01            ; RF = 1
        LOAD  RD, 01
        NEGATE RD               ; RD = FFFF = -1
        LOAD  RB, 80
        SHIFT RB LEFT 8         ; RB = 8000, the sign bit

        JUMP  [SORT] IF R0=R0   ; the indirect routines are only entered by name

; ---- indirect store: memory[R8] = R1 -------------------------------------
STORE_IND:
        LOAD  R9, 41
        SHIFT R9 LEFT 8         ; R9 = 4100, the opcode part of STORE R1, [00]
        R9 = R9 + R8            ; patch in the address to write to
        STORE R9, [SI_SLOT]     ; the instruction now says STORE R1, [R8]
        LOAD  R9, D0
        SHIFT R9 LEFT 8         ; R9 = D000, the opcode part of JUMP [00]
        R9 = R9 + RC            ; patch in the return address
        STORE R9, [SI_RET]
        JUMP  [SI_SLOT] IF R0=R0    ; run the instruction just built

SI_SLOT:
        STORE R1, [00]          ; patched at run time
SI_RET:
        JUMP  [00] IF R0=R0     ; patched at run time

; ---- indirect load: R1 = memory[R8] --------------------------------------
LOAD_IND:
        LOAD  R9, 31
        SHIFT R9 LEFT 8         ; R9 = 3100, the opcode part of LOAD R1, [00]
        R9 = R9 + R8
        STORE R9, [LI_SLOT]
        LOAD  R9, D0
        SHIFT R9 LEFT 8
        R9 = R9 + RC
        STORE R9, [LI_RET]
        JUMP  [LI_SLOT] IF R0=R0

LI_SLOT:
        LOAD  R1, [00]          ; patched at run time
LI_RET:
        JUMP  [00] IF R0=R0     ; patched at run time

; ---- bubble sort ---------------------------------------------------------
SORT:
        LOAD  RE, 09            ; nine passes over ten numbers

OUTER:
        LOAD  R7, RE            ; this pass makes RE comparisons
        LOAD  R8, ARRAY         ; R8 walks along the array

INNER:
        LOAD  RA, R8            ; remember where this pair starts
        LOAD  RC, GOT_A
        JUMP  [LOAD_IND] IF R0=R0

GOT_A:
        LOAD  R6, R1            ; R6 = a, the left hand number
        R8 = R8 + RF            ; point at the right hand number
        LOAD  RC, GOT_B
        JUMP  [LOAD_IND] IF R0=R0

GOT_B:
        LOAD  R2, R1            ; R2 = b, the right hand number
        LOAD  R5, R2
        NEGATE R5               ; R5 = -b
        R3 = R6 + R5            ; R3 = a - b
        JUMP  [NEXT] IF R3=R0   ; a = b, already in order
        R4 = R3 AND RB          ; is a - b negative?
        JUMP  [SWAP] IF R4=R0   ; no: a > b, so swap them
        JUMP  [NEXT] IF R0=R0   ; yes: a < b, leave them alone

SWAP:
        LOAD  R1, R2            ; write b into the left hand slot
        LOAD  R8, RA
        LOAD  RC, S1
        JUMP  [STORE_IND] IF R0=R0

S1:
        LOAD  R1, R6            ; and a into the right hand slot
        R8 = R8 + RF
        LOAD  RC, S2
        JUMP  [STORE_IND] IF R0=R0

S2:
NEXT:
        LOAD  R8, RA
        R8 = R8 + RF            ; move on to the next pair
        R7 = R7 + RD            ; one fewer comparison to do
        JUMP  [PASS_DONE] IF R7=R0
        JUMP  [INNER] IF R0=R0

PASS_DONE:
        RE = RE + RD            ; one fewer pass to do
        JUMP  [SORTED] IF RE=R0
        JUMP  [OUTER] IF R0=R0

; ---- report the smallest, largest and total ------------------------------
SORTED:
        LOAD  R8, ARRAY
        LOAD  RC, GOT_MIN
        JUMP  [LOAD_IND] IF R0=R0

GOT_MIN:
        STORE R1, [MINIMUM]     ; the first number is the smallest

        LOAD  R8, ARRAY
        LOAD  R9, 09
        R8 = R8 + R9            ; the last slot holds the largest number
        LOAD  RC, GOT_MAX
        JUMP  [LOAD_IND] IF R0=R0

GOT_MAX:
        STORE R1, [MAXIMUM]

        LOAD  R3, 00            ; R3 = running total
        LOAD  R8, ARRAY
        LOAD  RE, 0A            ; ten numbers to add up

SUM_LOOP:
        LOAD  RC, SUM_RET
        JUMP  [LOAD_IND] IF R0=R0

SUM_RET:
        R3 = R3 + R1
        R8 = R8 + RF
        RE = RE + RD
        JUMP  [SUM_DONE] IF RE=R0
        JUMP  [SUM_LOOP] IF R0=R0

SUM_DONE:
        STORE R3, [TOTAL]
        HALT

; ---- data ----------------------------------------------------------------
ARRAY:   .word 0025             ; 37
         .word 0005             ; 5
         .word 005C             ; 92
         .word 000E             ; 14
         .word 0044             ; 68
         .word 0003             ; 3
         .word 0033             ; 51
         .word 001A             ; 26
         .word 0050             ; 80
         .word 000B             ; 11
MINIMUM: .word 0000             ; becomes 0003
MAXIMUM: .word 0000             ; becomes 005C
TOTAL:   .word 0000             ; becomes 0183
