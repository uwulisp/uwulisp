use crate::env::{Env, env_get, env_set, new_env};
use crate::expr::{Expr, is_truthy};
use crate::gc::Heap;
use crate::macros::{eval_quasiquote, expand_macro};
use crate::reader::{parse_all, parse_params};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

// How many live heap slots we allow before triggering a collection inside
// `apply`.  Tune this to trade GC frequency against peak memory use.
const GC_THRESHOLD: usize = 1024;

thread_local! {
    static IMPORT_BASES: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

// ── trampoline ────────────────────────────────────────────────────────────────

/// The result of one "step" inside the trampoline loop.
///
/// Most expressions produce a finished `Value` immediately.  The tail-call
/// positions — the selected branch of `if`, the last expression of `begin` /
/// `let`, a lambda body, and the explicit `(tailcall ...)` form — instead
/// produce `TailCall`, which tells the loop to iterate rather than recurse.
///
/// This type is intentionally private; callers always see `Result<Expr, String>`.
enum Step {
    /// A fully evaluated result — exit the trampoline.
    Value(Expr),
    /// Tail-call: evaluate `expr` in `env` on the next iteration.
    TailCall { expr: Expr, env: Env },
}

// ── public API ────────────────────────────────────────────────────────────────

/// Evaluates an expression in the given environment.
///
/// `heap` is the GC heap that owns all `EnvData` frames.  It must be passed
/// to every recursive call so that allocations and lookups go to the same
/// heap, and so that the GC can be triggered at appropriate points.
///
/// ### Tail-call optimization
///
/// `eval` is implemented as a trampoline loop.  Tail positions inside `if`,
/// `begin`, `let`, lambda application, and the explicit `(tailcall ...)` form
/// iterate the loop instead of growing the Rust call stack.  This means
/// arbitrarily deep tail recursion uses O(1) stack frames.
///
/// **Explicit tail calls** — use `(tailcall f arg ...)` anywhere you want to
/// guarantee that a call is optimized.  `f` is evaluated first, then the
/// arguments, and the call is performed as a trampoline step:
///
/// ```scheme
/// ; Stack-safe countdown via explicit tail call:
/// (define (count-down n)
///   (if (= n 0)
///       "done"
///       (tailcall count-down (- n 1))))
/// ```
///
/// Plain calls in tail position inside `if` / `begin` / `let` / lambda bodies
/// are *also* trampolined automatically, so for direct recursion you usually
/// do not need `tailcall` at all.  `tailcall` becomes necessary for **mutual
/// recursion** (e.g. `(tailcall other-fn ...)`) because the optimizer cannot
/// statically know whether an arbitrary call expression is in tail position
/// with respect to the *current* lambda's stack frame.
pub fn eval(expr: &Expr, env: Env, heap: &mut Heap) -> Result<Expr, String> {
    // When compiled with `--features vm`, delegate to the bytecode VM.
    // The VM falls back to the tree-walker for uncompilable expressions
    // (e.g. those containing CubicalTerm), so behaviour is always correct.
    #[cfg(feature = "vm")]
    #[allow(unreachable_code)]
    {
        return crate::vm::vm_eval(expr, env, heap);
    }

    eval_tree(expr, env, heap)
}

/// Perform plain tree-walking evaluation, bypassing the VM check.
/// This prevents infinite recursion during VM fallback.
pub fn eval_tree(expr: &Expr, env: Env, heap: &mut Heap) -> Result<Expr, String> {
    // Trampoline state: the expression and environment for the current iteration.
    let mut cur_expr = expr.clone();
    let mut cur_env = env;

    loop {
        let step = eval_step(&cur_expr, cur_env, heap)?;
        match step {
            Step::Value(v) => return Ok(v),
            Step::TailCall { expr: e, env } => {
                cur_expr = e;
                cur_env = env;
            }
        }
    }
}

/// Temporarily adds a base directory used to resolve relative import paths.
pub fn with_import_base<T>(base: Option<&Path>, f: impl FnOnce() -> T) -> T {
    if let Some(base) = base {
        IMPORT_BASES.with(|bases| bases.borrow_mut().push(base.to_path_buf()));
        let result = f();
        IMPORT_BASES.with(|bases| {
            bases.borrow_mut().pop();
        });
        result
    } else {
        f()
    }
}

/// Applies a function (builtin or lambda) to already-evaluated arguments.
///
/// `call_site_env` is the environment at the call site; it is passed only so
/// that it can be included in the GC root set when a collection is triggered
/// inside this call.
///
/// When called from the trampoline loop (lambda application) this always
/// recurses into `eval` for the body — that inner `eval` has its own
/// trampoline, so the lambda body runs stack-free.  When called from user
/// code outside the loop (e.g. a builtin that calls back into eval) the same
/// applies.
pub fn apply(
    func: Expr,
    args: &[Expr],
    call_site_env: Env,
    heap: &mut Heap,
) -> Result<Expr, String> {
    match apply_step(func, args, call_site_env, heap)? {
        Step::Value(v) => Ok(v),
        Step::TailCall { expr, env } => eval(&expr, env, heap),
    }
}

// ── core step function ────────────────────────────────────────────────────────

/// One iteration of the trampoline: evaluates `expr` in `env` and returns
/// either a finished value or a tail-call descriptor.
fn eval_step(expr: &Expr, env: Env, heap: &mut Heap) -> Result<Step, String> {
    match expr {
        Expr::Number(_) => Ok(Step::Value(expr.clone())),
        Expr::Str(_) => Ok(Step::Value(expr.clone())),
        // CubicalTerm values are opaque atoms — they self-evaluate just like
        // numbers and are only inspected by the cubical builtins.
        Expr::CubicalTerm(_) => Ok(Step::Value(expr.clone())),

        Expr::Symbol(s) => Ok(Step::Value(env_get(heap, env, s)?)),

        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(Step::Value(expr.clone())),

        Expr::List(list) => {
            if list.is_empty() {
                return Ok(Step::Value(Expr::List(vec![])));
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
                        return Ok(Step::Value(list[1].clone()));
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
                        return Ok(Step::Value(eval_quasiquote(&list[1], env, heap, 1)?));
                    }

                    "unquote" => return Err("unquote outside quasiquote".into()),

                    // ── special forms that return a Step directly ──────────
                    "if" => return eval_if(list, env, heap),
                    "define" => return Ok(Step::Value(eval_define(list, env, heap)?)),
                    "lambda" => return Ok(Step::Value(eval_lambda(list, env, heap)?)),
                    "defmacro" => return Ok(Step::Value(eval_defmacro(list, env, heap)?)),
                    "import" => return Ok(Step::Value(eval_import(list, env, heap)?)),
                    "begin" => return eval_begin(list, env, heap),
                    "let" => return eval_let(list, env, heap),
                    "let*" => return eval_let_star(list, env, heap),
                    "for" => return eval_for(list, env, heap),

                    // ── explicit tail-call form ────────────────────────────
                    //
                    // `(tailcall f arg ...)` evaluates `f` and all `arg`s,
                    // then hands the call to `apply_step`.  If `f` is a
                    // lambda the result is a `TailCall` that the trampoline
                    // loop processes on the next iteration — no new Rust stack
                    // frame is consumed for the lambda body.  If `f` is a
                    // builtin the call is executed immediately (builtins are
                    // opaque Rust functions; we cannot defer them).
                    "tailcall" => {
                        if list.len() < 2 {
                            return Err("tailcall expects at least a function".into());
                        }
                        let func = eval(&list[1], env, heap)?;
                        let args: Result<Vec<Expr>, String> =
                            list[2..].iter().map(|e| eval(e, env, heap)).collect();
                        return apply_step(func, &args?, env, heap);
                    }

                    _ => {
                        // If `op` names a macro, expand (with raw, unevaluated
                        // argument expressions) and evaluate the result.
                        //
                        // Macro expansion requires two eval passes (substitute
                        // then evaluate the template, then evaluate the result).
                        // Both are non-tail so we call eval() directly; the
                        // second eval has its own trampoline internally.
                        if let Ok(Expr::Macro(params, body)) = env_get(heap, env, op) {
                            let substituted = expand_macro(&params, &body, &list[1..])?;
                            let expanded = eval(&substituted, env, heap)?;
                            // The expanded form is in tail position — trampoline it.
                            return Ok(Step::TailCall {
                                expr: expanded,
                                env,
                            });
                        }
                    }
                }
            }

            // Normal function application: evaluate operator and operands,
            // then delegate to apply_step so a lambda body becomes a TailCall.
            let func = eval(&list[0], env, heap)?;
            let args: Result<Vec<Expr>, String> =
                list[1..].iter().map(|e| eval(e, env, heap)).collect();
            apply_step(func, &args?, env, heap)
        }
    }
}

// ── special forms ─────────────────────────────────────────────────────────────

/// `(if cond then [else])`
///
/// The selected branch is in tail position: we return a `TailCall` so the
/// trampoline evaluates it without growing the stack.
fn eval_if(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Step, String> {
    if list.len() < 3 || list.len() > 4 {
        return Err(format!(
            "if expects 2 or 3 arguments, got {}",
            list.len() - 1
        ));
    }
    let cond = eval(&list[1], env, heap)?;
    if is_truthy(&cond) {
        Ok(Step::TailCall {
            expr: list[2].clone(),
            env,
        })
    } else if list.len() > 3 {
        Ok(Step::TailCall {
            expr: list[3].clone(),
            env,
        })
    } else {
        Ok(Step::Value(Expr::List(vec![])))
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
        let mac = Expr::Macro(params, Box::new(list[3].clone()));
        env_set(heap, env, name.clone(), mac);
        // defmacro returns () — same convention as define.
        Ok(Expr::List(vec![]))
    } else {
        Err("invalid defmacro: expected (defmacro <symbol> (<params...>) <body>)".into())
    }
}

/// `(import "path")`
///
/// Reads another uwulisp source file, evaluates each top-level form in the
/// current environment, and returns the last result. Relative imports are
/// resolved against the importing file's directory when available.
fn eval_import(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Expr, String> {
    if list.len() != 2 {
        return Err(format!(
            "import expects 1 argument (path string), got {}",
            list.len() - 1
        ));
    }

    let requested = match &list[1] {
        Expr::Str(path) => path,
        other => {
            return Err(format!(
                "import: path must be a string literal, got {:?}",
                other
            ));
        }
    };

    let path = resolve_import_path(requested);
    let src = fs::read_to_string(&path)
        .map_err(|err| format!("import: cannot read '{}': {}", path.display(), err))?;
    let exprs = parse_all(&src)
        .map_err(|err| format!("import: parse error in '{}': {}", path.display(), err))?;

    let mut result = Expr::List(vec![]);
    let base = path.parent();
    with_import_base(base, || {
        for expr in exprs {
            result = eval(&expr, env, heap)
                .map_err(|err| format!("import: error in '{}': {}", path.display(), err))?;
        }
        Ok(result)
    })
}

fn resolve_import_path(path: &str) -> PathBuf {
    let requested = Path::new(path);
    if requested.is_absolute() {
        return requested.to_path_buf();
    }

    IMPORT_BASES.with(|bases| {
        if let Some(base) = bases.borrow().last() {
            base.join(requested)
        } else {
            requested.to_path_buf()
        }
    })
}

/// `(begin expr...)`
///
/// All expressions except the last are evaluated strictly (they may have side
/// effects).  The last expression is in tail position and becomes a `TailCall`.
fn eval_begin(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Step, String> {
    let body = &list[1..];
    if body.is_empty() {
        return Ok(Step::Value(Expr::List(vec![])));
    }
    // Evaluate all but the last eagerly (side effects).
    for e in &body[..body.len() - 1] {
        eval(e, env, heap)?;
    }
    // Last expression is in tail position.
    Ok(Step::TailCall {
        expr: body[body.len() - 1].clone(),
        env,
    })
}

/// `(let ((name expr)...) body...)`
///
/// The last body expression is in tail position and becomes a `TailCall`.
fn eval_let(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Step, String> {
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

    let body = &list[2..];
    // Evaluate all but the last body expression eagerly.
    for e in &body[..body.len() - 1] {
        eval(e, new_e, heap)?;
    }
    // Last body expression is in tail position — trampoline into new_e.
    Ok(Step::TailCall {
        expr: body[body.len() - 1].clone(),
        env: new_e,
    })
}

/// `(let* ((name expr)...) body...)`
///
/// Sequential let semantics: each binding is evaluated immediately and is
/// visible to the following bindings.  All bindings and the body share a
/// single child scope opened before the first binding.
fn eval_let_star(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Step, String> {
    if list.len() < 3 {
        return Err(format!(
            "let* expects at least 2 arguments (bindings body), got {}",
            list.len() - 1
        ));
    }
    // Open a single child scope — all bindings and the body share it.
    let child_env = crate::expr::new_env(heap, Some(env));

    if let Expr::List(bindings) = &list[1] {
        for b in bindings {
            match b {
                Expr::List(pair) if pair.len() == 2 => {
                    if let Expr::Symbol(name) = &pair[0] {
                        // Evaluate RHS in the *current* (sequentially extended)
                        // child env so later bindings can see earlier ones.
                        let val = eval(&pair[1], child_env, heap)?;
                        crate::env::env_set(heap, child_env, name.clone(), val);
                    } else {
                        return Err(format!(
                            "let* binding name must be a symbol, got: {:?}",
                            pair[0]
                        ));
                    }
                }
                other => {
                    return Err(format!(
                        "let* binding must be a (name expr) pair, got: {:?}",
                        other
                    ));
                }
            }
        }
    } else {
        return Err(format!(
            "let* expects a list of bindings, got: {:?}",
            list[1]
        ));
    }

    let body = &list[2..];
    // Evaluate all but the last body expression eagerly.
    for e in &body[..body.len() - 1] {
        eval(e, child_env, heap)?;
    }
    // Last body expression is in tail position — trampoline into child_env.
    Ok(Step::TailCall {
        expr: body[body.len() - 1].clone(),
        env: child_env,
    })
}

/// `(for var arg body...)`
///
/// Dual semantics:
/// - `(for var coll body...)` when fewer than 5 elements, or when the third
///   and fourth arguments do not both evaluate to numbers.
/// - `(for var start end body...)` when there are 5+ elements and both
///   `start` and `end` evaluate to numbers.  `var` runs from `start` up to
///   but not including `end`, stepping by 1.0.
///
/// Always returns `()`.
fn eval_for(list: &[Expr], env: Env, heap: &mut Heap) -> Result<Step, String> {
    if list.len() < 4 {
        return Err(format!(
            "for expects at least 3 arguments (var collection body), got {}",
            list.len() - 1
        ));
    }

    let var_name = match &list[1] {
        Expr::Symbol(name) => name.clone(),
        other => {
            return Err(format!(
                "for: loop variable must be a symbol, got {:?}",
                other
            ));
        }
    };

    let loop_env = new_env(heap, Some(env));

    let numeric = if list.len() >= 5 {
        let start = eval(&list[2], env, heap)?;
        let end = eval(&list[3], env, heap)?;
        if let (Expr::Number(start_n), Expr::Number(end_n)) = (start, end) {
            let body = &list[4..];
            let mut i = start_n;
            while i < end_n {
                env_set(heap, loop_env, var_name.clone(), Expr::Number(i));
                for e in body {
                    eval(e, loop_env, heap)?;
                }
                i += 1.0;
            }
            true
        } else {
            false
        }
    } else {
        false
    };

    if !numeric {
        let coll = eval(&list[2], env, heap)?;
        let body = &list[3..];
        let items = match coll {
            Expr::List(items) => items,
            other => {
                return Err(format!(
                    "for: collection must be a list, got {:?}",
                    other
                ));
            }
        };
        for item in items {
            env_set(heap, loop_env, var_name.clone(), item);
            for e in body {
                eval(e, loop_env, heap)?;
            }
        }
    }

    Ok(Step::Value(Expr::List(vec![])))
}

// ── function application ──────────────────────────────────────────────────────

/// Core of `apply`: returns a `Step` so the trampoline can avoid a stack frame
/// for lambda bodies.
///
/// `call_site_env` is the environment at the call site; it is passed only so
/// that it can be included in the GC root set when a collection is triggered
/// inside this call.
fn apply_step(
    func: Expr,
    args: &[Expr],
    call_site_env: Env,
    heap: &mut Heap,
) -> Result<Step, String> {
    match func {
        Expr::Func(f) => {
            // Built-in functions receive the heap so they can allocate or
            // call back into eval if needed.  We cannot defer them, so execute
            // immediately and wrap the result in Step::Value.
            Ok(Step::Value(f(args, heap)?))
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

            // ── 3. Return a TailCall instead of recursing ─────────────────
            //
            // Previously this called `eval(&body, call_frame, heap)`.  Now we
            // return a Step::TailCall so the trampoline loop in `eval` picks
            // it up on the next iteration — no new Rust stack frame is created
            // for the lambda body.
            Ok(Step::TailCall {
                expr: *body,
                env: call_frame,
            })
        }

        other => Err(format!("not a function: {:?}", other)),
    }
}
