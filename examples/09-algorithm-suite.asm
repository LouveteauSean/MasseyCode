; ===========================================================================
;  MASSEY algorithm suite -- readable listing of the program image test.txt
;  The machine loads the hexadecimal image in test.txt (one word per line);
;  the 235 words below are exactly that image, 193 instructions followed by
;  42 data words.  test.txt cannot carry comments: the loader stops at the
;  first line that is not a hexadecimal word.
;
;  Register conventions used throughout:
;      R0 = 0 (always, needed by every JUMP)        RF = 1
;      RD = FFFF (-1, the decrement constant)       RB = 8000 (sign-bit mask)
;
;  Stages, in the order the machine runs them:
;      1. MUL    16-bit shift-and-add multiplication        MUL_A * MUL_B
;      2. DIV    repeated-subtraction division              DIV_A / DIV_B
;      3. GCD    Euclid's algorithm by subtraction
;      4. STORE/LOAD indirect subroutines (self-modifying code)
;      5. FIB    the first 16 Fibonacci numbers, stored through a pointer
;      6. FACT   8! using an in-line multiply inside a loop
;      7. SUMSQ  1*1 + 2*2 + ... + 20*20  (n*n = sum of the first n odd numbers)
;      8. SORT   bubble sort of ten numbers, in place through pointers,
;                then the minimum, maximum and total of the sorted array
;      9. CHK    an XOR checksum of every result above
; ===========================================================================

START:
	LOAD   R0, 00
	LOAD   RF, 01
	LOAD   RD, 01
	NEGATE RD			; RD = FFFF = -1
	LOAD   RB, 80
	SHIFT  RB LEFT 8		; RB = 8000, the sign-bit mask

; ---- 1. multiplication ----------------------------------------------------
MUL:
	LOAD   R1, [MUL_A]		; R1 = multiplicand
	LOAD   R2, [MUL_B]		; R2 = multiplier
	LOAD   R3, 00			; R3 = running total
	LOAD   RE, 10			; 16 bits to inspect
MUL_LOOP:
	LOAD   R4, R2
	R4 = R4 AND RF			; R4 = last bit of the multiplier
	JUMP   [MUL_SKIP] IF R4=R0
	R3 = R3 + R1			; add the shifted multiplicand
MUL_SKIP:
	SHIFT  R1 LEFT 1
	SHIFT  R2 RIGHT 1
	RE = RE + RD
	JUMP   [MUL_DONE] IF RE=R0
	JUMP   [MUL_LOOP] IF R0=R0
MUL_DONE:
	STORE  R3, [MUL_OUT]

; ---- 2. division ----------------------------------------------------------
DIV:
	LOAD   R1, [DIV_A]		; R1 = what is left of the dividend
	LOAD   R2, [DIV_B]		; R2 = divisor
	LOAD   R3, 00			; R3 = quotient
DIV_LOOP:
	LOAD   R4, R2
	NEGATE R4			; R4 = -divisor
	R5 = R1 + R4			; R5 = dividend - divisor
	R6 = R5 AND RB			; sign bit set?
	JUMP   [DIV_STEP] IF R6=R0	; R5 >= 0, so subtract again
	JUMP   [DIV_DONE] IF R0=R0
DIV_STEP:
	LOAD   R1, R5
	R3 = R3 + RF			; one more divisor fits
	JUMP   [DIV_LOOP] IF R0=R0
DIV_DONE:
	STORE  R3, [DIV_Q]
	STORE  R1, [DIV_R]

; ---- 3. greatest common divisor ------------------------------------------
GCD:
	LOAD   R1, [GCD_A]
	LOAD   R2, [GCD_B]
GCD_LOOP:
	LOAD   R4, R2
	NEGATE R4			; R4 = -b
	R3 = R1 + R4			; R3 = a - b
	JUMP   [GCD_EQ] IF R3=R0	; a == b, so this is the gcd
	R5 = R3 AND RB
	JUMP   [GCD_BIGA] IF R5=R0	; a - b > 0, so b is the smaller one
	LOAD   R7, R1
	NEGATE R7			; R7 = -a
	R2 = R2 + R7			; b = b - a
	JUMP   [GCD_LOOP] IF R0=R0
GCD_BIGA:
	LOAD   R1, R3			; a = a - b
	JUMP   [GCD_LOOP] IF R0=R0
GCD_EQ:
	STORE  R1, [GCD_OUT]
	JUMP   [FIBSTART] IF R0=R0	; the indirect routines below are not entered
					; by accident: only their slots are jumped to

; ---- 4. indirect addressing ----------------------------------------------
; Both routines build the instruction they need, place it in the slot below
; and jump to it: the machine really does change its own program while it
; runs. RC carries the return address and R8 the address being accessed.
STORE_IND:
	LOAD   R9, 41
	SHIFT  R9 LEFT 8		; R9 = 4100 = STORE R1, [00]
	R9 = R9 + R8
	STORE  R9, [SI_SLOT]		; patch the address into the instruction
	LOAD   R9, D0
	SHIFT  R9 LEFT 8		; R9 = D000 = JUMP [00] IF R0=R0
	R9 = R9 + RC
	STORE  R9, [SI_RET]		; patch the return address
	JUMP   [SI_SLOT] IF R0=R0
SI_SLOT:
	STORE  R1, [00]			; patched at run time
SI_RET:
	JUMP   [00] IF R0=R0		; patched at run time

LOAD_IND:
	LOAD   R9, 31
	SHIFT  R9 LEFT 8		; R9 = 3100 = LOAD R1, [00]
	R9 = R9 + R8
	STORE  R9, [LI_SLOT]
	LOAD   R9, D0
	SHIFT  R9 LEFT 8
	R9 = R9 + RC
	STORE  R9, [LI_RET]
	JUMP   [LI_SLOT] IF R0=R0
LI_SLOT:
	LOAD   R1, [00]			; patched at run time
LI_RET:
	JUMP   [00] IF R0=R0		; patched at run time

; ---- 5. Fibonacci numbers through a pointer -------------------------------
FIBSTART:
	LOAD   R1, 01			; R1 = previous term
	LOAD   R2, 01			; R2 = current term
	LOAD   R8, FIB			; R8 = where the next term goes
	LOAD   RE, 10			; 16 terms
FIB_LOOP:
	LOAD   RC, FIB_RET
	JUMP   [STORE_IND] IF R0=R0
FIB_RET:
	R8 = R8 + RF			; point at the next word
	R3 = R1 + R2			; next term
	LOAD   R1, R2
	LOAD   R2, R3
	RE = RE + RD
	JUMP   [FIB_DONE] IF RE=R0
	JUMP   [FIB_LOOP] IF R0=R0
FIB_DONE:

; ---- 6. factorial ---------------------------------------------------------
FACT:
	LOAD   R3, 01			; R3 = running product
	LOAD   RE, 08			; multiply by 8, 7, ... 1
FACT_LOOP:
	LOAD   R1, R3			; multiplicand = running product
	LOAD   R2, RE			; multiplier = loop counter
	LOAD   R5, 00
	LOAD   RA, 10
FACT_MUL:
	LOAD   R4, R2
	R4 = R4 AND RF
	JUMP   [FACT_SKIP] IF R4=R0
	R5 = R5 + R1
FACT_SKIP:
	SHIFT  R1 LEFT 1
	SHIFT  R2 RIGHT 1
	RA = RA + RD
	JUMP   [FACT_MUL_DONE] IF RA=R0
	JUMP   [FACT_MUL] IF R0=R0
FACT_MUL_DONE:
	LOAD   R3, R5
	RE = RE + RD
	JUMP   [FACT_DONE] IF RE=R0
	JUMP   [FACT_LOOP] IF R0=R0
FACT_DONE:
	STORE  R3, [FACT_OUT]

; ---- 7. sum of squares ----------------------------------------------------
; n*n is the sum of the first n odd numbers, so no multiply is needed here.
SUMSQ:
	LOAD   R3, 00			; R3 = total
	LOAD   R1, 01			; R1 = n, from 1
	LOAD   RE, 14			; 20 values
SUMSQ_N:
	LOAD   R5, 00			; R5 = n*n
	LOAD   R2, R1			; R2 = how many odd numbers to add
	LOAD   R6, 01			; R6 = the next odd number
SUMSQ_INNER:
	R5 = R5 + R6
	R6 = R6 + RF
	R6 = R6 + RF			; 1, 3, 5, 7, ...
	R2 = R2 + RD
	JUMP   [SUMSQ_INNER_DONE] IF R2=R0
	JUMP   [SUMSQ_INNER] IF R0=R0
SUMSQ_INNER_DONE:
	R3 = R3 + R5
	R1 = R1 + RF			; n = n + 1
	RE = RE + RD
	JUMP   [SUMSQ_DONE] IF RE=R0
	JUMP   [SUMSQ_N] IF R0=R0
SUMSQ_DONE:
	STORE  R3, [SUMSQ_OUT]

; ---- 8. bubble sort -------------------------------------------------------
SORT:
	LOAD   RE, 09			; nine passes over ten numbers
SORT_OUTER:
	LOAD   R7, RE			; comparisons in this pass
	LOAD   R8, SORT_ARR
SORT_INNER:
	LOAD   RA, R8			; remember where this pair starts
	LOAD   RC, SORT_LA
	JUMP   [LOAD_IND] IF R0=R0
SORT_LA:
	LOAD   R6, R1			; R6 = a, the left hand number
	R8 = R8 + RF
	LOAD   RC, SORT_LB
	JUMP   [LOAD_IND] IF R0=R0
SORT_LB:
	LOAD   R2, R1			; R2 = b, the right hand number
	LOAD   R5, R2
	NEGATE R5
	R3 = R6 + R5			; R3 = a - b
	JUMP   [SORT_NEXT] IF R3=R0	; equal: already in order
	R4 = R3 AND RB
	JUMP   [SORT_SWAP] IF R4=R0	; a - b is positive: a > b, so swap
	JUMP   [SORT_NEXT] IF R0=R0	; a < b: in order
SORT_SWAP:
	LOAD   R1, R2			; put b in the left hand slot
	LOAD   R8, RA
	LOAD   RC, SORT_S1
	JUMP   [STORE_IND] IF R0=R0
SORT_S1:
	LOAD   R1, R6			; and a in the right hand slot
	R8 = R8 + RF
	LOAD   RC, SORT_S2
	JUMP   [STORE_IND] IF R0=R0
SORT_S2:
SORT_NEXT:
	LOAD   R8, RA
	R8 = R8 + RF			; move on to the next pair
	R7 = R7 + RD
	JUMP   [SORT_PASS_DONE] IF R7=R0
	JUMP   [SORT_INNER] IF R0=R0
SORT_PASS_DONE:
	RE = RE + RD
	JUMP   [SORT_DONE] IF RE=R0
	JUMP   [SORT_OUTER] IF R0=R0
SORT_DONE:
	LOAD   R8, SORT_ARR
	LOAD   RC, SORT_GETMIN
	JUMP   [LOAD_IND] IF R0=R0
SORT_GETMIN:
	STORE  R1, [SORT_MIN]
	LOAD   R8, SORT_ARR
	LOAD   R9, 09
	R8 = R8 + R9			; the last slot holds the largest number
	LOAD   RC, SORT_GETMAX
	JUMP   [LOAD_IND] IF R0=R0
SORT_GETMAX:
	STORE  R1, [SORT_MAX]
	LOAD   R3, 00			; R3 = total of the array
	LOAD   R8, SORT_ARR
	LOAD   RE, 0A
SORT_SUM_LOOP:
	LOAD   RC, SORT_SUM_RET
	JUMP   [LOAD_IND] IF R0=R0
SORT_SUM_RET:
	R3 = R3 + R1
	R8 = R8 + RF
	RE = RE + RD
	JUMP   [SORT_SUM_DONE] IF RE=R0
	JUMP   [SORT_SUM_LOOP] IF R0=R0
SORT_SUM_DONE:
	STORE  R3, [SORT_SUM]

; ---- 9. checksum ----------------------------------------------------------
CHK:
	LOAD   R1, [MUL_OUT]
	LOAD   R2, [DIV_Q]
	R1 = R1 XOR R2
	LOAD   R2, [DIV_R]
	R1 = R1 XOR R2
	LOAD   R2, [GCD_OUT]
	R1 = R1 XOR R2
	LOAD   R2, [FACT_OUT]
	R1 = R1 XOR R2
	LOAD   R2, [SUMSQ_OUT]
	R1 = R1 XOR R2
	LOAD   R2, [SORT_SUM]
	R1 = R1 XOR R2
	STORE  R1, [CHK_OUT]
	HALT

; ---- data -----------------------------------------------------------------
.data
MUL_A:     .word 0x00DE			; 222
MUL_B:     .word 0x0123			; 291
MUL_OUT:   .word 0x0000
DIV_A:     .word 0x0BB8			; 3000
DIV_B:     .word 0x0017			; 23
DIV_Q:     .word 0x0000
DIV_R:     .word 0x0000
GCD_A:     .word 0x042F			; 1071
GCD_B:     .word 0x01CE			; 462
GCD_OUT:   .word 0x0000
FACT_OUT:  .word 0x0000
SUMSQ_OUT: .word 0x0000
FIB:       .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
           .word 0x0000
SORT_ARR:  .word 0x0025			; 37
           .word 0x0005			; 5
           .word 0x005C			; 92
           .word 0x000E			; 14
           .word 0x0044			; 68
           .word 0x0003			; 3
           .word 0x0033			; 51
           .word 0x001A			; 26
           .word 0x0050			; 80
           .word 0x000B			; 11
SORT_MIN:  .word 0x0000
SORT_MAX:  .word 0x0000
SORT_SUM:  .word 0x0000
CHK_OUT:   .word 0x0000
