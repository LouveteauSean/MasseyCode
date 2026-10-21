; ===========================================================================
;  The arithmetic logic unit  --  AND, OR, XOR, NEGATE and SHIFT
;
;  Every instruction that "activates circuits in the ALU" in one program.  The
;  two starting values are
;
;      A = 0F0F        0000 1111 0000 1111
;      B = 00FF        0000 0000 1111 1111
;
;  and the results, all of which are stored in memory, are
;
;      A AND B  = 000F     only the bits set in both numbers survive
;      A OR  B  = 0FFF     the bits set in either number
;      A XOR  B  = 0FF0    the bits that differ
;      -A       = F0F1    two's complement ("complement and add 1")
;      A << 8   = 0F00    shift left by 8 bits = 2 hexadecimal digits
;      B >> 4   = 000F    shift right by 4 bits = 1 hexadecimal digit
;
;  Watch the ALU pane while stepping: it shows the two operands that were fed
;  to the adder or the logic unit and the result that came out.
; ===========================================================================

        LOAD  R0, 00            ; R0 = 0
        LOAD  R1, [A]
        LOAD  R2, [B]

        R3 = R1 AND R2          ; bits set in both
        STORE R3, [AND_OUT]

        R3 = R1 OR R2           ; bits set in either
        STORE R3, [OR_OUT]

        R3 = R1 XOR R2          ; bits that differ
        STORE R3, [XOR_OUT]

        LOAD  R4, R1
        NEGATE R4               ; two's complement: R4 = -A
        STORE R4, [NEG_OUT]

        LOAD  R5, R1
        SHIFT R5 LEFT 8         ; move the low byte into the high byte
        STORE R5, [SHL_OUT]

        LOAD  R6, R2
        SHIFT R6 RIGHT 4        ; 00FF >> 4 = 000F
        STORE R6, [SHR_OUT]

        HALT

; ---- data ----------------------------------------------------------------
A:       .word 0F0F
B:       .word 00FF
AND_OUT: .word 0000             ; 000F
OR_OUT:  .word 0000             ; 0FFF
XOR_OUT: .word 0000             ; 0FF0
NEG_OUT: .word 0000             ; F0F1
SHL_OUT: .word 0000             ; 0F00
SHR_OUT: .word 0000             ; 000F
