use crate::tinyasm::{
    assembler::Assembler,
    encoder::{Instruction, MemoryAddr, Operand},
    registers::Register,
    jit::JitMemory,
};
use crate::vm::bytecode::{Chunk, Op, Value};

pub struct JitCompiler;

impl JitCompiler {
    pub fn compile_chunk(chunk: &Chunk) -> Result<(JitMemory, unsafe extern "C" fn(*mut crate::vm::jit_abi::JitFrame)), String> {
        let mut asm = Assembler::new();

        // Register usage:
        // RDI = frame_ptr (argument 1)
        // RBX = stack_ptr
        // R12 = stack_len
        // R13 = tag_ptr
        
        let frame_offset_stack_ptr = 0;
        let frame_offset_stack_len = 8;
        let frame_offset_result_tag = 24;
        let frame_offset_result_val = 32;
        let frame_offset_error = 40;
        let frame_offset_tag_ptr = 48;

        // Prologue
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::RBP)));
        asm.add_instruction(Instruction::Mov(Operand::Reg(Register::RBP), Operand::Reg(Register::RSP)));
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::RBX)));
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::R12)));
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::R13)));
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::R14)));
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::R15)));
        // align stack to 16 bytes
        asm.add_instruction(Instruction::Sub(Operand::Reg(Register::RSP), Operand::Imm32(8)));

        // Load fields from JitFrame (RDI)
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::RBX),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_stack_ptr))
        ));
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::R12),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_stack_len))
        ));
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::R13),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_tag_ptr))
        ));

        // Iterate ops
        for (i, op) in chunk.ops.iter().enumerate() {
            asm.add_instruction(Instruction::Label(format!("op_{}", i)));

            match op {
                Op::LoadConst(Value::Number(n)) => {
                    let bits = n.to_bits();
                    asm.add_instruction(Instruction::Mov(Operand::Reg(Register::RAX), Operand::Imm64(bits)));
                    
                    // tag_ptr[r12*8] = 0 (Number)
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr { base: Some(Register::R13), index: Some(Register::R12), scale: 8, disp: 0 }),
                        Operand::Imm32(0)
                    ));
                    // stack_ptr[r12*8] = bits
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr { base: Some(Register::RBX), index: Some(Register::R12), scale: 8, disp: 0 }),
                        Operand::Reg(Register::RAX)
                    ));
                    
                    asm.add_instruction(Instruction::Add(Operand::Reg(Register::R12), Operand::Imm32(1)));
                }
                Op::LoadConst(Value::Nil) => {
                    // tag_ptr[r12*8] = 1 (Nil)
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr { base: Some(Register::R13), index: Some(Register::R12), scale: 8, disp: 0 }),
                        Operand::Imm32(1)
                    ));
                    // stack_ptr[r12*8] = 0
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr { base: Some(Register::RBX), index: Some(Register::R12), scale: 8, disp: 0 }),
                        Operand::Imm32(0)
                    ));
                    asm.add_instruction(Instruction::Add(Operand::Reg(Register::R12), Operand::Imm32(1)));
                }
                Op::Pop => {
                    asm.add_instruction(Instruction::Sub(Operand::Reg(Register::R12), Operand::Imm32(1)));
                }
                Op::Jump(target) => {
                    asm.add_instruction(Instruction::JmpLabel(format!("op_{}", target)));
                }
                Op::JumpIfFalse(target) => {
                    asm.add_instruction(Instruction::Sub(Operand::Reg(Register::R12), Operand::Imm32(1)));
                    
                    // rdx = tag
                    asm.add_instruction(Instruction::Mov(
                        Operand::Reg(Register::RDX),
                        Operand::Mem(MemoryAddr { base: Some(Register::R13), index: Some(Register::R12), scale: 8, disp: 0 })
                    ));
                    // rax = val
                    asm.add_instruction(Instruction::Mov(
                        Operand::Reg(Register::RAX),
                        Operand::Mem(MemoryAddr { base: Some(Register::RBX), index: Some(Register::R12), scale: 8, disp: 0 })
                    ));

                    // tag == 1 (Nil) -> jump
                    asm.add_instruction(Instruction::Cmp(Operand::Reg(Register::RDX), Operand::Imm32(1)));
                    asm.add_instruction(Instruction::JeLabel(format!("op_{}", target)));

                    // tag == 0 (Number)
                    asm.add_instruction(Instruction::Cmp(Operand::Reg(Register::RDX), Operand::Imm32(0)));
                    asm.add_instruction(Instruction::JneLabel(format!("jif_cont_{}", i)));

                    // is val 0? (0.0 has bits 0)
                    asm.add_instruction(Instruction::Cmp(Operand::Reg(Register::RAX), Operand::Imm32(0)));
                    asm.add_instruction(Instruction::JeLabel(format!("op_{}", target)));

                    asm.add_instruction(Instruction::Label(format!("jif_cont_{}", i)));
                }
                Op::Return => {
                    asm.add_instruction(Instruction::Sub(Operand::Reg(Register::R12), Operand::Imm32(1)));
                    
                    // Result tag
                    asm.add_instruction(Instruction::Mov(
                        Operand::Reg(Register::RDX),
                        Operand::Mem(MemoryAddr { base: Some(Register::R13), index: Some(Register::R12), scale: 8, disp: 0 })
                    ));
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_result_tag)),
                        Operand::Reg(Register::RDX)
                    ));

                    // Result val
                    asm.add_instruction(Instruction::Mov(
                        Operand::Reg(Register::RAX),
                        Operand::Mem(MemoryAddr { base: Some(Register::RBX), index: Some(Register::R12), scale: 8, disp: 0 })
                    ));
                    asm.add_instruction(Instruction::Mov(
                        Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_result_val)),
                        Operand::Reg(Register::RAX)
                    ));
                    
                    asm.add_instruction(Instruction::JmpLabel("epilogue".to_string()));
                }
                Op::LoadVar(name) => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_load_var as usize, Some(name.as_ptr() as u64), Some(name.len() as u64));
                }
                Op::StoreVar(name) => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_store_var as usize, Some(name.as_ptr() as u64), Some(name.len() as u64));
                }
                Op::Call(n_args) => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_call as usize, Some(*n_args as u64), None);
                }
                Op::MakeFunc { code_offset, .. } => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_make_func as usize, Some(*code_offset as u64), None);
                }
                Op::TreeEval(_) => {
                    // We need a pointer to the Expr, but for Phase 1 we can't easily embed it safely unless we pin it or pass index.
                    // For now, this just calls the helper with arg 0.
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_tree_eval as usize, Some(0), None);
                }
                Op::PushEnv => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_push_env as usize, None, None);
                }
                Op::PopEnv => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_pop_env as usize, None, None);
                }
                Op::StoreSelf(_) => {
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_store_self as usize, Some(0), Some(0));
                }
                _ => {
                    // Fallback to error for unimplemented ops
                    Self::emit_helper_call(&mut asm, crate::vm::jit_abi::jit_helper_tree_eval as usize, Some(0), None);
                }
            }
        }

        asm.add_instruction(Instruction::Label("epilogue".to_string()));
        // Epilogue
        // Write back stack_len
        asm.add_instruction(Instruction::Mov(
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, frame_offset_stack_len)),
            Operand::Reg(Register::R12)
        ));
        
        asm.add_instruction(Instruction::Add(Operand::Reg(Register::RSP), Operand::Imm32(8))); // unalign stack
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::R15)));
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::R14)));
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::R13)));
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::R12)));
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::RBX)));
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::RBP)));
        asm.add_instruction(Instruction::Ret);

        let bytes = asm.assemble().map_err(|e| e.to_string())?;
        let mut mem = JitMemory::new(bytes.len()).map_err(|e| e.to_string())?;
        mem.write(&bytes).map_err(|e| e.to_string())?;
        mem.make_executable().map_err(|e| e.to_string())?;
        
        let raw_fn = unsafe { mem.as_fn().map_err(|e| e.to_string())? };
        let fp: unsafe extern "C" fn(*mut crate::vm::jit_abi::JitFrame) = unsafe { std::mem::transmute(raw_fn) };

        Ok((mem, fp))
    }

    fn emit_helper_call(asm: &mut Assembler, helper_ptr: usize, arg1: Option<u64>, arg2: Option<u64>) {
        // flush stack_len
        asm.add_instruction(Instruction::Mov(
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, 8)),
            Operand::Reg(Register::R12)
        ));
        
        // Push RDI because it's caller-saved in System V AMD64 ABI, and we need it back after call
        asm.add_instruction(Instruction::Push(Operand::Reg(Register::RDI)));
        
        // Align stack to 16 bytes before call (since Push RDI misaligned it by 8 bytes)
        asm.add_instruction(Instruction::Sub(Operand::Reg(Register::RSP), Operand::Imm32(8)));

        if let Some(a1) = arg1 {
            asm.add_instruction(Instruction::Mov(Operand::Reg(Register::RSI), Operand::Imm64(a1)));
        }
        if let Some(a2) = arg2 {
            asm.add_instruction(Instruction::Mov(Operand::Reg(Register::RDX), Operand::Imm64(a2)));
        }

        // Call helper
        asm.add_instruction(Instruction::Mov(Operand::Reg(Register::RAX), Operand::Imm64(helper_ptr as u64)));
        asm.add_instruction(Instruction::Call(Operand::Reg(Register::RAX)));

        // Unalign stack
        asm.add_instruction(Instruction::Add(Operand::Reg(Register::RSP), Operand::Imm32(8)));

        // Pop RDI
        asm.add_instruction(Instruction::Pop(Operand::Reg(Register::RDI)));

        // reload rbx, r12, r13 (stack might have reallocated, though for Phase 1 it's static capacity)
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::RBX),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, 0))
        ));
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::R12),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, 8))
        ));
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::R13),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, 48))
        ));

        // Check for error
        asm.add_instruction(Instruction::Mov(
            Operand::Reg(Register::RAX),
            Operand::Mem(MemoryAddr::base_disp(Register::RDI, 40))
        ));
        asm.add_instruction(Instruction::Cmp(Operand::Reg(Register::RAX), Operand::Imm32(0)));
        asm.add_instruction(Instruction::JneLabel("epilogue".to_string()));
    }
}
