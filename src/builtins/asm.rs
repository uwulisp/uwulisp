use crate::{
    env::{Env, env_set},
    expr::Expr,
};
use std::rc::Rc;

use crate::tinyasm::registers::Register;
use crate::tinyasm::encoder::{Instruction, Operand, MemoryAddr};
use crate::tinyasm::assembler::Assembler;
use crate::tinyasm::jit::JitMemory;

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse an `Expr::Symbol` into an x86-64 register.
fn parse_register(s: &str) -> Result<Register, String> {
    match s.to_uppercase().as_str() {
        "RAX" => Ok(Register::RAX),
        "RCX" => Ok(Register::RCX),
        "RDX" => Ok(Register::RDX),
        "RBX" => Ok(Register::RBX),
        "RSP" => Ok(Register::RSP),
        "RBP" => Ok(Register::RBP),
        "RSI" => Ok(Register::RSI),
        "RDI" => Ok(Register::RDI),
        "R8"  => Ok(Register::R8),
        "R9"  => Ok(Register::R9),
        "R10" => Ok(Register::R10),
        "R11" => Ok(Register::R11),
        "R12" => Ok(Register::R12),
        "R13" => Ok(Register::R13),
        "R14" => Ok(Register::R14),
        "R15" => Ok(Register::R15),
        _     => Err(format!("unknown register: '{}'", s)),
    }
}

/// Parse an `Expr` into an `Operand`.
///
/// Supported forms:
/// - `rax` / `r8` etc. → `Operand::Reg`
/// - integer literal   → `Operand::Imm32` (must fit in i32)
/// - `(mem base disp)` → `Operand::Mem` with base register + i32 displacement
fn parse_operand(expr: &Expr) -> Result<Operand, String> {
    match expr {
        Expr::Symbol(s) => {
            let reg = parse_register(s)?;
            Ok(Operand::Reg(reg))
        }
        Expr::Number(n) => {
            // Guard against silent truncation of large f64 values.
            let n = *n;
            if n < i32::MIN as f64 || n > i32::MAX as f64 {
                return Err(format!(
                    "immediate value {} is out of i32 range; use Imm64 for large constants",
                    n
                ));
            }
            Ok(Operand::Imm32(n as i32))
        }
        // (mem <base-register> <displacement>)
        Expr::List(parts) if parts.len() >= 1 => {
            if let Expr::Symbol(head) = &parts[0] {
                if head.as_str() == "mem" {
                    let base = match parts.get(1) {
                        Some(Expr::Symbol(s)) => Some(parse_register(s)?),
                        Some(_) => return Err("mem: base must be a register symbol".into()),
                        None    => None,
                    };
                    let disp = match parts.get(2) {
                        Some(Expr::Number(n)) => *n as i32,
                        Some(_) => return Err("mem: displacement must be a number".into()),
                        None    => 0,
                    };
                    let mem = match base {
                        Some(r) => MemoryAddr::base_disp(r, disp),
                        None    => MemoryAddr { base: None, index: None, scale: 1, disp },
                    };
                    return Ok(Operand::Mem(mem));
                }
            }
            Err(format!("invalid operand: {:?}", expr))
        }
        _ => Err(format!("invalid operand type: {:?}", expr)),
    }
}

/// Extract a symbol string from an `Expr`, for use as a label name.
fn parse_label_name(expr: &Expr, context: &str) -> Result<String, String> {
    match expr {
        Expr::Symbol(s) => Ok(s.clone()),
        _ => Err(format!("{}: label name must be a symbol, got {:?}", context, expr)),
    }
}

// ---------------------------------------------------------------------------
// One-operand instruction helper
// ---------------------------------------------------------------------------

fn parse_unary(
    parts: &[Expr],
    mnemonic: &str,
    make: fn(Operand) -> Instruction,
) -> Result<Instruction, String> {
    if parts.len() != 2 {
        return Err(format!("{}: expects 1 operand", mnemonic));
    }
    Ok(make(parse_operand(&parts[1])?))
}

// ---------------------------------------------------------------------------
// `asm` built-in
// ---------------------------------------------------------------------------

/// Register the `asm` built-in function into `env`.
///
/// Usage from Lisp:
/// ```lisp
/// (asm '(
///   (mov rax 0)
///   (label loop)
///   (add rax 1)
///   (cmp rax 5)
///   (jne loop)
///   (ret)
/// ))
/// ```
///
/// Returns the value left in RAX after execution.
pub fn assemble(env: &Env) {
    env_set(
        env,
        "asm".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("asm: expects exactly 1 argument (list of instructions)".into());
            }

            let mut asm = Assembler::new();

            let Expr::List(inst_exprs) = &args[0] else {
                return Err("asm: argument must be a list of instructions".into());
            };

            for inst_expr in inst_exprs {
                let Expr::List(parts) = inst_expr else {
                    return Err(format!(
                        "asm: each instruction must be a list, got {:?}", inst_expr
                    ));
                };
                if parts.is_empty() { continue; }

                let op = match &parts[0] {
                    Expr::Symbol(s) => s.as_str(),
                    _ => return Err(format!(
                        "asm: instruction mnemonic must be a symbol, got {:?}", parts[0]
                    )),
                };

                let instr = match op {
                    // --- Data movement ---
                    "mov" => {
                        if parts.len() != 3 { return Err("mov: expects 2 operands".into()); }
                        Instruction::Mov(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "push" => parse_unary(parts, "push", Instruction::Push)?,
                    "pop"  => parse_unary(parts, "pop",  Instruction::Pop)?,

                    // --- Arithmetic ---
                    "add" => {
                        if parts.len() != 3 { return Err("add: expects 2 operands".into()); }
                        Instruction::Add(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "sub" => {
                        if parts.len() != 3 { return Err("sub: expects 2 operands".into()); }
                        Instruction::Sub(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "imul" => {
                        if parts.len() != 3 { return Err("imul: expects 2 operands".into()); }
                        Instruction::IMul(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "mul" => parse_unary(parts, "mul", Instruction::Mul)?,
                    "div" => parse_unary(parts, "div", Instruction::Div)?,

                    // --- Bitwise / shift ---
                    "and" => {
                        if parts.len() != 3 { return Err("and: expects 2 operands".into()); }
                        Instruction::And(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "or" => {
                        if parts.len() != 3 { return Err("or: expects 2 operands".into()); }
                        Instruction::Or(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "xor" => {
                        if parts.len() != 3 { return Err("xor: expects 2 operands".into()); }
                        Instruction::Xor(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "not" => parse_unary(parts, "not", Instruction::Not)?,
                    "shl" => {
                        if parts.len() != 3 { return Err("shl: expects 2 operands".into()); }
                        Instruction::Shl(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "shr" => {
                        if parts.len() != 3 { return Err("shr: expects 2 operands".into()); }
                        Instruction::Shr(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }

                    // --- Compare / test ---
                    "cmp" => {
                        if parts.len() != 3 { return Err("cmp: expects 2 operands".into()); }
                        Instruction::Cmp(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "test" => {
                        if parts.len() != 3 { return Err("test: expects 2 operands".into()); }
                        Instruction::Test(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }

                    // --- Control flow ---
                    "call" => parse_unary(parts, "call", Instruction::Call)?,
                    "ret"  => Instruction::Ret,
                    "syscall" => Instruction::Syscall,

                    // --- Labels and jumps ---
                    "label" => {
                        if parts.len() != 2 { return Err("label: expects 1 name".into()); }
                        Instruction::Label(parse_label_name(&parts[1], "label")?)
                    }
                    "jmp" => {
                        if parts.len() != 2 { return Err("jmp: expects 1 target label".into()); }
                        Instruction::JmpLabel(parse_label_name(&parts[1], "jmp")?)
                    }
                    "je" => {
                        if parts.len() != 2 { return Err("je: expects 1 target label".into()); }
                        Instruction::JeLabel(parse_label_name(&parts[1], "je")?)
                    }
                    "jne" => {
                        if parts.len() != 2 { return Err("jne: expects 1 target label".into()); }
                        Instruction::JneLabel(parse_label_name(&parts[1], "jne")?)
                    }
                    "jl" => {
                        if parts.len() != 2 { return Err("jl: expects 1 target label".into()); }
                        Instruction::JlLabel(parse_label_name(&parts[1], "jl")?)
                    }
                    "jle" => {
                        if parts.len() != 2 { return Err("jle: expects 1 target label".into()); }
                        Instruction::JleLabel(parse_label_name(&parts[1], "jle")?)
                    }
                    "jge" => {
                        if parts.len() != 2 { return Err("jge: expects 1 target label".into()); }
                        Instruction::JgeLabel(parse_label_name(&parts[1], "jge")?)
                    }
                    "jg" => {
                        if parts.len() != 2 { return Err("jg: expects 1 target label".into()); }
                        Instruction::JgLabel(parse_label_name(&parts[1], "jg")?)
                    }

                    _ => return Err(format!("asm: unsupported instruction '{}'", op)),
                };

                asm.add_instruction(instr);
            }

            // Assemble to machine code.
            let code = asm.assemble()
                .map_err(|e| format!("assembly error: {}", e))?;

            // Allocate executable memory, write the code, flip permissions.
            let mut jit = JitMemory::new(code.len())
                .map_err(|e| format!("JIT allocation failed: {}", e))?;
            jit.write(&code)
                .map_err(|e| format!("JIT write failed: {}", e))?;
            jit.make_executable()
                .map_err(|e| format!("JIT mprotect failed: {}", e))?;

            // Execute and return RAX as a Lisp Number.
            let result = unsafe {
                let f = jit.as_fn()
                    .map_err(|e| format!("JIT fn pointer failed: {}", e))?;
                f()
            };
            Ok(Expr::Number(result as f64))
        })),
    );
}