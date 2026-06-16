mod builtins;
mod compiler;
mod env;
mod eval;
mod expr;
mod macros;
mod reader;
mod typechecker;
mod tinyasm;

use compiler::compile;
use env::Env;
use eval::eval;
use expr::LexEnv;
use reader::parse_all;
use std::io::{self, BufRead, IsTerminal, Write};
use std::rc::Rc;
use typechecker::{typecheck_toplevel, TyGlobal};

// ---------------------------------------------------------------------------
// Multiline accumulator
//
// Buffers input lines until a complete top-level s-expression has been
// received (i.e. paren depth returns to zero). A single instance is shared
// across all three execution modes (file, interactive REPL, batch stdin),
// eliminating the previously copy-pasted loop bodies.
// ---------------------------------------------------------------------------

struct LineAccumulator {
    buf: String,
    depth: i32,
}

impl LineAccumulator {
    fn new() -> Self {
        Self {
            buf: String::new(),
            depth: 0,
        }
    }

    /// Push one line of input. Returns `Some(expr_src)` when the accumulated
    /// text forms a complete (balanced) expression, `None` otherwise.
    fn push(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            return None;
        }
        self.depth += paren_delta(trimmed);
        if !self.buf.is_empty() {
            self.buf.push('\n');
        }
        self.buf.push_str(trimmed);
        if self.depth <= 0 {
            self.depth = 0;
            Some(self.buf.split_off(0)) // take and clear in one step
        } else {
            None
        }
    }

    /// Flush any partial buffer (used at EOF to handle trailing atoms).
    fn flush(&mut self) -> Option<String> {
        if self.buf.trim().is_empty() {
            None
        } else {
            self.depth = 0;
            Some(self.buf.split_off(0))
        }
    }

    /// True while we are inside an unfinished expression (for REPL prompts).
    fn is_continuation(&self) -> bool {
        self.depth > 0
    }
}

// ---------------------------------------------------------------------------
// Paren-depth counter (unchanged from original)
// ---------------------------------------------------------------------------

/// Returns the change in open-paren depth contributed by `line`.
/// Counts `(` as +1 and `)` as -1, ignoring characters inside strings.
fn paren_delta(line: &str) -> i32 {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escape = false;
    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_str {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_str = !in_str;
            continue;
        }
        if in_str {
            continue;
        }
        if ch == ';' {
            break; // line comment
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
    }
    depth
}

// ---------------------------------------------------------------------------
// Run one source string through compile → typecheck → eval.
//
// Returns `true` if any error occurred so callers can set a non-zero exit
// code. Errors are printed to stderr; successful results to stdout.
// ---------------------------------------------------------------------------

fn run(src: &str, env: &Env, ty_global: &mut TyGlobal) -> bool {
    let mut had_error = false;

    let exprs = match parse_all(src) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{}\n  => Parse error: {}\n", src, err);
            return true;
        }
    };

    let lex_env = Rc::new(LexEnv::Empty);

    for e in exprs {
        let mut dummy_names = Vec::new();
        let compiled = match compile(&e, &mut dummy_names) {
            Ok(c) => c,
            Err(err) => {
                eprintln!("{}\n  => Compile error: {}\n", src, err);
                had_error = true;
                continue;
            }
        };

        let ty = match typecheck_toplevel(&compiled, env, ty_global) {
            Ok(t) => t,
            Err(err) => {
                eprintln!("{}\n  => Type error: {}\n", src, err);
                had_error = true;
                continue;
            }
        };

        match eval(&compiled, env, &lex_env) {
            Ok(result) => println!("{}\n  => {:?}  :  {:?}\n", src, result, ty),
            Err(err) => {
                eprintln!("{}\n  => Runtime error: {}\n", src, err);
                had_error = true;
            }
        }
    }

    had_error
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<(), io::Error> {
    let env = builtins::global_env();
    let mut ty_global = TyGlobal::new();
    let mut had_error = false;

    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        // -------------------------------------------------------------------
        // --test: run the built-in test suite
        // -------------------------------------------------------------------
        Some("--test") => {
            let exprs = [
                // ----- arithmetic --------------------------------------------------
                "(define square (lambda (x) (* x x)))",
                "(square 5)",
                "(define fact (lambda (n) (if (< n 1) 1 (* n (fact (- n 1))))))",
                "(fact 10)",
                "(let ((a 3) (b 4)) (+ (* a a) (* b b)))",
                // ----- macros -------------------------------------------------------
                "(defmacro unless (cond then) (list 'if (list 'not cond) then 0))",
                "(unless 0 (+ 1 2))",
                "(unless 1 (+ 1 2))",
                "(defmacro my-or (a b) (list 'if a a b))",
                "(my-or 0 42)",
                "(my-or 7 42)",
                // ----- quasiquote --------------------------------------------------
                "(define x 10)",
                "(quasiquote (a b (unquote x)))",
                "(define lst (list 1 2 3))",
                "(quasiquote (start (unquote-splicing lst) end))",
                "'(1 2 3)",
                "(car '(1 2 3))",
                "(cdr '(1 2 3))",
                // ----- interval / path (cubical) ------------------------------------
                "i0",
                "i1",
                "(define interp (path (i) (+ (* (- 1 i) 1) (* i 5))))",
                "(papply interp i0)",
                "(papply interp i1)",
                "(papply interp 0.5)",
                "(define rp (refl 42))",
                "(papply rp i0)",
                "(papply rp i1)",
                "(papply rp 0.3)",
                "(path? interp)",
                "(path? rp)",
                "(path? 42)",
                // ----- pi types -----------------------------------------------------
                "(define arr (pi (x) 0 1))",
                "(pi? arr)",
                "(pi? 42)",
                "(path? arr)",
                "(define vec-type (pi (n) 0 (* n n)))",
                "(piapply vec-type 3)",
                "(piapply vec-type 5)",
                "(define type-path (path (i) (pi (x) 0 (* x (+ i 1)))))",
                "(piapply (papply type-path i0) 4)",
                "(piapply (papply type-path i1) 4)",
                // ----- sigma types --------------------------------------------------
                "(define pair-type (sigma (x) 0 1))",
                "(sigma? pair-type)",
                "(sigma? 42)",
                "(define dyn-vec (sigma (len) 0 (* len 10)))",
                "(sigmacod dyn-vec 3)",
                "(sigmacod dyn-vec 5)",
                // ----- glue types ---------------------------------------------------
                "(define double (lambda (x) (* x 2)))",
                "(define gt (glue-type 0 double))",
                "(glue-type? gt)",
                "(glue-type? 42)",
                "(define gv (glue 21 double))",
                "(glue? gv)",
                "(glue? 42)",
                "(unglue gv)",
                "(define gpath (path (i) (glue (* i 10) double)))",
                "(unglue (papply gpath 0.0))",
                "(unglue (papply gpath 0.5))",
                "(unglue (papply gpath 1.0))",
                // ----- sentinel symbols ---------------------------------------------
                "__Num__",
                "__Type__",
                "__Any__",
                "__GlueType__",
                "(__Path__ __Num__)",
                "(__Glue__ __Num__)",
                "(define arr2 (pi (x) __Num__ __Num__))",
                "(define vec-type2 (pi (n) __Num__ (* n n)))",
                "(piapply vec-type2 3)",
                "(print 'hello_world)",
            ];

            for src in &exprs {
                had_error |= run(src, &env, &mut ty_global);
            }
        }

        // -------------------------------------------------------------------
        // <path>: execute a source file
        // -------------------------------------------------------------------
        Some(path) => {
            let file = std::fs::File::open(path)?;
            let reader = io::BufReader::new(file);
            had_error |= run_lines(reader.lines(), &env, &mut ty_global);
        }

        // -------------------------------------------------------------------
        // No argument: interactive REPL or batch stdin
        // -------------------------------------------------------------------
        None => {
            let stdin = io::stdin();
            if stdin.is_terminal() {
                had_error |= run_repl(stdin, &env, &mut ty_global)?;
            } else {
                had_error |= run_lines(io::BufReader::new(stdin).lines(), &env, &mut ty_global);
            }
        }
    }

    if had_error {
        std::process::exit(1);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared line-driven execution helpers
// ---------------------------------------------------------------------------

/// Drives execution from any iterator of `io::Result<String>` lines (file or
/// batch stdin).  A single `LineAccumulator` provides the multiline buffering.
fn run_lines(
    lines: impl Iterator<Item = io::Result<String>>,
    env: &Env,
    ty_global: &mut TyGlobal,
) -> bool {
    let mut acc = LineAccumulator::new();
    let mut had_error = false;

    for line in lines {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Read error: {}", e);
                return true;
            }
        };
        if let Some(src) = acc.push(&line) {
            had_error |= run(&src, env, ty_global);
        }
    }

    // Handle a trailing atom or expression with no final newline.
    if let Some(src) = acc.flush() {
        had_error |= run(&src, env, ty_global);
    }

    had_error
}

/// Interactive REPL with multiline-aware prompts.
fn run_repl(stdin: io::Stdin, env: &Env, ty_global: &mut TyGlobal) -> io::Result<bool> {
    println!("uwulisp interactive REPL. Press Ctrl-D or type 'exit' to exit.");

    let mut acc = LineAccumulator::new();
    let mut had_error = false;
    let mut line_buf = String::new();

    loop {
        // Show a continuation prompt while inside an unfinished expression.
        print!("{}", if acc.is_continuation() { "    ...> " } else { "uwu> " });
        io::stdout().flush()?;

        line_buf.clear();
        let bytes_read = stdin.lock().read_line(&mut line_buf)?;
        if bytes_read == 0 {
            // EOF — flush any partial input and exit.
            if let Some(src) = acc.flush() {
                had_error |= run(&src, env, ty_global);
            }
            break;
        }

        let trimmed = line_buf.trim();
        if !acc.is_continuation() && trimmed == "exit" {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }

        if let Some(src) = acc.push(trimmed) {
            had_error |= run(&src, env, ty_global);
        }
    }

    Ok(had_error)
}