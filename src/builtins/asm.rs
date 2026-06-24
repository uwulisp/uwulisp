#![cfg(target_arch = "x86_64")]
// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

use std::fs;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

use crate::{
    env::{Env, env_set},
    expr::Expr,
    gc::Heap,
    tinyasm::{Assembler, Instruction, JitMemory, MemoryAddr, Operand, Register},
};

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
        "R8" => Ok(Register::R8),
        "R9" => Ok(Register::R9),
        "R10" => Ok(Register::R10),
        "R11" => Ok(Register::R11),
        "R12" => Ok(Register::R12),
        "R13" => Ok(Register::R13),
        "R14" => Ok(Register::R14),
        "R15" => Ok(Register::R15),
        _ => Err(format!("unknown register: '{}'", s)),
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
        Expr::Int(n) => {
            // Guard against silent truncation of large i64 values.
            let n = *n;
            if n < i32::MIN as i64 || n > i32::MAX as i64 {
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
                        None => None,
                    };
                    let disp = match parts.get(2) {
                        Some(Expr::Int(n)) => *n as i32,
                        Some(_) => return Err("mem: displacement must be a number".into()),
                        None => 0,
                    };
                    let mem = match base {
                        Some(r) => MemoryAddr::base_disp(r, disp),
                        None => MemoryAddr {
                            base: None,
                            index: None,
                            scale: 1,
                            disp,
                        },
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
        _ => Err(format!(
            "{}: label name must be a symbol, got {:?}",
            context, expr
        )),
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
// NASM-style text parsing helpers (used by `load-asm`)
// ---------------------------------------------------------------------------

/// Parse a single raw token from a NASM-style source line into an `Operand`.
///
/// Supported token forms:
/// - `rax`, `r8`, …                      → `Operand::Reg`
/// - decimal integer, e.g. `42`, `-7`    → `Operand::Imm32`
/// - hex integer, e.g. `0xff`, `0xFF`    → `Operand::Imm32`
/// - `[rax]`, `[rax+8]`, `[rax-8]`      → `Operand::Mem`
fn parse_text_operand(token: &str) -> Result<Operand, String> {
    let token = token.trim();

    // Memory reference: [base] or [base+disp] or [base-disp]
    if token.starts_with('[') && token.ends_with(']') {
        let inner = &token[1..token.len() - 1];

        // Split on the first '+' or '-', keeping sign on the displacement.
        let (base_str, disp) = if let Some(pos) = inner.find('+') {
            let disp_str = inner[pos + 1..].trim();
            let d = parse_integer(disp_str).map_err(|e| format!("mem displacement: {}", e))?;
            (&inner[..pos], d)
        } else if let Some(pos) = inner.rfind('-') {
            // rfind so we don't accidentally split a negative-only value like [-8]
            if pos == 0 {
                // Entire inner is a negative number with no base register.
                let d = parse_integer(inner).map_err(|e| format!("mem displacement: {}", e))?;
                return Ok(Operand::Mem(MemoryAddr {
                    base: None,
                    index: None,
                    scale: 1,
                    disp: d,
                }));
            }
            let disp_str = inner[pos..].trim(); // includes the '-'
            let d = parse_integer(disp_str).map_err(|e| format!("mem displacement: {}", e))?;
            (&inner[..pos], d)
        } else {
            (inner, 0i32)
        };

        let base_str = base_str.trim();
        if base_str.is_empty() {
            return Ok(Operand::Mem(MemoryAddr {
                base: None,
                index: None,
                scale: 1,
                disp,
            }));
        }

        let base = parse_register(base_str)?;
        return Ok(Operand::Mem(MemoryAddr::base_disp(base, disp)));
    }

    // Try as a register first.
    if let Ok(reg) = parse_register(token) {
        return Ok(Operand::Reg(reg));
    }

    // Otherwise treat as an integer immediate.
    let n = parse_integer(token)?;
    Ok(Operand::Imm32(n))
}

/// Parse a decimal or hexadecimal integer string into an `i32`.
fn parse_integer(s: &str) -> Result<i32, String> {
    let s = s.trim();
    let (neg, s) = if let Some(rest) = s.strip_prefix('-') {
        (true, rest.trim())
    } else {
        (false, s)
    };

    let abs: i64 = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).map_err(|_| format!("invalid hex integer: '{}'", s))?
    } else {
        s.parse::<i64>()
            .map_err(|_| format!("invalid integer: '{}'", s))?
    };

    let value = if neg { -abs } else { abs };
    if value < i32::MIN as i64 || value > i32::MAX as i64 {
        return Err(format!(
            "immediate {} is out of i32 range; use Imm64 for large constants",
            value
        ));
    }
    Ok(value as i32)
}

/// Split a NASM-style line into `(mnemonic, [operand_token, …])`, stripping
/// inline comments (`;` to end-of-line) and ignoring blank lines.
///
/// Operands are comma-separated.  Size hints like `QWORD PTR` are dropped.
fn tokenize_line(line: &str) -> Option<(String, Vec<String>)> {
    // Strip inline comment.
    let line = match line.split_once(';') {
        Some((before, _)) => before,
        None => line,
    };
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let mut parts = line.splitn(2, char::is_whitespace);
    let mnemonic = parts.next()?.trim().to_lowercase();
    if mnemonic.is_empty() {
        return None;
    }

    let operand_src = parts.next().unwrap_or("").trim();

    // Drop NASM size hints like "QWORD PTR", "DWORD PTR", etc.
    let operand_src = {
        let upper = operand_src.to_uppercase();
        if upper.contains("PTR") {
            // Remove the size keyword and "PTR" leaving only the bracket expression.
            let after_ptr = upper
                .find("PTR")
                .map(|i| &operand_src[i + 3..])
                .unwrap_or(operand_src)
                .trim();
            after_ptr.to_owned()
        } else {
            operand_src.to_owned()
        }
    };

    let operands: Vec<String> = if operand_src.is_empty() {
        vec![]
    } else {
        operand_src
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    };

    Some((mnemonic, operands))
}

/// Parse one NASM-style text line into an `Instruction`.
///
/// Label lines are detected by a trailing colon, e.g. `loop:`.
/// All other lines use the standard mnemonic + operand forms.
fn parse_text_instruction(line: &str) -> Result<Option<Instruction>, String> {
    let line = line.trim();

    // Label definition: "name:"
    if let Some(name) = line.strip_suffix(':') {
        let name = name.trim();
        if name.is_empty() {
            return Err("empty label name".into());
        }
        return Ok(Some(Instruction::Label(name.to_owned())));
    }

    let Some((mnemonic, ops)) = tokenize_line(line) else {
        return Ok(None); // blank or comment-only line
    };

    // Helper closures for operand-count checking.
    let need = |n: usize| -> Result<(), String> {
        if ops.len() != n {
            Err(format!(
                "{}: expects {} operand(s), got {}",
                mnemonic,
                n,
                ops.len()
            ))
        } else {
            Ok(())
        }
    };
    let op = |i: usize| parse_text_operand(&ops[i]);

    let instr = match mnemonic.as_str() {
        // --- Data movement ---
        "mov" => {
            need(2)?;
            Instruction::Mov(op(0)?, op(1)?)
        }
        "lea" => {
            need(2)?;
            Instruction::Lea(op(0)?, op(1)?)
        }
        "push" => {
            need(1)?;
            Instruction::Push(op(0)?)
        }
        "pop" => {
            need(1)?;
            Instruction::Pop(op(0)?)
        }

        // --- Arithmetic ---
        "add" => {
            need(2)?;
            Instruction::Add(op(0)?, op(1)?)
        }
        "sub" => {
            need(2)?;
            Instruction::Sub(op(0)?, op(1)?)
        }
        "imul" => {
            need(2)?;
            Instruction::IMul(op(0)?, op(1)?)
        }
        "mul" => {
            need(1)?;
            Instruction::Mul(op(0)?)
        }
        "div" => {
            need(1)?;
            Instruction::Div(op(0)?)
        }

        // --- Bitwise / shift ---
        "and" => {
            need(2)?;
            Instruction::And(op(0)?, op(1)?)
        }
        "or" => {
            need(2)?;
            Instruction::Or(op(0)?, op(1)?)
        }
        "xor" => {
            need(2)?;
            Instruction::Xor(op(0)?, op(1)?)
        }
        "not" => {
            need(1)?;
            Instruction::Not(op(0)?)
        }
        "shl" => {
            need(2)?;
            Instruction::Shl(op(0)?, op(1)?)
        }
        "shr" => {
            need(2)?;
            Instruction::Shr(op(0)?, op(1)?)
        }

        // --- Compare / test ---
        "cmp" => {
            need(2)?;
            Instruction::Cmp(op(0)?, op(1)?)
        }
        "test" => {
            need(2)?;
            Instruction::Test(op(0)?, op(1)?)
        }

        // --- Control flow ---
        "call" => {
            need(1)?;
            Instruction::Call(op(0)?)
        }
        "ret" => {
            need(0)?;
            Instruction::Ret
        }
        "syscall" => {
            need(0)?;
            Instruction::Syscall
        }

        // --- Jumps ---
        "jmp" => {
            need(1)?;
            Instruction::JmpLabel(ops[0].clone())
        }
        "je" => {
            need(1)?;
            Instruction::JeLabel(ops[0].clone())
        }
        "jne" => {
            need(1)?;
            Instruction::JneLabel(ops[0].clone())
        }
        "jl" => {
            need(1)?;
            Instruction::JlLabel(ops[0].clone())
        }
        "jle" => {
            need(1)?;
            Instruction::JleLabel(ops[0].clone())
        }
        "jge" => {
            need(1)?;
            Instruction::JgeLabel(ops[0].clone())
        }
        "jg" => {
            need(1)?;
            Instruction::JgLabel(ops[0].clone())
        }

        // NASM section/global directives — silently ignored.
        "section" | "global" | "extern" | "bits" | "default" => return Ok(None),

        other => return Err(format!("load-asm: unsupported instruction '{}'", other)),
    };

    Ok(Some(instr))
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
pub fn register_assembler(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "asm".into(),
        Expr::Func(Rc::new(|args, _heap| {
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
                        "asm: each instruction must be a list, got {:?}",
                        inst_expr
                    ));
                };
                if parts.is_empty() {
                    continue;
                }

                let op = match &parts[0] {
                    Expr::Symbol(s) => s.as_str(),
                    _ => {
                        return Err(format!(
                            "asm: instruction mnemonic must be a symbol, got {:?}",
                            parts[0]
                        ));
                    }
                };

                let instr = match op {
                    // --- Data movement ---
                    "mov" => {
                        if parts.len() != 3 {
                            return Err("mov: expects 2 operands".into());
                        }
                        Instruction::Mov(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "lea" => {
                        if parts.len() != 3 {
                            return Err("lea: expects 2 operands".into());
                        }
                        Instruction::Lea(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "push" => parse_unary(parts, "push", Instruction::Push)?,
                    "pop" => parse_unary(parts, "pop", Instruction::Pop)?,

                    // --- Arithmetic ---
                    "add" => {
                        if parts.len() != 3 {
                            return Err("add: expects 2 operands".into());
                        }
                        Instruction::Add(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "sub" => {
                        if parts.len() != 3 {
                            return Err("sub: expects 2 operands".into());
                        }
                        Instruction::Sub(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "imul" => {
                        if parts.len() != 3 {
                            return Err("imul: expects 2 operands".into());
                        }
                        Instruction::IMul(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "mul" => parse_unary(parts, "mul", Instruction::Mul)?,
                    "div" => parse_unary(parts, "div", Instruction::Div)?,

                    // --- Bitwise / shift ---
                    "and" => {
                        if parts.len() != 3 {
                            return Err("and: expects 2 operands".into());
                        }
                        Instruction::And(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "or" => {
                        if parts.len() != 3 {
                            return Err("or: expects 2 operands".into());
                        }
                        Instruction::Or(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "xor" => {
                        if parts.len() != 3 {
                            return Err("xor: expects 2 operands".into());
                        }
                        Instruction::Xor(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "not" => parse_unary(parts, "not", Instruction::Not)?,
                    "shl" => {
                        if parts.len() != 3 {
                            return Err("shl: expects 2 operands".into());
                        }
                        Instruction::Shl(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "shr" => {
                        if parts.len() != 3 {
                            return Err("shr: expects 2 operands".into());
                        }
                        Instruction::Shr(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }

                    // --- Compare / test ---
                    "cmp" => {
                        if parts.len() != 3 {
                            return Err("cmp: expects 2 operands".into());
                        }
                        Instruction::Cmp(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }
                    "test" => {
                        if parts.len() != 3 {
                            return Err("test: expects 2 operands".into());
                        }
                        Instruction::Test(parse_operand(&parts[1])?, parse_operand(&parts[2])?)
                    }

                    // --- Control flow ---
                    "call" => parse_unary(parts, "call", Instruction::Call)?,
                    "ret" => Instruction::Ret,
                    "syscall" => Instruction::Syscall,

                    // --- Labels and jumps ---
                    "label" => {
                        if parts.len() != 2 {
                            return Err("label: expects 1 name".into());
                        }
                        Instruction::Label(parse_label_name(&parts[1], "label")?)
                    }
                    "jmp" => {
                        if parts.len() != 2 {
                            return Err("jmp: expects 1 target label".into());
                        }
                        Instruction::JmpLabel(parse_label_name(&parts[1], "jmp")?)
                    }
                    "je" => {
                        if parts.len() != 2 {
                            return Err("je: expects 1 target label".into());
                        }
                        Instruction::JeLabel(parse_label_name(&parts[1], "je")?)
                    }
                    "jne" => {
                        if parts.len() != 2 {
                            return Err("jne: expects 1 target label".into());
                        }
                        Instruction::JneLabel(parse_label_name(&parts[1], "jne")?)
                    }
                    "jl" => {
                        if parts.len() != 2 {
                            return Err("jl: expects 1 target label".into());
                        }
                        Instruction::JlLabel(parse_label_name(&parts[1], "jl")?)
                    }
                    "jle" => {
                        if parts.len() != 2 {
                            return Err("jle: expects 1 target label".into());
                        }
                        Instruction::JleLabel(parse_label_name(&parts[1], "jle")?)
                    }
                    "jge" => {
                        if parts.len() != 2 {
                            return Err("jge: expects 1 target label".into());
                        }
                        Instruction::JgeLabel(parse_label_name(&parts[1], "jge")?)
                    }
                    "jg" => {
                        if parts.len() != 2 {
                            return Err("jg: expects 1 target label".into());
                        }
                        Instruction::JgLabel(parse_label_name(&parts[1], "jg")?)
                    }

                    _ => return Err(format!("asm: unsupported instruction '{}'", op)),
                };

                asm.add_instruction(instr);
            }

            // Assemble to machine code.
            let code = asm
                .assemble()
                .map_err(|e| format!("assembly error: {}", e))?;

            // Allocate executable memory, write the code, flip permissions.
            let mut jit =
                JitMemory::new(code.len()).map_err(|e| format!("JIT allocation failed: {}", e))?;
            jit.write(&code)
                .map_err(|e| format!("JIT write failed: {}", e))?;
            jit.make_executable()
                .map_err(|e| format!("JIT mprotect failed: {}", e))?;

            // Execute and return RAX as a Lisp Number.
            let result = unsafe {
                let f = jit
                    .as_fn()
                    .map_err(|e| format!("JIT fn pointer failed: {}", e))?;
                f()
            };
            Ok(Expr::Int(result as i64))
        })),
    );
}

// ---------------------------------------------------------------------------
// `load-asm` built-in
// ---------------------------------------------------------------------------

/// Register the `load-asm` built-in function into `env`.
///
/// Usage from Lisp:
/// ```lisp
/// (load-asm "path/to/program.asm")
/// ```
///
/// The file is expected to contain NASM-style x86-64 assembly text, for example:
/// ```asm
/// ; count from 0 to 4, return 5 in rax
///     mov rax, 0
/// loop:
///     add rax, 1
///     cmp rax, 5
///     jne loop
///     ret
/// ```
///
/// Supported syntax:
/// - Labels defined with a trailing colon (`loop:`)
/// - Jump targets as bare label names (`jne loop`)
/// - Operands: registers (`rax`), immediate decimals/hex (`42`, `0xff`),
///   and memory references (`[rax]`, `[rax+8]`, `[rbp-16]`)
/// - Inline comments starting with `;`
/// - NASM directives `section`, `global`, `extern`, `bits`, `default`
///   are silently ignored
///
/// Returns the value left in RAX after execution as a Lisp `Number`.
pub fn register_load_asm(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "load-asm".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("load-asm: expects exactly 1 argument (filename string)".into());
            }

            // Extract the filename from a Lisp string or symbol.
            let filename = match &args[0] {
                Expr::Str(s) => s.clone(),
                Expr::Symbol(s) => s.clone(),
                other => {
                    return Err(format!(
                        "load-asm: filename must be a string, got {:?}",
                        other
                    ));
                }
            };

            // Read the file.
            let source = fs::read_to_string(&filename)
                .map_err(|e| format!("load-asm: cannot read '{}': {}", filename, e))?;

            // Parse each line into instructions.
            let mut asm = Assembler::new();
            for (line_no, raw_line) in source.lines().enumerate() {
                match parse_text_instruction(raw_line) {
                    Ok(Some(instr)) => asm.add_instruction(instr),
                    Ok(None) => {} // blank / comment / directive
                    Err(e) => {
                        return Err(format!(
                            "load-asm: parse error in '{}' at line {}: {}",
                            filename,
                            line_no + 1,
                            e
                        ));
                    }
                }
            }

            // Assemble to machine code.
            let code = asm
                .assemble()
                .map_err(|e| format!("load-asm: assembly error: {}", e))?;

            // Allocate executable memory, write the code, flip permissions.
            let mut jit = JitMemory::new(code.len())
                .map_err(|e| format!("load-asm: JIT allocation failed: {}", e))?;
            jit.write(&code)
                .map_err(|e| format!("load-asm: JIT write failed: {}", e))?;
            jit.make_executable()
                .map_err(|e| format!("load-asm: JIT mprotect failed: {}", e))?;

            // Execute and return RAX as a Lisp Number.
            let result = unsafe {
                let f = jit
                    .as_fn()
                    .map_err(|e| format!("load-asm: JIT fn pointer failed: {}", e))?;
                f()
            };
            Ok(Expr::Int(result as i64))
        })),
    );
}

pub fn register_load_asm_parallel(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "load-asm-parallel".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err(
                    "load-asm-parallel: expects exactly 1 argument (list of filename strings)"
                        .into(),
                );
            }

            let Expr::List(filename_exprs) = &args[0] else {
                return Err(
                    "load-asm-parallel: argument must be a list of filename strings".into(),
                );
            };

            // Collect filenames eagerly so we can move them into threads.
            let filenames: Vec<String> = filename_exprs
                .iter()
                .map(|e| match e {
                    Expr::Str(s) | Expr::Symbol(s) => Ok(s.clone()),
                    other => Err(format!(
                        "load-asm-parallel: filename must be a string, got {:?}",
                        other
                    )),
                })
                .collect::<Result<_, _>>()?;

            // Spawn one thread per file.  Each thread: read → parse → assemble → JIT-exec.
            // `JitMemory: Send` so it can be created and executed on the worker thread.
            let handles: Vec<thread::JoinHandle<Result<u64, String>>> = filenames
                .into_iter()
                .map(|filename| {
                    thread::spawn(move || -> Result<u64, String> {
                        // ── read ──────────────────────────────────────────
                        let source = fs::read_to_string(&filename).map_err(|e| {
                            format!("load-asm-parallel: cannot read '{}': {}", filename, e)
                        })?;

                        // ── parse ─────────────────────────────────────────
                        let mut asm = Assembler::new();
                        for (line_no, raw_line) in source.lines().enumerate() {
                            match parse_text_instruction(raw_line) {
                                Ok(Some(instr)) => asm.add_instruction(instr),
                                Ok(None) => {}
                                Err(e) => {
                                    return Err(format!(
                                        "load-asm-parallel: parse error in '{}' at line {}: {}",
                                        filename,
                                        line_no + 1,
                                        e
                                    ));
                                }
                            }
                        }

                        // ── assemble ──────────────────────────────────────
                        let code = asm.assemble().map_err(|e| {
                            format!("load-asm-parallel: assembly error in '{}': {}", filename, e)
                        })?;

                        // ── JIT: write → mprotect → execute ───────────────
                        // JitMemory is created, owned, and executed entirely
                        // on this worker thread — no sharing required.
                        let mut jit = JitMemory::new(code.len()).map_err(|e| {
                            format!(
                                "load-asm-parallel: JIT allocation failed for '{}': {}",
                                filename, e
                            )
                        })?;
                        jit.write(&code).map_err(|e| {
                            format!(
                                "load-asm-parallel: JIT write failed for '{}': {}",
                                filename, e
                            )
                        })?;
                        jit.make_executable().map_err(|e| {
                            format!(
                                "load-asm-parallel: JIT mprotect failed for '{}': {}",
                                filename, e
                            )
                        })?;

                        let result = unsafe {
                            let f = jit.as_fn().map_err(|e| {
                                format!(
                                    "load-asm-parallel: JIT fn pointer failed for '{}': {}",
                                    filename, e
                                )
                            })?;
                            f()
                        };
                        Ok(result)
                    })
                })
                .collect();

            // Join all threads, preserving order, propagating first error.
            let results: Vec<Expr> = handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .map_err(|_| "load-asm-parallel: worker thread panicked".to_string())?
                        .map(|n| Expr::Int(n as i64))
                })
                .collect::<Result<_, _>>()?;

            Ok(Expr::List(results))
        })),
    );
}
