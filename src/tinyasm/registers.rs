#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
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
    pub fn code(self) -> u8 {
        match self {
            Register::RAX | Register::R8 => 0,
            Register::RCX | Register::R9 => 1,
            Register::RDX | Register::R10 => 2,
            Register::RBX | Register::R11 => 3,
            Register::RSP | Register::R12 => 4,
            Register::RBP | Register::R13 => 5,
            Register::RSI | Register::R14 => 6,
            Register::RDI | Register::R15 => 7,
        }
    }

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
            Register::R8 => "r8",
            Register::R9 => "r9",
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

// ---------------------------------------------------------------------------
// Control registers (CR0–CR4, CR8)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlRegister {
    CR0,
    CR1,
    CR2,
    CR3,
    CR4,
    CR8,
}

impl ControlRegister {
    pub fn code(self) -> u8 {
        match self {
            ControlRegister::CR0 => 0,
            ControlRegister::CR1 => 1,
            ControlRegister::CR2 => 2,
            ControlRegister::CR3 => 3,
            ControlRegister::CR4 => 4,
            ControlRegister::CR8 => 8,
        }
    }

    pub fn is_extended(self) -> bool {
        matches!(self, ControlRegister::CR8)
    }
}

impl std::fmt::Display for ControlRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ControlRegister::CR0 => "cr0",
            ControlRegister::CR1 => "cr1",
            ControlRegister::CR2 => "cr2",
            ControlRegister::CR3 => "cr3",
            ControlRegister::CR4 => "cr4",
            ControlRegister::CR8 => "cr8",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// XMM registers (XMM0–XMM15)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmmRegister {
    XMM0,
    XMM1,
    XMM2,
    XMM3,
    XMM4,
    XMM5,
    XMM6,
    XMM7,
    XMM8,
    XMM9,
    XMM10,
    XMM11,
    XMM12,
    XMM13,
    XMM14,
    XMM15,
}

impl XmmRegister {
    pub fn code(self) -> u8 {
        match self {
            XmmRegister::XMM0 | XmmRegister::XMM8 => 0,
            XmmRegister::XMM1 | XmmRegister::XMM9 => 1,
            XmmRegister::XMM2 | XmmRegister::XMM10 => 2,
            XmmRegister::XMM3 | XmmRegister::XMM11 => 3,
            XmmRegister::XMM4 | XmmRegister::XMM12 => 4,
            XmmRegister::XMM5 | XmmRegister::XMM13 => 5,
            XmmRegister::XMM6 | XmmRegister::XMM14 => 6,
            XmmRegister::XMM7 | XmmRegister::XMM15 => 7,
        }
    }

    pub fn is_extended(self) -> bool {
        matches!(
            self,
            XmmRegister::XMM8
                | XmmRegister::XMM9
                | XmmRegister::XMM10
                | XmmRegister::XMM11
                | XmmRegister::XMM12
                | XmmRegister::XMM13
                | XmmRegister::XMM14
                | XmmRegister::XMM15
        )
    }
}

impl std::fmt::Display for XmmRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            XmmRegister::XMM0 => "xmm0",
            XmmRegister::XMM1 => "xmm1",
            XmmRegister::XMM2 => "xmm2",
            XmmRegister::XMM3 => "xmm3",
            XmmRegister::XMM4 => "xmm4",
            XmmRegister::XMM5 => "xmm5",
            XmmRegister::XMM6 => "xmm6",
            XmmRegister::XMM7 => "xmm7",
            XmmRegister::XMM8 => "xmm8",
            XmmRegister::XMM9 => "xmm9",
            XmmRegister::XMM10 => "xmm10",
            XmmRegister::XMM11 => "xmm11",
            XmmRegister::XMM12 => "xmm12",
            XmmRegister::XMM13 => "xmm13",
            XmmRegister::XMM14 => "xmm14",
            XmmRegister::XMM15 => "xmm15",
        };
        write!(f, "{}", s)
    }
}
