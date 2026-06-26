use crate::tinyasm::registers::{ControlRegister, Register, XmmRegister};
use std::fmt;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    UnsupportedOperand(String),
    InvalidScale(u8),
    #[allow(dead_code)]
    InvalidDisplacement(String),
    Other(String),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::UnsupportedOperand(msg) => write!(f, "Unsupported operand: {}", msg),
            EncodeError::InvalidScale(scale) => write!(f, "Invalid scale: {}", scale),
            EncodeError::InvalidDisplacement(msg) => write!(f, "Invalid displacement: {}", msg),
            EncodeError::Other(msg) => write!(f, "Encoding error: {}", msg),
        }
    }
}

impl std::error::Error for EncodeError {}

// ---------------------------------------------------------------------------
// Memory addressing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryAddr {
    pub base: Option<Register>,
    pub index: Option<Register>,
    /// Must be 1, 2, 4, or 8.  Ignored when `index` is `None`.
    pub scale: u8,
    pub disp: i32,
}

impl MemoryAddr {
    /// Convenience constructor for simple `[reg]` or `[reg + disp]` addressing.
    pub fn base_disp(base: Register, disp: i32) -> Self {
        Self {
            base: Some(base),
            index: None,
            scale: 1,
            disp,
        }
    }
}

impl fmt::Display for MemoryAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        let mut parts: Vec<String> = Vec::new();
        if let Some(base) = self.base {
            parts.push(base.to_string());
        }
        if let Some(index) = self.index {
            parts.push(format!("{}*{}", index, self.scale));
        }
        if self.disp != 0 || parts.is_empty() {
            if self.disp > 0 && !parts.is_empty() {
                parts.push(format!("+{}", self.disp));
            } else {
                parts.push(self.disp.to_string());
            }
        }
        write!(f, "{}]", parts.join(" "))
    }
}

// ---------------------------------------------------------------------------
// Operand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum Operand {
    Reg(Register),
    Cr(ControlRegister),
    Xmm(XmmRegister),
    Imm64(u64),
    Imm32(i32),
    Mem(MemoryAddr),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Reg(r) => write!(f, "{}", r),
            Operand::Cr(c) => write!(f, "{}", c),
            Operand::Xmm(x) => write!(f, "{}", x),
            Operand::Imm64(v) => write!(f, "0x{:X}", v),
            Operand::Imm32(v) if *v < 0 => write!(f, "{}", v),
            Operand::Imm32(v) => write!(f, "0x{:X}", v),
            Operand::Mem(m) => write!(f, "qword {}", m),
        }
    }
}

// ---------------------------------------------------------------------------
// Instruction set
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Instruction {
    // Data movement
    Mov(Operand, Operand),
    /// Load effective address: `lea dst, [mem]`
    /// dst must be a register; src must be a memory operand.
    Lea(Operand, Operand),
    Push(Operand),
    Pop(Operand),

    // Control register MOV
    MovCr(Operand, Operand),

    // Arithmetic
    Add(Operand, Operand),
    Sub(Operand, Operand),
    IMul(Operand, Operand), // signed multiply: dst *= src
    Mul(Operand),           // unsigned RDX:RAX = RAX * op
    Div(Operand),           // unsigned RAX / op

    // Bitwise / shift
    And(Operand, Operand),
    Or(Operand, Operand),
    Xor(Operand, Operand),
    Not(Operand),
    Shl(Operand, Operand),
    Shr(Operand, Operand),

    // Compare / test
    Cmp(Operand, Operand),
    Test(Operand, Operand),

    // Control flow (direct)
    Call(Operand),
    Ret,
    Syscall,

    // SSE2 data movement
    Movdqa(Operand, Operand),
    Movdqu(Operand, Operand),
    /// SSE2 packed integer arithmetic
    Paddb(Operand, Operand),
    Paddw(Operand, Operand),
    Paddd(Operand, Operand),
    Paddq(Operand, Operand),
    Psubb(Operand, Operand),
    Psubw(Operand, Operand),
    Psubd(Operand, Operand),
    Psubq(Operand, Operand),
    /// SSE2 packed bitwise
    Pxor(Operand, Operand),
    Pand(Operand, Operand),
    Por(Operand, Operand),
    /// SSE2 packed compare
    Pcmpeqb(Operand, Operand),
    Pcmpeqw(Operand, Operand),
    Pcmpeqd(Operand, Operand),

    // Scalar SSE (double-precision float)
    Movsd(Operand, Operand),
    Addsd(Operand, Operand),
    Subsd(Operand, Operand),
    Mulsd(Operand, Operand),
    Divsd(Operand, Operand),
    Cvtsi2sd(Operand, Operand),
    Cvttsd2si(Operand, Operand),
    Ucomisd(Operand, Operand),
    /// Xorps — bitwise XOR (often used to zero XMM via xorps xmm, xmm).  No prefix.
    Xorps(Operand, Operand),

    // Labels and label-targeted jumps — resolved by Assembler, not Encoder.
    Label(String),
    JmpLabel(String),
    JeLabel(String),
    JneLabel(String),
    JlLabel(String),
    JleLabel(String),
    JgeLabel(String),
    JgLabel(String),
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Instruction::Mov(d, s) => write!(f, "mov {}, {}", d, s),
            Instruction::Lea(d, s) => write!(f, "lea {}, {}", d, s),
            Instruction::Push(o) => write!(f, "push {}", o),
            Instruction::Pop(o) => write!(f, "pop {}", o),
            Instruction::MovCr(d, s) => write!(f, "mov {}, {}", d, s),
            Instruction::Add(d, s) => write!(f, "add {}, {}", d, s),
            Instruction::Sub(d, s) => write!(f, "sub {}, {}", d, s),
            Instruction::IMul(d, s) => write!(f, "imul {}, {}", d, s),
            Instruction::Mul(o) => write!(f, "mul {}", o),
            Instruction::Div(o) => write!(f, "div {}", o),
            Instruction::And(d, s) => write!(f, "and {}, {}", d, s),
            Instruction::Or(d, s) => write!(f, "or {}, {}", d, s),
            Instruction::Xor(d, s) => write!(f, "xor {}, {}", d, s),
            Instruction::Not(o) => write!(f, "not {}", o),
            Instruction::Shl(d, c) => write!(f, "shl {}, {}", d, c),
            Instruction::Shr(d, c) => write!(f, "shr {}, {}", d, c),
            Instruction::Cmp(d, s) => write!(f, "cmp {}, {}", d, s),
            Instruction::Test(d, s) => write!(f, "test {}, {}", d, s),
            Instruction::Call(o) => write!(f, "call {}", o),
            Instruction::Ret => write!(f, "ret"),
            Instruction::Syscall => write!(f, "syscall"),
            Instruction::Movdqa(d, s) => write!(f, "movdqa {}, {}", d, s),
            Instruction::Movdqu(d, s) => write!(f, "movdqu {}, {}", d, s),
            Instruction::Paddb(d, s) => write!(f, "paddb {}, {}", d, s),
            Instruction::Paddw(d, s) => write!(f, "paddw {}, {}", d, s),
            Instruction::Paddd(d, s) => write!(f, "paddd {}, {}", d, s),
            Instruction::Paddq(d, s) => write!(f, "paddq {}, {}", d, s),
            Instruction::Psubb(d, s) => write!(f, "psubb {}, {}", d, s),
            Instruction::Psubw(d, s) => write!(f, "psubw {}, {}", d, s),
            Instruction::Psubd(d, s) => write!(f, "psubd {}, {}", d, s),
            Instruction::Psubq(d, s) => write!(f, "psubq {}, {}", d, s),
            Instruction::Pxor(d, s) => write!(f, "pxor {}, {}", d, s),
            Instruction::Pand(d, s) => write!(f, "pand {}, {}", d, s),
            Instruction::Por(d, s) => write!(f, "por {}, {}", d, s),
            Instruction::Pcmpeqb(d, s) => write!(f, "pcmpeqb {}, {}", d, s),
            Instruction::Pcmpeqw(d, s) => write!(f, "pcmpeqw {}, {}", d, s),
            Instruction::Pcmpeqd(d, s) => write!(f, "pcmpeqd {}, {}", d, s),
            Instruction::Movsd(d, s) => write!(f, "movsd {}, {}", d, s),
            Instruction::Addsd(d, s) => write!(f, "addsd {}, {}", d, s),
            Instruction::Subsd(d, s) => write!(f, "subsd {}, {}", d, s),
            Instruction::Mulsd(d, s) => write!(f, "mulsd {}, {}", d, s),
            Instruction::Divsd(d, s) => write!(f, "divsd {}, {}", d, s),
            Instruction::Cvtsi2sd(d, s) => write!(f, "cvtsi2sd {}, {}", d, s),
            Instruction::Cvttsd2si(d, s) => write!(f, "cvttsd2si {}, {}", d, s),
            Instruction::Ucomisd(d, s) => write!(f, "ucomisd {}, {}", d, s),
            Instruction::Xorps(d, s) => write!(f, "xorps {}, {}", d, s),
            Instruction::Label(n) => write!(f, "{}:", n),
            Instruction::JmpLabel(t) => write!(f, "jmp {}", t),
            Instruction::JeLabel(t) => write!(f, "je {}", t),
            Instruction::JneLabel(t) => write!(f, "jne {}", t),
            Instruction::JlLabel(t) => write!(f, "jl {}", t),
            Instruction::JleLabel(t) => write!(f, "jle {}", t),
            Instruction::JgeLabel(t) => write!(f, "jge {}", t),
            Instruction::JgLabel(t) => write!(f, "jg {}", t),
        }
    }
}

// ---------------------------------------------------------------------------
// REX prefix helpers
// ---------------------------------------------------------------------------

/// Builds a REX.W prefix byte.
///
/// Bit layout: `0100 WRXB`
/// - W = 1 → 64-bit operand size (always set here)
/// - R = 1 → ModR/M `reg` field is R8–R15
/// - X = 1 → SIB `index` field is R8–R15
/// - B = 1 → ModR/M `rm` / opcode-embedded register is R8–R15
#[inline]
fn rex_w(r: bool, x: bool, b: bool) -> u8 {
    0x48 | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

/// Builds a REX prefix byte without the W bit (used by SSE instructions).
///
/// Bit layout: `0100 0RXB`
#[inline]
fn rex(r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

// ---------------------------------------------------------------------------
// ModR/M + SIB encoding for memory operands
// ---------------------------------------------------------------------------

fn push_displacement(disp: i32, size: usize, bytes: &mut Vec<u8>) {
    match size {
        1 => bytes.push(disp as u8),
        4 => bytes.extend_from_slice(&disp.to_le_bytes()),
        _ => {}
    }
}

/// Returns `(modrm, sib, disp_size, rex_b, rex_x)` for a memory operand.
///
/// `reg_field` is the 3-bit value that goes into ModR/M's `reg` slot
/// (either a register code or an opcode extension).
fn encode_mem_parts(
    reg_field: u8,
    mem: MemoryAddr,
) -> Result<(u8, Option<u8>, usize, bool, bool), EncodeError> {
    // Choose mod bits and displacement size.
    let (mod_bits, disp_size) = if let Some(base) = mem.base {
        // RBP/R13 with mod=00 would be interpreted as RIP-relative, so force
        // at least an 8-bit displacement.
        let bp_family = base == Register::RBP || base == Register::R13;
        if mem.disp == 0 && !bp_family {
            (0x00, 0)
        } else if (-128..=127).contains(&mem.disp) {
            (0x01, 1)
        } else {
            (0x02, 4)
        }
    } else {
        // No base → disp32 only.
        (0x00, 4)
    };

    // RSP/R12 share the 3-bit code 0b100 with the "SIB follows" sentinel, so
    // any addressing using them as the base *must* include a SIB byte.
    let use_sib =
        mem.index.is_some() || mem.base == Some(Register::RSP) || mem.base == Some(Register::R12);

    let rm_bits = if use_sib {
        0x04 // "SIB byte present"
    } else {
        mem.base
            .ok_or_else(|| {
                EncodeError::Other("Memory operand with no base register and no SIB".into())
            })?
            .code()
    };

    let modrm = (mod_bits << 6) | (reg_field << 3) | rm_bits;
    let rex_b = mem.base.is_some_and(|r| r.is_extended());
    let rex_x = mem.index.is_some_and(|r| r.is_extended());

    let sib = if use_sib {
        let scale_bits = match mem.scale {
            1 => 0u8,
            2 => 1,
            4 => 2,
            8 => 3,
            s => return Err(EncodeError::InvalidScale(s)),
        };
        // No index → encode index field as 0b100 (no-index sentinel).
        let index_bits = mem.index.map(|r| r.code()).unwrap_or(0x04);
        let base_bits = mem.base.map(|r| r.code()).unwrap_or(0x05);
        Some((scale_bits << 6) | (index_bits << 3) | base_bits)
    } else {
        None
    };

    Ok((modrm, sib, disp_size, rex_b, rex_x))
}

// ---------------------------------------------------------------------------
// Public encode entry point
// ---------------------------------------------------------------------------

/// Encode a single instruction to machine bytes.
///
/// Label-related variants (`Label`, `Jxx Label`) **must** be resolved by the
/// [`Assembler`] before calling this function; they return an error here.
pub fn encode_instruction(instr: Instruction) -> Result<Vec<u8>, EncodeError> {
    let mut bytes = Vec::new();
    match instr {
        // Data movement
        Instruction::Mov(dst, src) => encode_mov(dst, src, &mut bytes)?,
        Instruction::Lea(dst, src) => encode_lea(dst, src, &mut bytes)?,
        Instruction::Push(op) => encode_push(op, &mut bytes)?,
        Instruction::Pop(op) => encode_pop(op, &mut bytes)?,
        Instruction::MovCr(dst, src) => encode_mov_cr(dst, src, &mut bytes)?,

        // Arithmetic
        Instruction::Add(d, s) => encode_arithmetic(0x01, 0x03, 0, d, s, &mut bytes)?,
        Instruction::Sub(d, s) => encode_arithmetic(0x29, 0x2B, 5, d, s, &mut bytes)?,
        Instruction::IMul(d, s) => encode_imul(d, s, &mut bytes)?,
        Instruction::Mul(op) => encode_unary(0xF7, 4, op, &mut bytes)?,
        Instruction::Div(op) => encode_unary(0xF7, 6, op, &mut bytes)?,

        // Bitwise / shift
        Instruction::And(d, s) => encode_arithmetic(0x21, 0x23, 4, d, s, &mut bytes)?,
        Instruction::Or(d, s) => encode_arithmetic(0x09, 0x0B, 1, d, s, &mut bytes)?,
        Instruction::Xor(d, s) => encode_arithmetic(0x31, 0x33, 6, d, s, &mut bytes)?,
        Instruction::Not(op) => encode_unary(0xF7, 2, op, &mut bytes)?,
        Instruction::Shl(d, c) => encode_shift(4, d, c, &mut bytes)?,
        Instruction::Shr(d, c) => encode_shift(5, d, c, &mut bytes)?,

        // Compare / test
        Instruction::Cmp(d, s) => encode_arithmetic(0x39, 0x3B, 7, d, s, &mut bytes)?,
        Instruction::Test(d, s) => encode_test(d, s, &mut bytes)?,

        // SSE2 data movement
        Instruction::Movdqa(d, s) => encode_sse_move(0x66, d, s, &mut bytes)?,
        Instruction::Movdqu(d, s) => encode_sse_move(0xF3, d, s, &mut bytes)?,
        Instruction::Paddb(d, s) => encode_sse_alu(0xFC, d, s, &mut bytes)?,
        Instruction::Paddw(d, s) => encode_sse_alu(0xFD, d, s, &mut bytes)?,
        Instruction::Paddd(d, s) => encode_sse_alu(0xFE, d, s, &mut bytes)?,
        Instruction::Paddq(d, s) => encode_sse_alu(0xD4, d, s, &mut bytes)?,
        Instruction::Psubb(d, s) => encode_sse_alu(0xF8, d, s, &mut bytes)?,
        Instruction::Psubw(d, s) => encode_sse_alu(0xF9, d, s, &mut bytes)?,
        Instruction::Psubd(d, s) => encode_sse_alu(0xFA, d, s, &mut bytes)?,
        Instruction::Psubq(d, s) => encode_sse_alu(0xFB, d, s, &mut bytes)?,
        Instruction::Pxor(d, s) => encode_sse_alu(0xEF, d, s, &mut bytes)?,
        Instruction::Pand(d, s) => encode_sse_alu(0xDB, d, s, &mut bytes)?,
        Instruction::Por(d, s) => encode_sse_alu(0xEB, d, s, &mut bytes)?,
        Instruction::Pcmpeqb(d, s) => encode_sse_alu(0x74, d, s, &mut bytes)?,
        Instruction::Pcmpeqw(d, s) => encode_sse_alu(0x75, d, s, &mut bytes)?,
        Instruction::Pcmpeqd(d, s) => encode_sse_alu(0x76, d, s, &mut bytes)?,

        // Scalar SSE (double-precision float)
        Instruction::Movsd(d, s) => encode_sse_scalar_move(d, s, &mut bytes)?,
        Instruction::Addsd(d, s) => encode_sse_scalar_alu(0xF2, 0x58, d, s, &mut bytes)?,
        Instruction::Subsd(d, s) => encode_sse_scalar_alu(0xF2, 0x5C, d, s, &mut bytes)?,
        Instruction::Mulsd(d, s) => encode_sse_scalar_alu(0xF2, 0x59, d, s, &mut bytes)?,
        Instruction::Divsd(d, s) => encode_sse_scalar_alu(0xF2, 0x5E, d, s, &mut bytes)?,
        Instruction::Cvtsi2sd(d, s) => encode_cvtsi2sd(d, s, &mut bytes)?,
        Instruction::Cvttsd2si(d, s) => encode_cvttsd2si(d, s, &mut bytes)?,
        Instruction::Ucomisd(d, s) => encode_sse_scalar_alu(0x66, 0x2E, d, s, &mut bytes)?,
        Instruction::Xorps(d, s) => encode_sse_scalar_alu(0x00, 0x57, d, s, &mut bytes)?,

        // Control flow
        Instruction::Call(op) => encode_call(op, &mut bytes)?,
        Instruction::Ret => bytes.push(0xC3),
        Instruction::Syscall => bytes.extend_from_slice(&[0x0F, 0x05]),

        // Labels / jumps — must be handled by Assembler.
        Instruction::Label(_)
        | Instruction::JmpLabel(_)
        | Instruction::JeLabel(_)
        | Instruction::JneLabel(_)
        | Instruction::JlLabel(_)
        | Instruction::JleLabel(_)
        | Instruction::JgeLabel(_)
        | Instruction::JgLabel(_) => {
            return Err(EncodeError::Other(
                "Label/jump instructions must be handled by Assembler, not Encoder".into(),
            ));
        }
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// MOV
// ---------------------------------------------------------------------------

fn encode_mov(dst: Operand, src: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match (dst, src) {
        // MOV r64, imm64  →  REX.W B8+rd  id
        (Operand::Reg(r), Operand::Imm64(imm)) => {
            bytes.push(rex_w(false, false, r.is_extended()));
            bytes.push(0xB8 + r.code());
            bytes.extend_from_slice(&imm.to_le_bytes());
        }
        // MOV r64, imm32 (sign-extended)  →  REX.W C7 /0 id
        (Operand::Reg(r), Operand::Imm32(imm)) => {
            bytes.push(rex_w(false, false, r.is_extended()));
            bytes.push(0xC7);
            bytes.push(0xC0 | r.code());
            bytes.extend_from_slice(&imm.to_le_bytes());
        }
        // MOV r64, r64  →  REX.W 89 /r   (opcode 89: MOV r/m64, r64)
        // ModR/M: reg = src (the "reg" field), rm = dst
        (Operand::Reg(dst_r), Operand::Reg(src_r)) => {
            bytes.push(rex_w(src_r.is_extended(), false, dst_r.is_extended()));
            bytes.push(0x89);
            bytes.push(0xC0 | (src_r.code() << 3) | dst_r.code());
        }
        // MOV r64, [mem]  →  REX.W 8B /r
        (Operand::Reg(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(dst_r.is_extended(), rex_x, rex_b));
            bytes.push(0x8B);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        // MOV [mem], r64  →  REX.W 89 /r
        (Operand::Mem(mem), Operand::Reg(src_r)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(src_r.code(), mem)?;
            bytes.push(rex_w(src_r.is_extended(), rex_x, rex_b));
            bytes.push(0x89);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "MOV: unsupported operand combination".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// LEA
// ---------------------------------------------------------------------------

/// Encode `LEA r64, [mem]`  =>  REX.W 8D /r
///
/// LEA computes an effective address without performing a memory access, so
/// the source *must* be a `Mem` operand and the destination must be a `Reg`.
fn encode_lea(dst: Operand, src: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Reg(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(dst_r.is_extended(), rex_x, rex_b));
            bytes.push(0x8D);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        (Operand::Reg(_), _) => {
            return Err(EncodeError::UnsupportedOperand(
                "LEA: source must be a memory operand".into(),
            ));
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "LEA: destination must be a register".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// PUSH / POP
// ---------------------------------------------------------------------------

fn encode_push(op: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match op {
        // PUSH r64  →  (REX.B?) 50+rd
        Operand::Reg(r) => {
            if r.is_extended() {
                bytes.push(0x41);
            } // REX.B
            bytes.push(0x50 + r.code());
        }
        // PUSH imm32 (sign-extended to 64 bits)  →  68 id
        Operand::Imm32(imm) => {
            if (-128..=127).contains(&imm) {
                bytes.push(0x6A);
                bytes.push(imm as u8);
            } else {
                bytes.push(0x68);
                bytes.extend_from_slice(&imm.to_le_bytes());
            }
        }
        // PUSH [mem]  →  REX.W FF /6
        Operand::Mem(mem) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(6, mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.push(0xFF);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "PUSH: unsupported operand".into(),
            ));
        }
    }
    Ok(())
}

fn encode_pop(op: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match op {
        // POP r64  →  (REX.B?) 58+rd
        Operand::Reg(r) => {
            if r.is_extended() {
                bytes.push(0x41);
            } // REX.B
            bytes.push(0x58 + r.code());
        }
        // POP [mem]  →  REX.W 8F /0
        Operand::Mem(mem) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(0, mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.push(0x8F);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "POP: unsupported operand".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// IMUL (two-operand: dst r64 *= src r/m64)
// ---------------------------------------------------------------------------

fn encode_imul(dst: Operand, src: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match (dst, src) {
        // IMUL r64, r/m64  →  REX.W 0F AF /r
        (Operand::Reg(dst_r), Operand::Reg(src_r)) => {
            bytes.push(rex_w(dst_r.is_extended(), false, src_r.is_extended()));
            bytes.extend_from_slice(&[0x0F, 0xAF]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        (Operand::Reg(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(dst_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[0x0F, 0xAF]);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        // IMUL r64, r/m64, imm8  →  REX.W 6B /r ib
        (Operand::Reg(dst_r), Operand::Imm32(imm)) if (-128..=127).contains(&imm) => {
            bytes.push(rex_w(dst_r.is_extended(), false, false));
            bytes.push(0x6B);
            // Self-multiply: dst = dst * imm  (src same as dst in ModR/M)
            bytes.push(0xC0 | (dst_r.code() << 3) | dst_r.code());
            bytes.push(imm as u8);
        }
        // IMUL r64, r/m64, imm32  →  REX.W 69 /r id
        (Operand::Reg(dst_r), Operand::Imm32(imm)) => {
            bytes.push(rex_w(dst_r.is_extended(), false, false));
            bytes.push(0x69);
            bytes.push(0xC0 | (dst_r.code() << 3) | dst_r.code());
            bytes.extend_from_slice(&imm.to_le_bytes());
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "IMUL: destination must be a register".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CALL
// ---------------------------------------------------------------------------

fn encode_call(op: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match op {
        // CALL r64  →  (REX.B?) FF /2
        Operand::Reg(r) => {
            if r.is_extended() {
                bytes.push(0x41);
            }
            bytes.push(0xFF);
            bytes.push(0xD0 | r.code()); // ModR/M: mod=11, reg=2, rm=r
        }
        // CALL [mem]  →  REX.W FF /2
        Operand::Mem(mem) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(2, mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.push(0xFF);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "CALL: operand must be a register or memory".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic arithmetic (ADD / SUB / AND / OR / XOR / CMP)
// ---------------------------------------------------------------------------

/// Encode a two-operand arithmetic instruction.
///
/// - `op_mr`   — opcode for `reg/mem, reg`  (e.g. `0x01` for ADD)
/// - `op_rm`   — opcode for `reg, reg/mem`  (e.g. `0x03` for ADD)
/// - `ext_idx` — `/digit` extension for the immediate form (e.g. `0` for ADD)
fn encode_arithmetic(
    op_mr: u8,
    op_rm: u8,
    ext_idx: u8,
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        // op r64, r64
        (Operand::Reg(dst_r), Operand::Reg(src_r)) => {
            bytes.push(rex_w(src_r.is_extended(), false, dst_r.is_extended()));
            bytes.push(op_mr);
            bytes.push(0xC0 | (src_r.code() << 3) | dst_r.code());
        }
        // op r64, [mem]
        (Operand::Reg(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(dst_r.is_extended(), rex_x, rex_b));
            bytes.push(op_rm);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        // op [mem], r64
        (Operand::Mem(mem), Operand::Reg(src_r)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(src_r.code(), mem)?;
            bytes.push(rex_w(src_r.is_extended(), rex_x, rex_b));
            bytes.push(op_mr);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        // op r/m64, imm8 (sign-extended) or imm32
        (dst, Operand::Imm32(imm)) => {
            let (opcode, is_imm8) = if (-128..=127).contains(&imm) {
                (0x83u8, true)
            } else {
                (0x81u8, false)
            };
            match dst {
                Operand::Reg(r) => {
                    bytes.push(rex_w(false, false, r.is_extended()));
                    bytes.push(opcode);
                    bytes.push(0xC0 | (ext_idx << 3) | r.code());
                }
                Operand::Mem(mem) => {
                    let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(ext_idx, mem)?;
                    bytes.push(rex_w(false, rex_x, rex_b));
                    bytes.push(opcode);
                    bytes.push(modrm);
                    if let Some(s) = sib {
                        bytes.push(s);
                    }
                    push_displacement(mem.disp, disp_sz, bytes);
                }
                _ => {
                    return Err(EncodeError::UnsupportedOperand(
                        "Arithmetic Imm: destination must be register or memory".into(),
                    ));
                }
            }
            if is_imm8 {
                bytes.push(imm as u8);
            } else {
                bytes.extend_from_slice(&imm.to_le_bytes());
            }
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Arithmetic: unsupported operands".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TEST
// ---------------------------------------------------------------------------

fn encode_test(dst: Operand, src: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match (dst, src) {
        // TEST r/m64, r64  →  REX.W 85 /r
        (Operand::Reg(dst_r), Operand::Reg(src_r)) => {
            bytes.push(rex_w(src_r.is_extended(), false, dst_r.is_extended()));
            bytes.push(0x85);
            bytes.push(0xC0 | (src_r.code() << 3) | dst_r.code());
        }
        // TEST r/m64, imm32  →  REX.W F7 /0 id
        (Operand::Reg(r), Operand::Imm32(imm)) => {
            bytes.push(rex_w(false, false, r.is_extended()));
            bytes.push(0xF7);
            bytes.push(0xC0 | r.code()); // /0
            bytes.extend_from_slice(&imm.to_le_bytes());
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "TEST: unsupported operands".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shifts
// ---------------------------------------------------------------------------

fn encode_shift(
    ext_idx: u8,
    dst: Operand,
    count: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    let (opcode, emit_imm) = match count {
        Operand::Reg(Register::RCX) => (0xD3u8, false), // shift by CL
        Operand::Imm32(1) => (0xD1, false),             // shift by 1 (implicit)
        Operand::Imm32(_) => (0xC1, true),              // shift by imm8
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Shift count must be CL register or an immediate".into(),
            ));
        }
    };

    match dst {
        Operand::Reg(r) => {
            bytes.push(rex_w(false, false, r.is_extended()));
            bytes.push(opcode);
            bytes.push(0xC0 | (ext_idx << 3) | r.code());
        }
        Operand::Mem(mem) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(ext_idx, mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.push(opcode);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Shift dst must be register or memory".into(),
            ));
        }
    }

    if emit_imm {
        if let Operand::Imm32(imm) = count {
            bytes.push(imm as u8);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unary (NOT / MUL / DIV)
// ---------------------------------------------------------------------------

fn encode_unary(
    opcode: u8,
    ext_idx: u8,
    op: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match op {
        Operand::Reg(r) => {
            bytes.push(rex_w(false, false, r.is_extended()));
            bytes.push(opcode);
            bytes.push(0xC0 | (ext_idx << 3) | r.code());
        }
        Operand::Mem(mem) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(ext_idx, mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.push(opcode);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Unary: operand must be register or memory".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// MOV to/from control registers
// ---------------------------------------------------------------------------
//
// Encoding:
//   MOV r64, crN  →  REX.W 0F 20 /r   (reg field = CR, rm = GPR)
//   MOV crN, r64  →  REX.W 0F 22 /r   (reg field = CR, rm = GPR)

fn encode_mov_cr(dst: Operand, src: Operand, bytes: &mut Vec<u8>) -> Result<(), EncodeError> {
    match (dst, src) {
        // MOV r64, crN  (read control register into GPR)
        (Operand::Reg(gpr), Operand::Cr(cr)) => {
            bytes.push(rex_w(cr.is_extended(), false, gpr.is_extended()));
            bytes.extend_from_slice(&[0x0F, 0x20]);
            bytes.push(0xC0 | ((cr.code() & 7) << 3) | gpr.code());
        }
        // MOV crN, r64  (write GPR to control register)
        (Operand::Cr(cr), Operand::Reg(gpr)) => {
            bytes.push(rex_w(cr.is_extended(), false, gpr.is_extended()));
            bytes.extend_from_slice(&[0x0F, 0x22]);
            bytes.push(0xC0 | ((cr.code() & 7) << 3) | gpr.code());
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "MOV CR: operands must be (GPR, CR) or (CR, GPR)".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSE2 ALU (two XMM operands: dst ^= src)
// ---------------------------------------------------------------------------
//
// All follow the pattern: 66 0F <opcode> /r
// ModR/M: reg = dst, rm = src (or memory).

fn encode_sse_alu(
    opcode: u8,
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Xmm(dst_r), Operand::Xmm(src_r)) => {
            let rex_r = dst_r.is_extended();
            let rex_b = src_r.is_extended();
            bytes.push(rex(rex_r, false, rex_b));
            bytes.extend_from_slice(&[0x66, 0x0F, opcode]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        (Operand::Xmm(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex(dst_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[0x66, 0x0F, opcode]);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "SSE ALU: destination must be XMM, source XMM or memory".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSE2 data move (MOVDQA / MOVDQU)
// ---------------------------------------------------------------------------

fn encode_sse_move(
    prefix: u8,
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        // xmm ← xmm
        (Operand::Xmm(dst_r), Operand::Xmm(src_r)) => {
            let rex_r = dst_r.is_extended();
            let rex_b = src_r.is_extended();
            bytes.push(rex(rex_r, false, rex_b));
            bytes.extend_from_slice(&[prefix, 0x0F, 0x6F]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        // xmm ← [mem]
        (Operand::Xmm(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex(dst_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[prefix, 0x0F, 0x6F]);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        // [mem] ← xmm
        (Operand::Mem(mem), Operand::Xmm(src_r)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(src_r.code(), mem)?;
            bytes.push(rex(src_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[prefix, 0x0F, 0x7F]);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "SSE move: invalid operand combination".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Scalar SSE ALU (Addsd, Subsd, Mulsd, Divsd, Ucomisd, Xorps)
// ---------------------------------------------------------------------------
//
// Pattern: [prefix] 0F <opcode> /r
//   F2 prefix → Addsd / Subsd / Mulsd / Divsd
//   66 prefix → Ucomisd
//   no prefix → Xorps

fn encode_sse_scalar_alu(
    prefix: u8,
    opcode: u8,
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Xmm(dst_r), Operand::Xmm(src_r)) => {
            let rex_r = dst_r.is_extended();
            let rex_b = src_r.is_extended();
            bytes.push(rex(rex_r, false, rex_b));
            if prefix != 0 {
                bytes.push(prefix);
            }
            bytes.extend_from_slice(&[0x0F, opcode]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        (Operand::Xmm(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex(dst_r.is_extended(), rex_x, rex_b));
            if prefix != 0 {
                bytes.push(prefix);
            }
            bytes.extend_from_slice(&[0x0F, opcode]);
            bytes.push(modrm);
            if let Some(s) = sib {
                bytes.push(s);
            }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "SSE scalar ALU: destination must be XMM, source XMM or memory".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Movsd (Move Scalar Double)
// ---------------------------------------------------------------------------
//
// Encoding:
//   movsd xmm1, xmm2/m64  →  F2 0F 10 /r   (load)
//   movsd xmm2/m64, xmm1  →  F2 0F 11 /r   (store)

fn encode_sse_scalar_move(
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Xmm(dst_r), Operand::Xmm(src_r)) => {
            let rex_r = dst_r.is_extended();
            let rex_b = src_r.is_extended();
            bytes.push(rex(rex_r, false, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x10]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        (Operand::Xmm(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex(dst_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x10]);
            bytes.push(modrm);
            if let Some(s) = sib { bytes.push(s); }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        (Operand::Mem(mem), Operand::Xmm(src_r)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(src_r.code(), mem)?;
            bytes.push(rex(src_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x11]);
            bytes.push(modrm);
            if let Some(s) = sib { bytes.push(s); }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Movsd: invalid operand combination".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cvtsi2sd (convert int32/int64 to double)
// ---------------------------------------------------------------------------
//
// Encoding:
//   cvtsi2sd xmm1, r/m32   →  F2 0F 2A /r
//   cvtsi2sd xmm1, r/m64   →  F2 REX.W 0F 2A /r

fn encode_cvtsi2sd(
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Xmm(dst_r), Operand::Reg(src_r)) => {
            let rex_r = dst_r.is_extended();
            let rex_b = src_r.is_extended();
            bytes.push(rex_w(rex_r, false, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x2A]);
            bytes.push(0xC0 | (dst_r.code() << 3) | src_r.code());
        }
        (Operand::Xmm(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(dst_r.is_extended(), rex_x, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x2A]);
            bytes.push(modrm);
            if let Some(s) = sib { bytes.push(s); }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Cvtsi2sd: destination must be XMM, source GPR or memory".into(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Cvttsd2si (truncate double to int32/int64)
// ---------------------------------------------------------------------------
//
// Encoding:
//   cvttsd2si r32, xmm2/m64  →  F2 0F 2C /r
//   cvttsd2si r64, xmm2/m64  →  F2 REX.W 0F 2C /r

fn encode_cvttsd2si(
    dst: Operand,
    src: Operand,
    bytes: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match (dst, src) {
        (Operand::Reg(dst_r), Operand::Xmm(src_r)) => {
            let rex_r = src_r.is_extended();
            let rex_b = dst_r.is_extended();
            bytes.push(rex_w(rex_r, false, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x2C]);
            bytes.push(0xC0 | (src_r.code() << 3) | dst_r.code());
        }
        (Operand::Reg(dst_r), Operand::Mem(mem)) => {
            let (modrm, sib, disp_sz, rex_b, rex_x) = encode_mem_parts(dst_r.code(), mem)?;
            bytes.push(rex_w(false, rex_x, rex_b));
            bytes.extend_from_slice(&[0xF2, 0x0F, 0x2C]);
            bytes.push(modrm);
            if let Some(s) = sib { bytes.push(s); }
            push_displacement(mem.disp, disp_sz, bytes);
        }
        _ => {
            return Err(EncodeError::UnsupportedOperand(
                "Cvttsd2si: destination must be GPR, source XMM or memory".into(),
            ));
        }
    }
    Ok(())
}
