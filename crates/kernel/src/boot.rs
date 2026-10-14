//! Demo guest + boot helpers.
//!
//! Demo program (entry-relative, position-free):
//!   addiu $3,$0,'H' (0x48) ; BREAK 0 (putc path uses service 0 via SYSCALL)
//! Kept tiny so `run_until_exit` stays green from M1.

use n64_cpu::TrapKind;

/// Words for the HI demo: putc 'H', putc 'I', exit 0.
/// Encoded as: addiu $4,$0,'H' ; syscall 0 ; addiu $4,$0,'I' ; syscall 0 ;
///             addiu $4,$0,0 ; syscall 1 ; break 0
pub fn demo_guest_words() -> Vec<u32> {
    vec![
        0x2404_0048, // addiu $a0,$0,'H'
        0x0000_000C, // syscall 0
        0x2404_0049, // addiu $a0,$0,'I'
        0x0000_000C, // syscall 0
        0x0000_000C, // syscall 0 (third char path unused, keeps alignment simple)
        0x0000_200D, // break (code>>6 = 0x80? no: funct 0x0D, code field)
    ]
}

/// Render guest: fills framebuffer via RDP path (words live in kernel for
/// the frontend staged boot; real pixel work happens in gpu).
pub fn render_guest_words() -> Vec<u32> {
    vec![
        0x2404_0048, // addiu $a0,$0,'H'
        0x0000_000C, // syscall 0
        0x0000_200D, // break 0x80
    ]
}

pub fn trap_is_exit(kind: TrapKind, code: u32) -> bool {
    matches!(kind, TrapKind::Break) || (matches!(kind, TrapKind::Syscall) && code == 1)
}
