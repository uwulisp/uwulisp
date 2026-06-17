### utils
```rust
// ─────────────────────────────────────────────────────────────────────────────
// I/O — user input
// ─────────────────────────────────────────────────────────────────────────────

/// Registers interactive-input builtins.
///
/// `(read-line)`            — reads one line from stdin, returns `Expr::Str`.
/// `(read-line prompt)`     — prints `prompt` (no newline) first, then reads.
pub fn register_io(env: &Env) {
    // (read-line [prompt])
    env_set(
        env,
        "read-line".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() > 1 {
                return Err("read-line: expects 0 or 1 arguments".into());
            }
            // Optional prompt
            if let Some(p) = args.first() {
                print!("{}", display_str(p));
                io::stdout().flush().map_err(|e| e.to_string())?;
            }
            let stdin = io::stdin();
            let mut line = String::new();
            stdin
                .lock()
                .read_line(&mut line)
                .map_err(|e| format!("read-line: {}", e))?;
            // Strip the trailing newline to match Scheme behaviour.
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(Expr::Str(line))
        })),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// File I/O
// ─────────────────────────────────────────────────────────────────────────────

/// Registers file-system builtins.
///
/// `(file-read   path)`          — read whole file, return `Expr::Str`.
/// `(file-write  path content)`  — overwrite file with string content.
/// `(file-append path content)`  — append string content to file.
/// `(file-exists? path)`         — return 1.0 / 0.0.
/// `(file-delete  path)`         — delete file; returns `()`.
pub fn register_file(env: &Env) {
    // (file-read path)
    env_set(
        env,
        "file-read".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("file-read: expects exactly 1 argument".into());
            }
            let path = str_arg(&args[0])?;
            std::fs::read_to_string(path)
                .map(Expr::Str)
                .map_err(|e| format!("file-read: {}: {}", path, e))
        })),
    );

    // (file-write path content)
    env_set(
        env,
        "file-write".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 2 {
                return Err("file-write: expects (file-write path content)".into());
            }
            let path    = str_arg(&args[0])?;
            let content = str_arg(&args[1])?;
            std::fs::write(path, content)
                .map(|_| Expr::List(vec![]))
                .map_err(|e| format!("file-write: {}: {}", path, e))
        })),
    );

    // (file-append path content)
    env_set(
        env,
        "file-append".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 2 {
                return Err("file-append: expects (file-append path content)".into());
            }
            let path    = str_arg(&args[0])?;
            let content = str_arg(&args[1])?;
            use std::io::Write as _;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
                .map(|_| Expr::List(vec![]))
                .map_err(|e| format!("file-append: {}: {}", path, e))
        })),
    );

    // (file-exists? path)
    env_set(
        env,
        "file-exists?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("file-exists?: expects exactly 1 argument".into());
            }
            let path = str_arg(&args[0])?;
            Ok(Expr::Number(if std::path::Path::new(path).exists() {
                1.0
            } else {
                0.0
            }))
        })),
    );

    // (file-delete path)
    env_set(
        env,
        "file-delete".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("file-delete: expects exactly 1 argument".into());
            }
            let path = str_arg(&args[0])?;
            std::fs::remove_file(path)
                .map(|_| Expr::List(vec![]))
                .map_err(|e| format!("file-delete: {}: {}", path, e))
        })),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// OS — shell execution
// ─────────────────────────────────────────────────────────────────────────────

/// Registers OS / process builtins.
///
/// `(shell cmd)`                    — run `cmd` via `sh -c`, block until done,
///                                    return captured stdout as `Expr::Str`.
/// `(shell-status cmd)`             — same but return exit code as `Expr::Number`.
pub fn register_os(env: &Env) {
    use std::process::Command;

    // (shell cmd-string) → Expr::Str  (captured stdout)
    env_set(
        env,
        "shell".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("shell: expects exactly 1 argument".into());
            }
            let cmd = str_arg(&args[0])?;
            let out = Command::new("sh")
                .args(["-c", cmd])
                .output()
                .map_err(|e| format!("shell: {}", e))?;
            // Combine stdout; ignore stderr (available via shell redirection if needed).
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            Ok(Expr::Str(stdout))
        })),
    );

    // (shell-status cmd-string) → Expr::Number  (exit code)
    env_set(
        env,
        "shell-status".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("shell-status: expects exactly 1 argument".into());
            }
            let cmd = str_arg(&args[0])?;
            let status = Command::new("sh")
                .args(["-c", cmd])
                .status()
                .map_err(|e| format!("shell-status: {}", e))?;
            Ok(Expr::Number(status.code().unwrap_or(-1) as f64))
        })),
    );
}
```

### asm
```rust
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
pub fn register_load_asm(env: &Env) {
    env_set(
        env,
        "load-asm".into(),
        Expr::Func(Rc::new(|args| {
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
                    ))
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
                        ))
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
            Ok(Expr::Number(result as f64))
        })),
    );
}
```