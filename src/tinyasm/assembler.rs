use std::collections::HashMap;
use crate::tinyasm::encoder::{Instruction, encode_instruction, EncodeError};

pub struct Assembler {
    instructions: Vec<Instruction>,
    /// Label name → byte offset determined in pass 1.
    labels: HashMap<String, usize>,
    debug: bool,
}

impl Assembler {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            labels: HashMap::new(),
            debug: false,
        }
    }

    /// Enable verbose per-instruction debug output on stderr.
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn add_instruction(&mut self, instr: Instruction) {
        self.instructions.push(instr);
    }

    // -----------------------------------------------------------------------
    // Two-pass assembly
    // -----------------------------------------------------------------------

    /// Assemble all queued instructions into a flat machine-code byte buffer.
    ///
    /// **Pass 1** walks the instruction list, records each label's byte offset
    /// in `self.labels`, and caches the encoded size of every non-label
    /// instruction.  Jump instructions use fixed-width encodings so that offsets
    /// computed here remain stable in pass 2.
    ///
    /// **Pass 2** emits the actual machine bytes, resolving label names to
    /// rel32 displacements.
    pub fn assemble(&mut self) -> Result<Vec<u8>, EncodeError> {
        let sizes = self.run_pass1()?;
        self.run_pass2(&sizes)
    }

    // --- Pass 1: symbol table + size cache --------------------------------

    /// Returns a per-instruction size table (0 for labels, actual byte count
    /// for everything else).  Encoding each non-jump instruction once here
    /// avoids redundant encoding in pass 2.
    fn run_pass1(&mut self) -> Result<Vec<usize>, EncodeError> {
        if self.debug { eprintln!("=== [Pass 1] Symbol Resolution ==="); }

        let mut sizes  = Vec::with_capacity(self.instructions.len());
        let mut offset = 0usize;
        self.labels.clear();

        for instr in &self.instructions {
            match instr {
                Instruction::Label(name) => {
                    if self.labels.contains_key(name) {
                        return Err(EncodeError::Other(
                            format!("Duplicate label: '{}'", name)
                        ));
                    }
                    self.labels.insert(name.clone(), offset);
                    if self.debug {
                        eprintln!("  label '{}' → 0x{:04X}", name, offset);
                    }
                    sizes.push(0);
                }
                other => {
                    let sz = self.fixed_size(other)?;
                    offset += sz;
                    sizes.push(sz);
                }
            }
        }
        Ok(sizes)
    }

    // --- Pass 2: emit bytes -----------------------------------------------

    fn run_pass2(&self, sizes: &[usize]) -> Result<Vec<u8>, EncodeError> {
        if self.debug { eprintln!("=== [Pass 2] Code Generation ==="); }

        let mut out = Vec::new();

        for (instr, &_sz) in self.instructions.iter().zip(sizes.iter()) {
            let offset = out.len();

            if self.debug {
                eprint!("  [0x{:04X}] {:<35} → ", offset, instr.to_string());
            }

            match instr {
                Instruction::Label(_) => {
                    if self.debug { eprintln!("(label — no bytes)"); }
                    continue;
                }

                Instruction::JmpLabel(target) => {
                    // E9 rel32  (5 bytes)
                    let bytes = self.encode_rel32_jump(&[0xE9], target, offset, 5)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JeLabel(target) => {
                    // 0F 84 rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x84], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JneLabel(target) => {
                    // 0F 85 rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x85], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JlLabel(target) => {
                    // 0F 8C rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x8C], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JleLabel(target) => {
                    // 0F 8E rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x8E], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JgeLabel(target) => {
                    // 0F 8D rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x8D], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
                Instruction::JgLabel(target) => {
                    // 0F 8F rel32  (6 bytes)
                    let bytes = self.encode_rel32_jump(&[0x0F, 0x8F], target, offset, 6)?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }

                other => {
                    let bytes = encode_instruction(other.clone())?;
                    self.debug_bytes(&bytes);
                    out.extend(bytes);
                }
            }
        }

        Ok(out)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Encode a near jump with a rel32 displacement.
    ///
    /// - `prefix`       — opcode byte(s) before the displacement (e.g. `&[0xE9]`)
    /// - `target`       — label to jump to
    /// - `instr_offset` — byte offset of this instruction in the output buffer
    /// - `instr_len`    — total byte length of this instruction (prefix bytes + 4)
    fn encode_rel32_jump(
        &self,
        prefix:       &[u8],
        target:       &str,
        instr_offset: usize,
        instr_len:    usize,
    ) -> Result<Vec<u8>, EncodeError> {
        let target_offset = *self.labels.get(target).ok_or_else(|| {
            EncodeError::Other(format!("Undefined label: '{}'", target))
        })?;

        // The processor adds the displacement to IP *after* the jump instruction.
        let next_ip = instr_offset + instr_len;
        let rel: i32 = (target_offset as i64 - next_ip as i64)
            .try_into()
            .map_err(|_| EncodeError::Other(
                format!("Jump to '{}' is out of rel32 range", target)
            ))?;

        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(&rel.to_le_bytes());
        Ok(bytes)
    }

    /// Returns the fixed encoded size (in bytes) of an instruction without
    /// allocating a full byte buffer.
    ///
    /// For labels (0 bytes) and jump instructions (fixed-width rel32 encodings)
    /// this avoids calling `encode_instruction`, which would reject them.
    fn fixed_size(&self, instr: &Instruction) -> Result<usize, EncodeError> {
        match instr {
            Instruction::Label(_)     => Ok(0),
            Instruction::JmpLabel(_)  => Ok(5),
            Instruction::JeLabel(_)
            | Instruction::JneLabel(_)
            | Instruction::JlLabel(_)
            | Instruction::JleLabel(_)
            | Instruction::JgeLabel(_)
            | Instruction::JgLabel(_) => Ok(6),
            other => encode_instruction(other.clone()).map(|b| b.len()),
        }
    }

    fn debug_bytes(&self, bytes: &[u8]) {
        if self.debug {
            let hex: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
            eprintln!("[{}]", hex.join(" "));
        }
    }
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}