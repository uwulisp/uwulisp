use crate::env::{env_get, env_set, Env};
use crate::expr::{is_truthy, is_sentinel_symbol, Expr, LexEnv};
use crate::macros::{eval_quasiquote, expand_macro};
use crate::compiler::compile;
use crate::reader::parse_params;
use std::rc::Rc;

/// Evaluates an expression in the given environment.
///
/// The main dispatch loop is structured for tail-call optimisation (TCO):
/// instead of recursing for `if`, `begin`, `let`, `letrec`, and direct lambda
/// application, the loop variables `expr` and `lex_env` are updated in-place
/// and execution continues at the top of the loop.  This means arbitrarily
/// deep mutual recursion and iteration never consume extra Rust stack frames.
pub fn eval(expr: &Expr, env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    // Owned working copies so we can rebind them for tail calls.
    let mut expr: Expr = expr.clone();
    let mut lex_env: Rc<LexEnv> = lex_env.clone();

    'tco: loop {
        match &expr {
            // ---------------------------------------------------------------
            // Self-evaluating atoms
            // ---------------------------------------------------------------
            Expr::Number(_) => return Ok(expr),

            Expr::Symbol(s) => {
                return if is_sentinel_symbol(s) {
                    Ok(expr)
                } else {
                    env_get(env, s)
                };
            }

            Expr::Index(i) => {
                return lex_env
                    .get(*i)
                    .ok_or_else(|| format!("unbound index {}", i));
            }

            // Already-reduced values are returned as-is.
            Expr::Func(_)
            | Expr::Lambda(..)
            | Expr::Macro(..)
            | Expr::Path(..)
            | Expr::Pi(..)
            | Expr::Sigma(..)
            | Expr::GlueType(..)
            | Expr::Glue(..) => return Ok(expr),

            // ---------------------------------------------------------------
            // Lists: special forms and function application
            // ---------------------------------------------------------------
            Expr::List(list) => {
                if list.is_empty() {
                    return Ok(Expr::List(vec![]));
                }

                if let Expr::Symbol(op) = &list[0] {
                    match op.as_str() {
                        // ---------------------------------------------------
                        // Non-tail special forms (return immediately)
                        // ---------------------------------------------------
                        "quote" => return Ok(list[1].clone()),

                        "quasiquote" => {
                            return eval_quasiquote(&list[1], env, &lex_env, 1);
                        }

                        "unquote" => return Err("unquote outside quasiquote".into()),

                        "define" => {
                            return if let Expr::Symbol(name) = &list[1] {
                                let val = eval(&list[2], env, &lex_env)?;
                                env_set(env, name.clone(), val.clone());
                                Ok(val)
                            } else {
                                Err("invalid define: expected (define <symbol> <expr>)".into())
                            };
                        }

                        "lambda" => {
                            return if let Expr::Number(arity) = &list[1] {
                                Ok(Expr::Lambda(
                                    *arity as usize,
                                    Box::new(list[2].clone()),
                                    lex_env.clone(),
                                ))
                            } else {
                                Err("lambda core: expected arity".into())
                            };
                        }

                        "defmacro" => return eval_defmacro(list, env, &lex_env),

                        "funext"   => return eval_funext(list, env, &lex_env),
                        "path"     => return eval_path(list, env, &lex_env),
                        "papply"   => return eval_papply(list, env, &lex_env),
                        "pi"       => return eval_pi(list, env, &lex_env),
                        "piapply"  => return eval_piapply(list, env, &lex_env),
                        "sigma"    => return eval_sigma(list, env, &lex_env),
                        "sigmacod" => return eval_sigmacod(list, env, &lex_env),
                        "glue-type" => return eval_glue_type(list, env, &lex_env),
                        "glue"     => return eval_glue(list, env, &lex_env),
                        "unglue"   => return eval_unglue(list, env, &lex_env),

                        "__Path__" => {
                            if list.len() != 2 {
                                return Err("__Path__: expected 1 argument".into());
                            }
                            let dom = eval(&list[1], env, &lex_env)?;
                            return Ok(Expr::List(vec![
                                Expr::Symbol("__Path__".into()),
                                dom,
                            ]));
                        }
                        "__Glue__" => {
                            if list.len() != 2 {
                                return Err("__Glue__: expected 1 argument".into());
                            }
                            let base = eval(&list[1], env, &lex_env)?;
                            return Ok(Expr::List(vec![
                                Expr::Symbol("__Glue__".into()),
                                base,
                            ]));
                        }

                        // ---------------------------------------------------
                        // TCO special forms — update expr/lex_env and loop
                        // ---------------------------------------------------

                        // (if cond then [else])
                        // Both branches are in tail position.
                        "if" => {
                            let cond = eval(&list[1], env, &lex_env)?;
                            // Clone the chosen branch before dropping the borrow
                            // of `list` (and transitively `expr`) so that Rust's
                            // NLL borrow checker is happy with the reassignment.
                            let next = if is_truthy(&cond) {
                                list[2].clone()
                            } else if list.len() > 3 {
                                list[3].clone()
                            } else {
                                return Ok(Expr::List(vec![]));
                            };
                            expr = next;          // rebind — `list` borrow ends here
                            continue 'tco;
                        }

                        // (begin e1 … en) — only en is in tail position.
                        "begin" => {
                            if list.len() < 2 {
                                return Ok(Expr::List(vec![]));
                            }
                            // Evaluate all but the last expression for side effects.
                            for e in &list[1..list.len() - 1] {
                                eval(e, env, &lex_env)?;
                            }
                            let next = list.last().unwrap().clone();
                            expr = next;
                            continue 'tco;
                        }

                        // (let ((name expr)…) body…) — last body is tail position.
                        "let" => {
                            let mut new_lex = lex_env.clone();
                            if let Expr::List(bindings) = &list[1] {
                                for b in bindings {
                                    if let Expr::List(pair) = b {
                                        let val = eval(&pair[1], env, &new_lex)?;
                                        new_lex = Rc::new(LexEnv::Node(val, new_lex));
                                    }
                                }
                            }
                            // Evaluate all but the last body expression.
                            for e in &list[2..list.len() - 1] {
                                eval(e, env, &new_lex)?;
                            }
                            let next = list.last().unwrap().clone();
                            expr = next;
                            lex_env = new_lex;
                            continue 'tco;
                        }

                        // (letrec ((name expr)…) body…) — last body is tail position.
                        "letrec" => {
                            let final_lex = build_letrec_env(list, env, &lex_env)?;
                            for e in &list[2..list.len() - 1] {
                                eval(e, env, &final_lex)?;
                            }
                            let next = list.last().unwrap().clone();
                            expr = next;
                            lex_env = final_lex;
                            continue 'tco;
                        }

                        _ => {
                            // Named macro: expand, compile, then tail-call the result.
                            if let Ok(Expr::Macro(params, body)) = env_get(env, op) {
                                let expanded = expand_macro(&params, &body, &list[1..])?;
                                let mut dummy = Vec::new();
                                let compiled = compile(&expanded, &mut dummy)?;
                                expr = compiled;
                                continue 'tco;
                            }
                        }
                    }
                }

                // -----------------------------------------------------------
                // General function application
                //
                // Evaluate the operator and all operands eagerly, then
                // dispatch on the operator's value.
                // -----------------------------------------------------------
                let func = eval(&list[0], env, &lex_env)?;
                let args: Result<Vec<Expr>, String> =
                    list[1..].iter().map(|e| eval(e, env, &lex_env)).collect();
                let args = args?;

                // Macro value (rare: an operator expression evaluated to a macro).
                if let Expr::Macro(ref params, ref body) = func {
                    let expanded = expand_macro(params, body, &args)?;
                    let mut dummy = Vec::new();
                    let compiled = compile(&expanded, &mut dummy)?;
                    expr = compiled;
                    continue 'tco;
                }

                // Beta-reduce a lambda in tail position — the key TCO case.
                // All other function types fall through to `apply`.
                if let Expr::Lambda(arity, body, penv) = func {
                    let n = args.len();
                    match n.cmp(&arity) {
                        // Exact-arity application: bind args and loop (TCO).
                        std::cmp::Ordering::Equal => {
                            let new_lex = bind_args(args, penv);
                            // The borrow of `list` (and `expr`) via the outer
                            // `match &expr` ends here because `list` is not used
                            // below this point; NLL lets us safely reassign.
                            expr = *body;
                            lex_env = new_lex;
                            continue 'tco;
                        }

                        // Under-application: capture supplied args in the closure
                        // and return a lambda that waits for the rest.
                        std::cmp::Ordering::Less => {
                            let new_lex = bind_args(args, penv);
                            return Ok(Expr::Lambda(arity - n, body, new_lex));
                        }

                        // Over-application: apply the first `arity` args, then
                        // apply the resulting function to the remaining ones.
                        // This handles the common pattern of a function that
                        // returns a lambda (e.g. curried builders).
                        std::cmp::Ordering::Greater => {
                            let (first, rest) = args.split_at(arity);
                            let new_lex = bind_args(first.to_vec(), penv);
                            let inner_result = eval(&body, env, &new_lex)?;
                            return apply(inner_result, rest, env);
                        }
                    }
                }

                // Builtin or unknown — delegate to `apply` (no TCO opportunity).
                return apply(func, &args, env);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Bind a vector of already-evaluated arguments into a new lexical environment.
//
// Arguments are pushed left-to-right onto `penv`, matching the order the
// compiler assigns De Bruijn indices (last param → index 0, first → index N-1).
// ---------------------------------------------------------------------------
#[inline]
fn bind_args(args: Vec<Expr>, penv: Rc<LexEnv>) -> Rc<LexEnv> {
    let mut env = penv;
    for arg in args {
        env = Rc::new(LexEnv::Node(arg, env));
    }
    env
}

// ---------------------------------------------------------------------------
// Applies a function (builtin or lambda) to already-evaluated arguments.
//
// Compared with the old version this now supports:
//
//   • Partial application (n < arity): captures supplied args in the closure
//     and returns a new lambda expecting the remaining arguments.
//
//   • Over-application (n > arity): applies the first `arity` args, then
//     recursively applies the result to the remaining arguments.  This lets
//     callers write `(((f a) b) c)` as `(f a b c)` for any curried `f`.
// ---------------------------------------------------------------------------
pub fn apply(func: Expr, args: &[Expr], env: &Env) -> Result<Expr, String> {
    // Loop so over-application can tail-call without extra stack frames.
    let mut func = func;
    let mut remaining: &[Expr] = args;
    // Owned storage for the "rest" slice in the over-application case.
    let mut owned_rest: Vec<Expr>;

    loop {
        match func {
            Expr::Func(f) => return f(remaining),

            Expr::Lambda(arity, body, penv) => {
                let n = remaining.len();
                match n.cmp(&arity) {
                    std::cmp::Ordering::Equal => {
                        // Standard beta reduction.
                        let new_lex = bind_args(remaining.to_vec(), penv);
                        return eval(&body, env, &new_lex);
                    }

                    std::cmp::Ordering::Less => {
                        // Partial application: capture what we have.
                        let new_lex = bind_args(remaining.to_vec(), penv);
                        return Ok(Expr::Lambda(arity - n, body, new_lex));
                    }

                    std::cmp::Ordering::Greater => {
                        // Over-application: saturate this lambda, then continue
                        // applying the result to the leftover arguments.
                        let (first, rest) = remaining.split_at(arity);
                        let new_lex = bind_args(first.to_vec(), penv);
                        func = eval(&body, env, &new_lex)?;
                        // Keep rest alive for the next iteration.
                        owned_rest = rest.to_vec();
                        remaining = &owned_rest;
                        // loop — no extra stack frame
                    }
                }
            }

            other => {
                return Err(format!(
                    "not a function: {:?} (applied to {} argument(s))",
                    other,
                    remaining.len()
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// letrec env builder (extracted from eval so the TCO loop can call it cleanly)
//
// Returns the fully-resolved lexical environment for the letrec body.
// Implements the standard two-pass back-patch trick:
//   1. Extend lex_env with placeholders so forward-referencing lambdas compile.
//   2. Evaluate every RHS in the placeholder env.
//   3. Rebuild with real values; re-close any captured lambdas.
// ---------------------------------------------------------------------------
fn build_letrec_env(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
) -> Result<Rc<LexEnv>, String> {
    if list.len() < 3 {
        return Err("letrec: expected bindings and a body".into());
    }
    let bindings = match &list[1] {
        Expr::List(b) => b,
        _ => return Err("letrec: bindings must be a list".into()),
    };
    let n = bindings.len();

    // Pass 1: placeholder env so De Bruijn indices resolve during evaluation.
    let placeholder = Expr::Symbol("__letrec_placeholder__".into());
    let mut placeholder_env = lex_env.clone();
    for _ in 0..n {
        placeholder_env = Rc::new(LexEnv::Node(placeholder.clone(), placeholder_env));
    }

    // Pass 2: evaluate every RHS in the placeholder env.
    let mut vals: Vec<Expr> = Vec::with_capacity(n);
    for b in bindings {
        match b {
            Expr::List(pair) if pair.len() >= 2 => {
                vals.push(eval(&pair[1], env, &placeholder_env)?);
            }
            _ => return Err("letrec: each binding must be (name expr)".into()),
        }
    }

    // Pass 3: build the real env with concrete values.
    let mut real_env = lex_env.clone();
    for v in &vals {
        real_env = Rc::new(LexEnv::Node(v.clone(), real_env));
    }

    // Re-close any top-level lambdas that closed over the placeholder env,
    // so that recursive calls during execution see the real bindings.
    let mut final_env = lex_env.clone();
    for v in vals {
        let v = match v {
            Expr::Lambda(arity, body, _) => Expr::Lambda(arity, body, real_env.clone()),
            other => other,
        };
        final_env = Rc::new(LexEnv::Node(v, final_env));
    }

    Ok(final_env)
}

// ---------------------------------------------------------------------------
// Special-form helpers (unchanged in behaviour; kept as separate fns for
// readability since they are not in a tail position and return directly)
// ---------------------------------------------------------------------------

/// (path <arity> <body>)
///
/// Constructs a Path value whose body is an expression in which De Bruijn
/// index 0 refers to the interval variable `i ∈ [0,1]`.
///
/// `<arity>` must be the number literal `1.0` (one interval dimension).
/// Multidimensional paths are not supported by this form; compose `path`
/// applications instead.
///
/// The body is stored unevaluated together with the current lexical
/// environment, so that free variables in the body are captured correctly.
/// eval_papply later pushes the concrete interval value at index 0 before
/// evaluating the body.
fn eval_path(list: &[Expr], _env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("path: expected (path <arity> <body>)".into());
    }
    // Validate that the caller supplied arity = 1 (one interval dimension).
    match &list[1] {
        Expr::Number(n) if *n == 1.0 => {}
        other => return Err(format!(
            "path: arity must be 1.0 (one interval dimension), got {:?}",
            other
        )),
    }
    Ok(Expr::Path(Box::new(list[2].clone()), lex_env.clone()))
}

/// (papply p t)
///
/// Applies a path `p : Path A a b` to an interval point `t ∈ [0,1]`,
/// returning the value of the path at that point.
///
/// Interval convention (De Bruijn):
///   When evaluating a non-Func path body the interval value `t` is pushed
///   onto the front of the path's captured lexical environment, so that
///   De Bruijn index 0 inside the body refers to the interval variable.
///   Any values the path closed over at construction time are shifted up
///   by one (index 1, 2, …).
///
/// Func-based bodies (produced by `funext`) receive `t` as their sole
/// argument instead, bypassing De Bruijn indexing entirely.
fn eval_papply(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("papply: expected (papply <path> <interval-point>)".into());
    }
    let p = eval(&list[1], env, lex_env)?;
    let t = eval(&list[2], env, lex_env)?;

    let t_val = match &t {
        Expr::Number(n) => *n,
        other => return Err(format!("papply: interval point must be a number, got {:?}", other)),
    };
    // Accept any point in the closed interval [0, 1].
    // Cubical paths must be evaluable at interior points (e.g. for
    // composition, transport, and hcomp), not just at the endpoints.
    if !(0.0..=1.0).contains(&t_val) {
        return Err(format!(
            "papply: interval point {} out of bounds, expected [0,1]",
            t_val
        ));
    }

    match p {
        Expr::Path(body, penv) => {
            match *body {
                // Func-based path bodies (e.g. from funext) store a closure
                // that expects the interval value as its direct argument,
                // rather than reading it from the lexical environment.
                // Calling eval() on Expr::Func is a no-op (self-evaluating),
                // so we must call the function directly here.
                Expr::Func(f) => f(&[Expr::Number(t_val)]),
                // Ordinary path body: an expression with the interval variable
                // at De Bruijn index 0.  Push the interval value and evaluate.
                other => {
                    let new_env = Rc::new(LexEnv::Node(Expr::Number(t_val), penv));
                    eval(&other, env, &new_env)
                }
            }
        }
        other => Err(format!("papply: not a path: {:?}", other)),
    }
}

/// (pi (x) dom cod)
fn eval_pi(list: &[Expr], _env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    let dom = Box::new(list[1].clone());
    let cod = Box::new(list[2].clone());
    Ok(Expr::Pi(dom, cod, lex_env.clone()))
}

/// (piapply p v)
fn eval_piapply(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("piapply: expected (piapply <pi-type> <value>)".into());
    }
    let p = eval(&list[1], env, lex_env)?;
    let v = eval(&list[2], env, lex_env)?;

    match p {
        Expr::Pi(_dom, cod, penv) => {
            let new_env = Rc::new(LexEnv::Node(v, penv));
            eval(&cod, env, &new_env)
        }
        other => Err(format!("piapply: not a pi-type: {:?}", other)),
    }
}

/// (sigma (x) dom cod)
fn eval_sigma(list: &[Expr], _env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    let dom = Box::new(list[1].clone());
    let cod = Box::new(list[2].clone());
    Ok(Expr::Sigma(dom, cod, lex_env.clone()))
}

/// (sigmacod s v)
fn eval_sigmacod(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("sigmacod: expected (sigmacod <sigma-type> <value>)".into());
    }
    let s = eval(&list[1], env, lex_env)?;
    let v = eval(&list[2], env, lex_env)?;

    match s {
        Expr::Sigma(_dom, cod, penv) => {
            let new_env = Rc::new(LexEnv::Node(v, penv));
            eval(&cod, env, &new_env)
        }
        other => Err(format!("sigmacod: not a sigma-type: {:?}", other)),
    }
}

/// (defmacro name (params…) body)
fn eval_defmacro(list: &[Expr], env: &Env, _lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if let Expr::Symbol(name) = &list[1] {
        let params = parse_params(&list[2])?;
        let mac = Expr::Macro(params, Box::new(list[3].clone()));
        env_set(env, name.clone(), mac.clone());
        Ok(mac)
    } else {
        Err("invalid defmacro: expected <symbol>".into())
    }
}

/// (funext f g p)
///
/// Constructs a path between two functions `f` and `g` given a pointwise
/// homotopy `p : Π x, Path (f x) (g x)`.
///
/// Returns `Expr::Path(Func(…), _)` where the body `Func` accepts the
/// interval value `i` directly (called by the `Expr::Func` arm in
/// `eval_papply`) and returns a function-valued fiber at that point:
///
///   (papply (funext f g p) i0)       =>  a function ≡ f   (every x maps to f x)
///   (papply (funext f g p) i1)       =>  a function ≡ g   (every x maps to g x)
///   ((papply (funext f g p) i0) x)   =>  f x
///   ((papply (funext f g p) i1) x)   =>  g x
fn eval_funext(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 4 {
        return Err("funext: expected (funext f g p)".into());
    }
    let p = eval(&list[3], env, lex_env)?;
    let env_clone = env.clone();

    // The path body is an Expr::Func closure that receives the interval
    // point `i` ∈ [0,1] as its sole argument (called directly by
    // eval_papply — see the Expr::Func arm added there).  It returns
    // another Func representing the function at that fiber, which:
    //   • at i=0 computes f(x)
    //   • at i=1 computes g(x)
    //   • at intermediate i evaluates p(x) at that point
    let p_for_body = p.clone();
    let env_for_body = env_clone.clone();
    let body_func = Expr::Func(Rc::new(move |args: &[Expr]| -> Result<Expr, String> {
        // args[0] is the interval point i ∈ [0,1].
        let i = args[0].clone();
        let p_captured = p_for_body.clone();
        let env_captured = env_for_body.clone();
        // Return a Lambda of arity 1 over x.
        // We use Expr::Func here too so the body doesn't require De Bruijn
        // compilation: the closure captures p and i directly.
        Ok(Expr::Func(Rc::new(move |xargs: &[Expr]| -> Result<Expr, String> {
            if xargs.is_empty() {
                return Err("funext: inner lambda called with no arguments".into());
            }
            let x = xargs[0].clone();
            // Apply the pointwise homotopy p to x, obtaining a Path.
            let px = apply(p_captured.clone(), &[x], &env_captured)?;
            match px {
                Expr::Path(body, penv) => {
                    // Evaluate the path body at interval point i.
                    let new_lex = Rc::new(LexEnv::Node(i.clone(), penv));
                    eval(&body, &env_captured, &new_lex)
                }
                // p x returned a Func-based path body (nested funext, etc.) —
                // call it directly with i.
                Expr::Func(f) => f(&[i.clone()]),
                other => Err(format!(
                    "funext: pointwise homotopy did not return a path, got {:?}",
                    other
                )),
            }
        })))
    }));

    // Wrap body_func in a Path.  eval_papply will detect Expr::Func path
    // bodies and call them with the interval value directly (see eval_papply).
    Ok(Expr::Path(Box::new(body_func), lex_env.clone()))
}

/// (glue-type base equiv)
fn eval_glue_type(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("glue-type: expected (glue-type <base-type> <equiv>)".into());
    }
    let base = eval(&list[1], env, lex_env)?;
    let equiv = eval(&list[2], env, lex_env)?;
    Ok(Expr::GlueType(Box::new(base), Box::new(equiv)))
}

/// (glue val equiv)
fn eval_glue(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err("glue: expected (glue <val> <equiv>)".into());
    }
    let val = eval(&list[1], env, lex_env)?;
    let equiv = eval(&list[2], env, lex_env)?;
    Ok(Expr::Glue(Box::new(val), Box::new(equiv)))
}

/// (unglue g)
fn eval_unglue(list: &[Expr], env: &Env, lex_env: &Rc<LexEnv>) -> Result<Expr, String> {
    if list.len() != 2 {
        return Err("unglue: expected (unglue <glue-term>)".into());
    }
    let g = eval(&list[1], env, lex_env)?;
    match g {
        Expr::Glue(val, equiv) => apply(*equiv, &[*val], env),
        other => Err(format!("unglue: not a glue term: {:?}", other)),
    }
}