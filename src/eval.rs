use crate::env::{Env, env_get, env_set, new_env};
use crate::expr::{Expr, is_truthy};
use crate::gc::{GcHandle, Heap};
use crate::macros::{eval_quasiquote, expand_macro};
use crate::reader::parse_params;

// How many live heap slots we allow before triggering a collection inside
// `apply`.  Tune this to trade GC frequency against peak memory use.
const GC_THRESHOLD: usize = 1024;

/// Evaluates an expression in the given environment.
///
/// `heap` is the GC heap that owns all `EnvData` frames.  It must be passed
/// to every recursive call so that allocations and lookups go to the same
/// heap, and so that the GC can be triggered at appropriate points.
pub fn eval(expr: &Expr, env: Env, heap: &mut Heap) -> Result<Expr, String> {
    match expr {
        Expr::Number(_)      => Ok(expr.clone()),
        Expr::Str(_)         => Ok(expr.clone()),
        // CubicalTerm values are opaque atoms — they self-evaluate just like
        // numbers and are only inspected by the cubical builtins.
        Expr::CubicalTerm(_) => Ok(expr.clone()),

        Expr::Symbol(s) => env_get(heap, env, s),

        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(expr.clone()),

        Expr::List(list) => {
            if list.is_empty() {
                return Ok(Expr::List(vec![]));
            }

            if let Expr::Symbol(op) = &list[0] {
                match op.as_str() {
                    "quote" => {
                        if list.len() != 2 {
                            return Err(format!(
                                "quote expects 1 argument, got {}",
                                list.len() - 1
                            ));
                        }
                        return Ok(list[1].clone());
                    }

                    "quasiquote" => {
                        if list.len() != 2 {
                            return Err(format!(
                                "quasiquote expects 1 argument, got {}",
                                list.len() - 1
                            ));
                        }
                        // eval_quasiquote needs heap for any nested unquote
                        // splices that call back into eval.
                        return eval_quasiquote(&list[1], env, heap, 1);
                    }

                    "unquote" => return Err("unquote outside quasiquote".into()),

                    "if"       => return eval_if(list, env, heap),
                    "define"   => return eval_define(list, env, heap),
                    "lambda"   => return eval_lambda(list, env, heap),
                    "defmacro" => return eval_defmacro(list, env, heap),
                    "begin"    => return eval_begin(list, env, heap),
                    "let"      => return eval_let(list, env, heap),

                    _ => {
                        // If `op` names a macro, expand (with raw, unevaluated
                        // argument expressions) and evaluate the result.
                        if let Ok(Expr::Macro(params, body)) = env_get(heap, env, op) {
                            let substituted = expand_macro(&params, &body, &list[1..])?;
                            let expanded    = eval(&substituted, env, heap)?;
                            return eval(&expanded, env, heap);
                        }
                    }
                }
            }

            // Normal function application: evaluate operator and operands.
            let func = eval(&list[0], env, heap)?;
            let args: Result<Vec<Expr>, String> =
                list[1..].iter().map(|e| eval(e, env, heap)).collect();
            apply(func, &args?, env, heap)
        }
    }
}

// ── special forms ─────────────────────────────────────────────────────────────

/// `(if cond then [else])`
fn eval_if(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    if list.len() < 3 || list.len() > 4 {
        return Err(format!(
            "if expects 2 or 3 arguments, got {}",
            list.len() - 1
        ));
    }
    let cond = eval(&list[1], env, heap)?;
    if is_truthy(&cond) {
        eval(&list[2], env, heap)
    } else if list.len() > 3 {
        eval(&list[3], env, heap)
    } else {
        Ok(Expr::List(vec![]))
    }
}

/// `(define name expr)`
fn eval_define(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err(format!(
            "define expects 2 arguments, got {}",
            list.len() - 1
        ));
    }
    if let Expr::Symbol(name) = &list[1] {
        let val = eval(&list[2], env, heap)?;
        env_set(heap, env, name.clone(), val.clone());
        Ok(val)
    } else {
        Err("invalid define: expected (define <symbol> <expr>)".into())
    }
}

/// `(lambda (params...) body)`
///
/// ### Why no `downgrade` call
///
/// The old code stored a `WeakEnv` (`Weak<RefCell<EnvData>>`) to avoid
/// creating a strong `Rc` cycle when the resulting lambda was `define`d back
/// into the same environment.  With the GC design there are no reference
/// counts at all — `GcHandle` is a plain `Copy` integer — so storing `env`
/// directly is safe and correct.  Liveness is determined by the mark phase,
/// not by Rust's drop order.
fn eval_lambda(list: &[Expr], env: Env, _heap: &mut Heap) -> Result<Expr, String> {
    if list.len() != 3 {
        return Err(format!(
            "lambda expects 2 arguments (params body), got {}",
            list.len() - 1
        ));
    }
    let params = parse_params(&list[1])?;
    // Store `env` (a GcHandle) directly — no Weak, no downgrade.
    Ok(Expr::Lambda(params, Box::new(list[2].clone()), env))
}

/// `(defmacro name (params...) body)`
fn eval_defmacro(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    if list.len() != 4 {
        return Err(format!(
            "defmacro expects 3 arguments (name params body), got {}",
            list.len() - 1
        ));
    }
    if let Expr::Symbol(name) = &list[1] {
        let params = parse_params(&list[2])?;
        let mac    = Expr::Macro(params, Box::new(list[3].clone()));
        env_set(heap, env, name.clone(), mac.clone());
        Ok(mac)
    } else {
        Err("invalid defmacro: expected (defmacro <symbol> (<params...>) <body>)".into())
    }
}

/// `(begin expr...)`
fn eval_begin(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    let mut result = Expr::List(vec![]);
    for e in &list[1..] {
        result = eval(e, env, heap)?;
    }
    Ok(result)
}

/// `(let ((name expr)...) body...)`
fn eval_let(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    if list.len() < 3 {
        return Err(format!(
            "let expects at least 2 arguments (bindings body), got {}",
            list.len() - 1
        ));
    }
    // Allocate a fresh child frame.  `env` is its only root right now, but
    // it is reachable from the Rust stack so it is safe across any GC triggered
    // inside the binding-evaluation loop below.
    let new_e = new_env(heap, Some(env));

    if let Expr::List(bindings) = &list[1] {
        for b in bindings {
            match b {
                Expr::List(pair) if pair.len() == 2 => {
                    if let Expr::Symbol(name) = &pair[0] {
                        // Evaluate the RHS in the *outer* env (standard `let`
                        // semantics — bindings don't see each other).
                        let val = eval(&pair[1], env, heap)?;
                        env_set(heap, new_e, name.clone(), val);
                    } else {
                        return Err(format!(
                            "let binding name must be a symbol, got: {:?}",
                            pair[0]
                        ));
                    }
                }
                other => {
                    return Err(format!(
                        "let binding must be a (name expr) pair, got: {:?}",
                        other
                    ));
                }
            }
        }
    } else {
        return Err(format!(
            "let expects a list of bindings, got: {:?}",
            list[1]
        ));
    }

    let mut result = Expr::List(vec![]);
    for e in &list[2..] {
        result = eval(e, new_e, heap)?;
    }
    Ok(result)
}

// ── function application ──────────────────────────────────────────────────────

/// Applies a function (builtin or lambda) to already-evaluated arguments.
///
/// `call_site_env` is the environment at the call site; it is passed only so
/// that it can be included in the GC root set when a collection is triggered
/// inside this call.
pub fn apply(
    func:          Expr,
    args:          &[Expr],
    call_site_env: Env,
    heap:          &mut Heap,
) -> Result<Expr, String> {
    match func {
        Expr::Func(f) => {
            // Built-in functions receive the heap so they can allocate or
            // call back into eval if needed.
            f(args, heap)
        }

        Expr::Lambda(params, body, closure_env) => {
            // ── 1. Allocate the call frame ────────────────────────────────
            //
            // The call frame's parent is `closure_env` (where the lambda was
            // *defined*), not `call_site_env` (where it was *called*).  This
            // is standard lexical scoping.
            let call_frame = new_env(heap, Some(closure_env));

            // Bind parameters to arguments.
            if params.len() != args.len() {
                return Err(format!(
                    "arity mismatch: expected {} arguments, got {}",
                    params.len(),
                    args.len()
                ));
            }
            for (p, a) in params.iter().zip(args.iter()) {
                env_set(heap, call_frame, p.clone(), a.clone());
            }

            // ── 2. Maybe collect ──────────────────────────────────────────
            //
            // Trigger a GC cycle if the heap has grown past the threshold.
            // Roots we must keep alive:
            //   • `call_site_env`  — the caller's env (may hold live values)
            //   • `closure_env`    — the lambda's captured env
            //   • `call_frame`     — the frame we just built
            //
            // Note: `call_frame`'s parent chain already includes `closure_env`,
            // so the mark phase would reach it anyway — listing both is just
            // defensive and costs nothing.
            if heap.live_count() > GC_THRESHOLD {
                heap.collect(&[call_site_env, closure_env, call_frame]);
            }

            // ── 3. Evaluate the body ──────────────────────────────────────
            eval(&body, call_frame, heap)
        }

        other => Err(format!("not a function: {:?}", other)),
    }
}