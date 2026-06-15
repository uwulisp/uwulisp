mod builtins;
mod compiler;
mod env;
mod eval;
mod expr;
mod macros;
mod reader;
mod typechecker;

use compiler::compile;
use env::Env;
use eval::eval;
use expr::LexEnv;
use reader::parse_all;
use std::io::IsTerminal;
use std::rc::Rc;
use typechecker::{TyGlobal, typecheck_toplevel};

/// Returns the change in open-paren depth contributed by `line`.
/// Counts `(` as +1 and `)` as -1, ignoring characters inside strings
/// (a simple approximation sufficient for a Lisp reader).
fn paren_delta(line: &str) -> i32 {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut escape = false;
    for ch in line.chars() {
        if escape { escape = false; continue; }
        if ch == '\\' && in_str { escape = true; continue; }
        if ch == '"' { in_str = !in_str; continue; }
        if in_str { continue; }
        if ch == ';' { break; } // line comment
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Parses, type-checks, and evaluates each top-level expression in `src`.
fn run(src: &str, env: &Env, ty_global: &mut TyGlobal) {
    match parse_all(src) {
        Ok(exprs) => {
            let lex_env = Rc::new(LexEnv::Empty);
            for e in exprs {
                let mut dummy_names = Vec::new();
                match compile(&e, &mut dummy_names) {
                    Ok(compiled) => {
                        // --- Type-check before evaluating ---
                        match typecheck_toplevel(&compiled, env, ty_global) {
                            Ok(ty) => match eval(&compiled, env, &lex_env) {
                                Ok(result) => println!("{}\n  => {:?}  :  {:?}\n", src, result, ty),
                                Err(err) => println!("{}\n  => Runtime error: {}\n", src, err),
                            },
                            Err(type_err) => {
                                println!("{}\n  => Type error: {}\n", src, type_err);
                            }
                        }
                    }
                    Err(err) => println!("{}\n  => Compile error: {}\n", src, err),
                }
            }
        }
        Err(err) => println!("{}\n  => Parse error: {}\n", src, err),
    }
}

fn main() -> Result<(), std::io::Error> {
    // 1. Initialize global states exactly once
    let env = builtins::global_env();
    let mut ty_global = TyGlobal::new();
    
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        if args[1] == "--test" {
            // --- run test suite ---
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
                // print
                "(print 'hello_world)",
            ];

            for src in &exprs {
                run(src, &env, &mut ty_global);
            }
        } else {
            // --- File Execution Mode ---
            let file = std::fs::File::open(&args[1])?;
            let mut reader = std::io::BufReader::new(file);
            let mut line_buf = String::new();
            let mut accumulator = String::new();
            let mut depth: i32 = 0;

            while std::io::BufRead::read_line(&mut reader, &mut line_buf)? > 0 {
                let trimmed = line_buf.trim_end();
                if trimmed.is_empty() {
                    line_buf.clear();
                    continue;
                }
                depth += paren_delta(trimmed);
                if !accumulator.is_empty() {
                    accumulator.push('\n');
                }
                accumulator.push_str(trimmed);
                if depth <= 0 {
                    run(&accumulator, &env, &mut ty_global);
                    accumulator.clear();
                    depth = 0;
                }
                line_buf.clear();
            }
            // Run any remaining input (e.g. a trailing atom with no parens)
            if !accumulator.trim().is_empty() {
                run(&accumulator, &env, &mut ty_global);
            }
        }
    } else {
        // --- Stdio Mode: REPL or Batch ---
        use std::io::{stdin, stdout, Write, BufRead};
        if stdin().is_terminal() {
            // --- Interactive REPL (multiline-aware) ---
            println!("uwulisp interactive REPL. Press Ctrl-D or type 'exit' to exit.");
            let mut line_buf = String::new();
            let mut accumulator = String::new();
            let mut depth: i32 = 0;
            loop {
                if depth <= 0 {
                    print!("uwulisp> ");
                } else {
                    print!("    ...> ");
                }
                stdout().flush()?;
                line_buf.clear();
                let bytes_read = stdin().read_line(&mut line_buf)?;
                if bytes_read == 0 {
                    // EOF — flush whatever we have
                    if !accumulator.trim().is_empty() {
                        run(&accumulator, &env, &mut ty_global);
                    }
                    break;
                }
                let trimmed = line_buf.trim();
                if depth <= 0 && trimmed == "exit" {
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }
                depth += paren_delta(trimmed);
                if !accumulator.is_empty() {
                    accumulator.push('\n');
                }
                accumulator.push_str(trimmed);
                if depth <= 0 {
                    run(&accumulator, &env, &mut ty_global);
                    accumulator.clear();
                    depth = 0;
                }
            }
        } else {
            // --- Batch stdin (multiline-aware) ---
            let mut reader = std::io::BufReader::new(stdin());
            let mut line_buf = String::new();
            let mut accumulator = String::new();
            let mut depth: i32 = 0;
            while reader.read_line(&mut line_buf)? > 0 {
                let trimmed = line_buf.trim_end();
                if !trimmed.is_empty() {
                    depth += paren_delta(trimmed);
                    if !accumulator.is_empty() {
                        accumulator.push('\n');
                    }
                    accumulator.push_str(trimmed);
                    if depth <= 0 {
                        run(&accumulator, &env, &mut ty_global);
                        accumulator.clear();
                        depth = 0;
                    }
                }
                line_buf.clear();
            }
            if !accumulator.trim().is_empty() {
                run(&accumulator, &env, &mut ty_global);
            }
        }
    }

    Ok(())
}