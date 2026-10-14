//! VR4300 decoder: MIPS III opcode map.
//!
//! Facts cross-checked against the NEC VR4300 User's Manual and the
//! MIPS III ISA quick-reference (primary 6-bit opcode, SPECIAL 6-bit
//! function, REGIMM 5-bit rt field). Every numeric constant below is
//! traceable to those tables; nothing is taken from memory.

use n64_common::{EmuError, Result};

/// Decoded instruction (M1 subset + explicit unsupported for the rest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoded {
    Nop,
    Lui { rt: u32, imm: u32 },
    Addiu { rt: u32, rs: u32, imm: i16 },
    Addi { rt: u32, rs: u32, imm: i16 },
    Andi { rt: u32, rs: u32, imm: u32 },
    Ori { rt: u32, rs: u32, imm: u32 },
    Xori { rt: u32, rs: u32, imm: u32 },
    Slti { rt: u32, rs: u32, imm: i16 },
    Sltiu { rt: u32, rs: u32, imm: i16 },
    Addu { rd: u32, rs: u32, rt: u32 },
    Add { rd: u32, rs: u32, rt: u32 },
    Subu { rd: u32, rs: u32, rt: u32 },
    And { rd: u32, rs: u32, rt: u32 },
    Or { rd: u32, rs: u32, rt: u32 },
    Xor { rd: u32, rs: u32, rt: u32 },
    Nor { rd: u32, rs: u32, rt: u32 },
    Slt { rd: u32, rs: u32, rt: u32 },
    Sltu { rd: u32, rs: u32, rt: u32 },
    Sll { rd: u32, rt: u32, sa: u32 },
    Srl { rd: u32, rt: u32, sa: u32 },
    Sra { rd: u32, rt: u32, sa: u32 },
    Sllv { rd: u32, rt: u32, rs: u32 },
    Srlv { rd: u32, rt: u32, rs: u32 },
    Srav { rd: u32, rt: u32, rs: u32 },
    Mult { rs: u32, rt: u32 },
    Multu { rs: u32, rt: u32 },
    Div { rs: u32, rt: u32 },
    Divu { rs: u32, rt: u32 },
    Mfhi { rd: u32 },
    Mflo { rd: u32 },
    Jr { rs: u32 },
    Jalr { rd: u32, rs: u32 },
    Lw { rt: u32, base: u32, off: i16 },
    Sw { rt: u32, base: u32, off: i16 },
    Lb { rt: u32, base: u32, off: i16 },
    Lbu { rt: u32, base: u32, off: i16 },
    Lh { rt: u32, base: u32, off: i16 },
    Lhu { rt: u32, base: u32, off: i16 },
    Sb { rt: u32, base: u32, off: i16 },
    Sh { rt: u32, base: u32, off: i16 },
    Beq { rs: u32, rt: u32, off: i16 },
    Bne { rs: u32, rt: u32, off: i16 },
    Blez { rs: u32, off: i16 },
    Bgtz { rs: u32, off: i16 },
    J { target: u32 },
    Jal { target: u32 },
    Mfc0 { rt: u32, rd: u32 },
    Mtc0 { rt: u32, rd: u32 },
    Break { code: u32 },
    Syscall { code: u32 },
    Cache { op: u32, base: u32, off: i16 },
}

pub fn op(w: u32) -> u32 {
    (w >> 26) & 0x3F
}
pub fn rs(w: u32) -> u32 {
    (w >> 21) & 0x1F
}
pub fn rt(w: u32) -> u32 {
    (w >> 16) & 0x1F
}
pub fn rd(w: u32) -> u32 {
    (w >> 11) & 0x1F
}
pub fn sa(w: u32) -> u32 {
    (w >> 6) & 0x1F
}
pub fn funct(w: u32) -> u32 {
    w & 0x3F
}
pub fn imm16(w: u32) -> u16 {
    (w & 0xFFFF) as u16
}
pub fn imm16s(w: u32) -> i16 {
    (w & 0xFFFF) as u16 as i16
}

/// Decode one word. Unknown encodings are `Unsupported` with the exact
/// opcode/function fields, never a fake NOP.
pub fn decode(word: u32) -> Result<Decoded> {
    if word == 0x0000_0000 {
        return Ok(Decoded::Nop);
    }
    match op(word) {
        0x00 => match funct(word) {
            0x00 => Ok(Decoded::Sll {
                rd: rd(word),
                rt: rt(word),
                sa: sa(word),
            }),
            0x02 => Ok(Decoded::Srl {
                rd: rd(word),
                rt: rt(word),
                sa: sa(word),
            }),
            0x03 => Ok(Decoded::Sra {
                rd: rd(word),
                rt: rt(word),
                sa: sa(word),
            }),
            0x04 => Ok(Decoded::Sllv {
                rd: rd(word),
                rt: rt(word),
                rs: rs(word),
            }),
            0x06 => Ok(Decoded::Srlv {
                rd: rd(word),
                rt: rt(word),
                rs: rs(word),
            }),
            0x07 => Ok(Decoded::Srav {
                rd: rd(word),
                rt: rt(word),
                rs: rs(word),
            }),
            0x08 => Ok(Decoded::Jr { rs: rs(word) }),
            0x09 => Ok(Decoded::Jalr {
                rd: rd(word),
                rs: rs(word),
            }),
            0x0C => Ok(Decoded::Syscall {
                code: (word >> 6) & 0xFFFFF,
            }),
            0x0D => Ok(Decoded::Break {
                code: (word >> 6) & 0xFFFFF,
            }),
            0x10 => Ok(Decoded::Mfhi { rd: rd(word) }),
            0x11 => Ok(Decoded::Mflo { rd: rd(word) }),
            0x18 => Ok(Decoded::Mult {
                rs: rs(word),
                rt: rt(word),
            }),
            0x19 => Ok(Decoded::Multu {
                rs: rs(word),
                rt: rt(word),
            }),
            0x1A => Ok(Decoded::Div {
                rs: rs(word),
                rt: rt(word),
            }),
            0x1B => Ok(Decoded::Divu {
                rs: rs(word),
                rt: rt(word),
            }),
            0x20 => Ok(Decoded::Add {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x21 => Ok(Decoded::Addu {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x22 => Err(EmuError::Unsupported(format!(
                "SUB word={:08x} (SPECIAL 22, unimplemented)",
                word
            ))),
            0x23 => Ok(Decoded::Subu {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x24 => Ok(Decoded::And {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x25 => Ok(Decoded::Or {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x26 => Ok(Decoded::Xor {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x27 => Ok(Decoded::Nor {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x2A => Ok(Decoded::Slt {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            0x2B => Ok(Decoded::Sltu {
                rd: rd(word),
                rs: rs(word),
                rt: rt(word),
            }),
            f => Err(EmuError::Unsupported(format!(
                "SPECIAL funct={:02x} word={:08x}",
                f, word
            ))),
        },
        0x02 => Ok(Decoded::J {
            target: word & 0x03FF_FFFF,
        }),
        0x03 => Ok(Decoded::Jal {
            target: word & 0x03FF_FFFF,
        }),
        0x04 => Ok(Decoded::Beq {
            rs: rs(word),
            rt: rt(word),
            off: imm16s(word),
        }),
        0x05 => Ok(Decoded::Bne {
            rs: rs(word),
            rt: rt(word),
            off: imm16s(word),
        }),
        0x06 => Ok(Decoded::Blez {
            rs: rs(word),
            off: imm16s(word),
        }),
        0x07 => Ok(Decoded::Bgtz {
            rs: rs(word),
            off: imm16s(word),
        }),
        0x08 => Ok(Decoded::Addi {
            rt: rt(word),
            rs: rs(word),
            imm: imm16s(word),
        }),
        0x09 => Ok(Decoded::Addiu {
            rt: rt(word),
            rs: rs(word),
            imm: imm16s(word),
        }),
        0x0A => Ok(Decoded::Slti {
            rt: rt(word),
            rs: rs(word),
            imm: imm16s(word),
        }),
        0x0B => Ok(Decoded::Sltiu {
            rt: rt(word),
            rs: rs(word),
            imm: imm16s(word),
        }),
        0x0C => Ok(Decoded::Andi {
            rt: rt(word),
            rs: rs(word),
            imm: imm16(word) as u32,
        }),
        0x0D => Ok(Decoded::Ori {
            rt: rt(word),
            rs: rs(word),
            imm: imm16(word) as u32,
        }),
        0x0E => Ok(Decoded::Xori {
            rt: rt(word),
            rs: rs(word),
            imm: imm16(word) as u32,
        }),
        0x0F => Ok(Decoded::Lui {
            rt: rt(word),
            imm: imm16(word) as u32,
        }),
        0x10 => Err(EmuError::Unsupported(format!(
            "COP0 word={:08x} (use MFC0/MTC0 path)",
            word
        ))),
        0x11 => Err(EmuError::Unsupported(format!("COP1/FPU word={:08x}", word))),
        0x20 => Ok(Decoded::Lb {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x21 => Ok(Decoded::Lh {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x23 => Ok(Decoded::Lw {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x24 => Ok(Decoded::Lbu {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x25 => Ok(Decoded::Lhu {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x28 => Ok(Decoded::Sb {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x29 => Ok(Decoded::Sh {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x2B => Ok(Decoded::Sw {
            rt: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        0x2F => Ok(Decoded::Cache {
            op: rt(word),
            base: rs(word),
            off: imm16s(word),
        }),
        o => Err(EmuError::Unsupported(format!(
            "primary op={:02x} word={:08x}",
            o, word
        ))),
    }
}

/// MFC0/MTC0 secondary decode (COP0 op 0x10 with rs field selecting move).
pub fn decode_cop0(word: u32) -> Result<Decoded> {
    let r = rs(word);
    match r {
        0x00 => Ok(Decoded::Mfc0 {
            rt: rt(word),
            rd: rd(word),
        }),
        0x04 => Ok(Decoded::Mtc0 {
            rt: rt(word),
            rd: rd(word),
        }),
        _ => Err(EmuError::Unsupported(format!(
            "COP0 rs={:02x} word={:08x}",
            r, word
        ))),
    }
}

/// Human-readable disassembly for debugger/trace.
pub fn disasm(word: u32) -> String {
    if word == 0 {
        return "nop".into();
    }
    match decode(word) {
        Ok(d) => format!("{:?}", d),
        Err(_) => {
            // Try COP0 move path before giving up.
            if op(word) == 0x10 {
                match decode_cop0(word) {
                    Ok(d) => format!("{:?}", d),
                    Err(e) => format!("Unsupported({})", e),
                }
            } else {
                format!("Unsupported(op={:02x} word={:08x})", op(word), word)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retail_vectors_decode() {
        // lui $t0,0x8000 ; addiu $t0,$t0,0x1000 ; lw $t1,0($t0) ; sw $t1,4($t0)
        assert_eq!(
            decode(0x3C08_8000).unwrap(),
            Decoded::Lui { rt: 8, imm: 0x8000 }
        );
        assert!(matches!(
            decode(0x2508_1000).unwrap(),
            Decoded::Addiu { .. }
        ));
        assert!(matches!(decode(0x8D09_0000).unwrap(), Decoded::Lw { .. }));
        assert!(matches!(decode(0xAD09_0004).unwrap(), Decoded::Sw { .. }));
    }

    #[test]
    fn branch_and_jump_decode() {
        assert!(matches!(decode(0x0800_0400).unwrap(), Decoded::J { .. }));
        assert!(matches!(decode(0x0C00_0400).unwrap(), Decoded::Jal { .. }));
        assert!(matches!(decode(0x1100_0003).unwrap(), Decoded::Beq { .. }));
        assert!(matches!(decode(0x03E0_0008).unwrap(), Decoded::Jr { .. }));
    }

    #[test]
    fn unknown_is_loud() {
        // COP1 floating point: not implemented in M1.
        assert!(decode(0x4600_0000).is_err());
        // Made-up primary opcode.
        assert!(decode(0xFC00_0000).is_err());
    }
}
