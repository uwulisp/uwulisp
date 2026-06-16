#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Register {
    RAX,
    RCX,
    RDX,
    RBX,
    RSP,
    RBP,
    RSI,
    RDI,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Register {
    /// Returns the 3-bit encoding used in ModR/M, SIB, and opcode fields.
    ///
    /// For extended registers (R8–R15) the high bit is carried by the REX
    /// prefix; only the low 3 bits are returned here.
    pub fn code(self) -> u8 {
        match self {
            Register::RAX | Register::R8  => 0,
            Register::RCX | Register::R9  => 1,
            Register::RDX | Register::R10 => 2,
            Register::RBX | Register::R11 => 3,
            Register::RSP | Register::R12 => 4,
            Register::RBP | Register::R13 => 5,
            Register::RSI | Register::R14 => 6,
            Register::RDI | Register::R15 => 7,
        }
    }

    /// Returns `true` for R8–R15, which require the REX.B/R/X extension bit.
    pub fn is_extended(self) -> bool {
        matches!(
            self,
            Register::R8
                | Register::R9
                | Register::R10
                | Register::R11
                | Register::R12
                | Register::R13
                | Register::R14
                | Register::R15
        )
    }
}

impl std::fmt::Display for Register {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Register::RAX => "rax",
            Register::RCX => "rcx",
            Register::RDX => "rdx",
            Register::RBX => "rbx",
            Register::RSP => "rsp",
            Register::RBP => "rbp",
            Register::RSI => "rsi",
            Register::RDI => "rdi",
            Register::R8  => "r8",
            Register::R9  => "r9",
            Register::R10 => "r10",
            Register::R11 => "r11",
            Register::R12 => "r12",
            Register::R13 => "r13",
            Register::R14 => "r14",
            Register::R15 => "r15",
        };
        write!(f, "{}", s)
    }
}