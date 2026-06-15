mod builtins;
mod compiler;
mod env;
mod eval;
mod expr;
mod macros;
mod reader;
mod typechecker;

use env::Env;
use eval::eval;
use expr::LexEnv;
use reader::parse_all;
use compiler::compile;
use typechecker::{typecheck_toplevel, TyGlobal};
use std::rc::Rc;

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
                            Ok(ty) => {
                                match eval(&compiled, env, &lex_env) {
                                    Ok(result) => println!("{}\n  => {:?}  :  {:?}\n", src, result, ty),
                                    Err(err)   => println!("{}\n  => Runtime error: {}\n", src, err),
                                }
                            }
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

fn main() {
    let env = builtins::global_env();
    let mut ty_global: TyGlobal = TyGlobal::new();

    let exprs = vec![
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

        // ----- pi types -----------------------------------------------------
        "(define arr (pi (x) 0 1))",
        "(pi? arr)",
        "(pi? 42)",
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
    ];

    for src in exprs {
        run(src, &env, &mut ty_global);
    }
}