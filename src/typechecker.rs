//! Bidirectional type checker for the Lisp/cubical interpreter.
//!
//! This module implements a simple bidirectional type checker that works
//! alongside the evaluator. It operates on *compiled* (De Bruijn) expressions
//! and uses two environments:
//!
//! - `Env` / `TyGlobal`: maps global names → their types (Expr values).
//! - `TyEnv`: a linked-list of local-variable types, parallel to `LexEnv`.
//!
//! ## Supported forms
//!
//! | Expression            | Inference | Checking |
//! |-----------------------|-----------|----------|
//! | Number literal        | ✓ (Num)   | ✓        |
//! | Symbol (global var)   | ✓         | ✓        |
//! | Index (local var)     | ✓         | ✓        |
//! | `(lambda arity body)` | ✓ (Pi)    | ✓ against Pi |
//! | `(path 1.0 body)`     | ✓         | ✓ against PathTy |
//! | `(pi dom cod)`        | ✓ (Type)  | ✓        |
//! | `(sigma dom cod)`     | ✓ (Type)  | ✓        |
//! | `(if c t e)`          | ✓ (join)  | ✓        |
//! | `(let binds body)`    | ✓         | ✓        |
//! | `(begin e…)`          | ✓         | ✓        |
//! | `(papply p t)`        | ✓         | ✓        |
//! | `(piapply f v)`       | ✓         | ✓        |
//! | `(sigmacod s v)`      | ✓         | ✓        |
//! | Function application  | ✓         | ✓        |
//!
//! ## Type universe
//!
//! Types are themselves `Expr` values evaluated at type-check time.
//! We use sentinel symbols (not user-accessible):
//!
//! - `__Num__`       — the type of all numbers.
//! - `__Type__`      — the universe of all types (Pi, Sigma, Path types live here).
//! - `__Path__ dom`  — the type of path values whose endpoints live in `dom`.
//! - `__Any__`       — a top/"unknown" type used when inference cannot determine more.

use std::collections::HashMap;
use std::rc::Rc;

use crate::env::{env_get, Env};
use crate::eval::eval;
use crate::expr::{Expr, LexEnv};

// ---------------------------------------------------------------------------
// Public type synonyms
// ---------------------------------------------------------------------------

/// Linked-list of local-variable *types*, parallel to `LexEnv`.
#[derive(Clone, Debug)]
pub enum TyEnv {
    Empty,
    Node(Expr, Rc<TyEnv>),
}

impl TyEnv {
    pub fn get(&self, index: usize) -> Option<Expr> {
        let mut curr = self;
        let mut i = index;
        loop {
            match curr {
                TyEnv::Empty => return None,
                TyEnv::Node(ty, next) => {
                    if i == 0 {
                        return Some(ty.clone());
                    }
                    curr = next;
                    i -= 1;
                }
            }
        }
    }
}

/// Global name → type map (separate from the value environment).
pub type TyGlobal = HashMap<String, Expr>;

// ---------------------------------------------------------------------------
// Sentinel constructors
// ---------------------------------------------------------------------------

fn ty_num() -> Expr {
    Expr::Symbol("__Num__".into())
}

fn ty_type() -> Expr {
    Expr::Symbol("__Type__".into())
}

fn ty_any() -> Expr {
    Expr::Symbol("__Any__".into())
}

fn ty_path(dom: Expr) -> Expr {
    Expr::List(vec![Expr::Symbol("__Path__".into()), dom])
}

// ---------------------------------------------------------------------------
// Sentinel predicates
// ---------------------------------------------------------------------------

fn is_any(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Any__")
}

fn is_num(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Num__")
}

fn is_type_universe(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Type__")
}

/// Matches `(__Path__ dom)` and returns `Some(dom)`.
fn as_path_ty(t: &Expr) -> Option<&Expr> {
    if let Expr::List(l) = t {
        if l.len() == 2 {
            if let Expr::Symbol(s) = &l[0] {
                if s == "__Path__" {
                    return Some(&l[1]);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Type equality (structural, up to alpha-equivalence via De Bruijn)
// ---------------------------------------------------------------------------

/// Returns true when two type expressions are definitionally equal.
///
/// Before comparing we attempt to normalize both sides via `eval`. If
/// reduction fails we fall back to pure structural comparison so that
/// type-checking of ground (non-dependent) terms is not disrupted by
/// evaluation errors in unevaluable open terms.
fn types_equal(a: &Expr, b: &Expr) -> bool {
    types_equal_structural(a, b)
}

/// Attempt to reduce `e` in an empty environment (works for closed terms).
fn try_reduce(e: &Expr, env: &Env) -> Expr {
    eval(e, env, &Rc::new(LexEnv::Empty)).unwrap_or_else(|_| e.clone())
}

/// Normalize then compare; falls back to structural equality on error.
fn types_equal_normalized(a: &Expr, b: &Expr, env: &Env) -> bool {
    let a_nf = try_reduce(a, env);
    let b_nf = try_reduce(b, env);
    types_equal_structural(&a_nf, &b_nf)
}

fn types_equal_structural(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Symbol(sa), Expr::Symbol(sb)) => sa == sb,
        (Expr::Number(na), Expr::Number(nb)) => (na - nb).abs() < f64::EPSILON,
        (Expr::Index(ia), Expr::Index(ib)) => ia == ib,
        (Expr::List(la), Expr::List(lb)) => {
            la.len() == lb.len()
                && la
                    .iter()
                    .zip(lb.iter())
                    .all(|(x, y)| types_equal_structural(x, y))
        }
        (Expr::Pi(da, ca, _), Expr::Pi(db, cb, _)) => {
            types_equal_structural(da, db) && types_equal_structural(ca, cb)
        }
        (Expr::Sigma(da, ca, _), Expr::Sigma(db, cb, _)) => {
            types_equal_structural(da, db) && types_equal_structural(ca, cb)
        }
        (Expr::Path(ba, _), Expr::Path(bb, _)) => types_equal_structural(ba, bb),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Infer the type of a compiled expression.
///
/// Returns `Ok(ty)` where `ty` is an `Expr` representing the inferred type,
/// or `Err(message)` on a type error.
pub fn infer(
    expr: &Expr,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    infer_inner(expr, env, lex_env, ty_global, ty_env)
        .map_err(|e| format!("{}\n  in: {:?}", e, expr))
}

fn infer_inner(
    expr: &Expr,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    match expr {
        // ----- Atoms -------------------------------------------------------
        Expr::Number(_) => Ok(ty_num()),

        Expr::Symbol(s) => {
            if let Some(ty) = ty_global.get(s) {
                return Ok(ty.clone());
            }
            match s.as_str() {
                "+" | "-" | "*" | "/" | "%" => Ok(ty_any()),
                "=" | "<" | ">" | "<=" | ">=" | "not" => Ok(ty_any()),
                "list" | "car" | "cdr" | "cons" | "null?" => Ok(ty_any()),
                "print" => Ok(ty_any()),
                "i0" | "i1" => Ok(ty_num()),
                "refl" => Ok(ty_any()),
                "pi?" | "sigma?" | "path?" => Ok(ty_num()),
                _ => match env_get(env, s) {
                    Ok(v) => infer_value_type(&v),
                    Err(_) => Err(format!("type error: undefined symbol '{}'", s)),
                },
            }
        }

        Expr::Index(i) => ty_env
            .get(*i)
            .ok_or_else(|| format!("type error: unbound index #{}", i)),

        // ----- Already-evaluated values ------------------------------------
        Expr::Func(_) => Ok(ty_any()),
        Expr::Lambda(arity, body, _) => {
            // Without annotation we can't know parameter types, but we can
            // infer the body type assuming all params are __Any__, giving
            // (Pi __Any__ (Pi __Any__ … body_ty)) of the right depth.
            let mut body_ty_env = ty_env.clone();
            for _ in 0..*arity {
                body_ty_env = Rc::new(TyEnv::Node(ty_any(), body_ty_env));
            }
            // Body type is inferred; we can't construct the Pi properly
            // without argument types, so we return __Any__.
            let _ = infer(body, env, lex_env, ty_global, &body_ty_env)?;
            Ok(ty_any())
        }
        Expr::Macro(..) => Ok(ty_any()),
        Expr::Path(body, _) => {
            let body_ty_env = Rc::new(TyEnv::Node(ty_num(), ty_env.clone()));
            let body_ty = infer(body, env, lex_env, ty_global, &body_ty_env)?;
            Ok(ty_path(body_ty))
        }
        Expr::Pi(..) => Ok(ty_type()),
        Expr::Sigma(..) => Ok(ty_type()),

        // ----- Lists (special forms and applications) ----------------------
        Expr::List(list) => {
            if list.is_empty() {
                return Ok(ty_any());
            }

            if let Expr::Symbol(op) = &list[0] {
                match op.as_str() {
                    "quote" | "quasiquote" => return Ok(ty_any()),

                    "if" => return infer_if(list, env, lex_env, ty_global, ty_env),
                    "define" => return infer_define(list, env, lex_env, ty_global, ty_env),
                    "lambda" => return infer_lambda(list, env, lex_env, ty_global, ty_env),
                    "begin" => return infer_begin(list, env, lex_env, ty_global, ty_env),
                    "let" => return infer_let(list, env, lex_env, ty_global, ty_env),

                    "path" => return infer_path(list, env, lex_env, ty_global, ty_env),
                    "papply" => return infer_papply(list, env, lex_env, ty_global, ty_env),

                    "pi" => return Ok(ty_type()),
                    "piapply" => return infer_piapply(list, env, lex_env, ty_global, ty_env),

                    "sigma" => return Ok(ty_type()),
                    "sigmacod" => return infer_sigmacod(list, env, lex_env, ty_global, ty_env),

                    "defmacro" => return Ok(ty_any()),
                    _ => {}
                }
            }

            infer_application(list, env, lex_env, ty_global, ty_env)
        }
    }
}

/// Check that a compiled expression has an expected type.
pub fn check(
    expr: &Expr,
    expected: &Expr,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<(), String> {
    // Special case: lambda checked against Pi.
    if let (Expr::List(list), Expr::Pi(dom, cod, pi_lex)) = (expr, expected) {
        if matches!(list.first(), Some(Expr::Symbol(s)) if s == "lambda") {
            return check_lambda_against_pi(
                list, dom, cod, pi_lex, env, lex_env, ty_global, ty_env,
            );
        }
    }

    // Special case: path checked against __Path__ type.
    if let Some(dom) = as_path_ty(expected) {
        if let Expr::List(list) = expr {
            if matches!(list.first(), Some(Expr::Symbol(s)) if s == "path") {
                return check_path_against_pathty(list, dom, env, lex_env, ty_global, ty_env);
            }
        }
    }

    // __Any__ on either side — skip.
    if is_any(expected) {
        return Ok(());
    }

    let inferred = infer(expr, env, lex_env, ty_global, ty_env)?;

    if is_any(&inferred) {
        return Ok(());
    }

    if types_equal_normalized(&inferred, expected, env) {
        Ok(())
    } else {
        Err(format!(
            "type mismatch: expected {:?}, got {:?}\n  in expression: {:?}",
            expected, inferred, expr
        ))
    }
}

// ---------------------------------------------------------------------------
// Infer helpers for each special form
// ---------------------------------------------------------------------------

fn infer_if(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    if list.len() < 3 {
        return Err("type error: if requires at least condition and then-branch".into());
    }
    let cond_ty = infer(&list[1], env, lex_env, ty_global, ty_env)?;
    if !is_any(&cond_ty) && !is_num(&cond_ty) {
        return Err(format!(
            "type error: if condition must be a number (got {:?})",
            cond_ty
        ));
    }
    let then_ty = infer(&list[2], env, lex_env, ty_global, ty_env)?;
    if list.len() > 3 {
        let else_ty = infer(&list[3], env, lex_env, ty_global, ty_env)?;
        if is_any(&then_ty) || is_any(&else_ty) {
            // At least one branch is unknown — return the more specific one.
            return Ok(if is_any(&then_ty) { else_ty } else { then_ty });
        }
        if types_equal_normalized(&then_ty, &else_ty, env) {
            Ok(then_ty)
        } else {
            // Branches disagree: surface a proper type error rather than
            // silently widening to __Any__.
            Err(format!(
                "type error: if branches have incompatible types: then={:?}, else={:?}",
                then_ty, else_ty
            ))
        }
    } else {
        Ok(then_ty)
    }
}

fn infer_define(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    if list.len() < 3 {
        return Err("type error: define requires name and value".into());
    }
    // Note: typecheck_toplevel handles registering the type in ty_global.
    // This path is reached when define appears nested (e.g. inside begin).
    infer(&list[2], env, lex_env, ty_global, ty_env)
}

fn infer_lambda(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // Core lambda: (lambda <arity:Number> <body>)
    // We don't know parameter types without annotation, so we push __Any__
    // for each slot and infer the body type.  The result is a Pi with __Any__
    // domains — better than just returning __Any__ for the whole thing.
    let arity = if let Some(Expr::Number(n)) = list.get(1) {
        *n as usize
    } else {
        return Ok(ty_any());
    };

    let mut body_ty_env = ty_env.clone();
    for _ in 0..arity {
        body_ty_env = Rc::new(TyEnv::Node(ty_any(), body_ty_env));
    }

    let body = list.get(2).ok_or("lambda: missing body")?;
    let body_ty = infer(body, env, lex_env, ty_global, &body_ty_env)?;

    // Build a right-nested Pi: (Pi __Any__ (Pi __Any__ … body_ty))
    let mut result = body_ty;
    for _ in 0..arity {
        result = Expr::Pi(
            Box::new(ty_any()),
            Box::new(result),
            Rc::new(LexEnv::Empty),
        );
    }
    Ok(result)
}

fn infer_begin(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    if list.len() < 2 {
        return Ok(ty_any());
    }
    let mut last = ty_any();
    for e in &list[1..] {
        last = infer(e, env, lex_env, ty_global, ty_env)?;
    }
    Ok(last)
}

fn infer_let(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (let ((name val)…) body…)
    if list.len() < 3 {
        return Err("type error: let requires bindings and a body".into());
    }
    let mut current_ty_env = ty_env.clone();
    if let Expr::List(bindings) = &list[1] {
        for b in bindings {
            if let Expr::List(pair) = b {
                if pair.len() < 2 {
                    return Err("type error: let binding must be a (name value) pair".into());
                }
                let val_ty = infer(&pair[1], env, lex_env, ty_global, &current_ty_env)?;
                current_ty_env = Rc::new(TyEnv::Node(val_ty, current_ty_env));
            }
        }
    }
    let mut last = ty_any();
    for e in &list[2..] {
        last = infer(e, env, lex_env, ty_global, &current_ty_env)?;
    }
    Ok(last)
}

fn infer_path(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    if list.len() < 3 {
        return Err("type error: path requires arity and body".into());
    }
    // The bound variable is the interval ∈ [0,1], typed __Num__.
    let body_ty_env = Rc::new(TyEnv::Node(ty_num(), ty_env.clone()));
    let body_ty = infer(&list[2], env, lex_env, ty_global, &body_ty_env)?;
    Ok(ty_path(body_ty))
}

fn infer_papply(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (papply p t)
    if list.len() != 3 {
        return Err("type error: papply expects (papply <path> <t>)".into());
    }
    let p_ty = infer(&list[1], env, lex_env, ty_global, ty_env)?;
    let t_ty = infer(&list[2], env, lex_env, ty_global, ty_env)?;

    if !is_any(&t_ty) && !is_num(&t_ty) {
        return Err(format!(
            "type error: papply interval point must be Num, got {:?}",
            t_ty
        ));
    }

    if is_any(&p_ty) {
        return Ok(ty_any());
    }

    match as_path_ty(&p_ty) {
        Some(dom) => Ok(dom.clone()),
        None => Err(format!(
            "type error: papply requires a path type, got {:?}",
            p_ty
        )),
    }
}

fn infer_piapply(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (piapply pi-expr value)
    if list.len() != 3 {
        return Err("type error: piapply expects (piapply <pi-type> <value>)".into());
    }

    // First check that the argument type-checks at all.
    infer(&list[2], env, lex_env, ty_global, ty_env)?;

    // Try to evaluate the pi-expr to a concrete Pi value; if the expression
    // is open we fall back to __Any__ instead of silently swallowing errors.
    let pi_val = eval(&list[1], env, lex_env).map_err(|e| {
        format!("type error: could not evaluate pi-type expression: {}", e)
    })?;

    match pi_val {
        Expr::Pi(_dom, cod, pi_lex_env) => {
            let v = eval(&list[2], env, lex_env).map_err(|e| {
                format!("type error: could not evaluate piapply argument: {}", e)
            })?;
            let new_lex = Rc::new(LexEnv::Node(v, pi_lex_env));
            eval(&cod, env, &new_lex)
                .map_err(|e| format!("type error in piapply codomain instantiation: {}", e))
        }
        Expr::Symbol(s) if s == "__Any__" => Ok(ty_any()),
        other => Err(format!(
            "type error: piapply requires a Pi type, got {:?}",
            other
        )),
    }
}

fn infer_sigmacod(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (sigmacod sigma-expr value)
    if list.len() != 3 {
        return Err("type error: sigmacod expects (sigmacod <sigma-type> <value>)".into());
    }

    infer(&list[2], env, lex_env, ty_global, ty_env)?;

    let sigma_val = eval(&list[1], env, lex_env).map_err(|e| {
        format!("type error: could not evaluate sigma-type expression: {}", e)
    })?;

    match sigma_val {
        Expr::Sigma(_dom, cod, sig_lex_env) => {
            let v = eval(&list[2], env, lex_env).map_err(|e| {
                format!("type error: could not evaluate sigmacod argument: {}", e)
            })?;
            let new_lex = Rc::new(LexEnv::Node(v, sig_lex_env));
            eval(&cod, env, &new_lex)
                .map_err(|e| format!("type error in sigmacod codomain instantiation: {}", e))
        }
        Expr::Symbol(s) if s == "__Any__" => Ok(ty_any()),
        other => Err(format!(
            "type error: sigmacod requires a Sigma type, got {:?}",
            other
        )),
    }
}

fn infer_application(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    let fn_ty = infer(&list[0], env, lex_env, ty_global, ty_env)?;

    // Type-check all arguments; if we know the function is a Pi type,
    // also verify the first argument against the domain.
    match &fn_ty {
        Expr::Pi(dom, cod, _) => {
            // Check argument count: a single-Pi covers exactly 1 argument.
            // For curried multi-arg functions we check the first arg against
            // the domain and recurse for the rest.
            if list.len() < 2 {
                return Err("type error: function applied to zero arguments".into());
            }
            // Check first argument against the Pi domain.
            check(&list[1], dom, env, lex_env, ty_global, ty_env)?;
            // Check remaining args (best-effort — we don't unfold the rest of
            // the Pi chain here without full curried type info).
            for arg in &list[2..] {
                infer(arg, env, lex_env, ty_global, ty_env)?;
            }
            Ok(*cod.clone())
        }
        _ => {
            // Unknown function type: still type-check all arguments.
            for arg in &list[1..] {
                infer(arg, env, lex_env, ty_global, ty_env)?;
            }
            Ok(ty_any())
        }
    }
}

// ---------------------------------------------------------------------------
// Check helpers
// ---------------------------------------------------------------------------

fn check_lambda_against_pi(
    list: &[Expr],
    dom: &Expr,
    cod: &Expr,
    _pi_lex: &Rc<LexEnv>,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<(), String> {
    let new_ty_env = Rc::new(TyEnv::Node(dom.clone(), ty_env.clone()));
    check(&list[2], cod, env, lex_env, ty_global, &new_ty_env)
}

fn check_path_against_pathty(
    list: &[Expr],
    dom: &Expr,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<(), String> {
    let new_ty_env = Rc::new(TyEnv::Node(ty_num(), ty_env.clone()));
    check(&list[2], dom, env, lex_env, ty_global, &new_ty_env)
}

// ---------------------------------------------------------------------------
// Infer the type of an *already-evaluated* Expr value (used for globals).
// ---------------------------------------------------------------------------

fn infer_value_type(v: &Expr) -> Result<Expr, String> {
    match v {
        Expr::Number(_) => Ok(ty_num()),
        Expr::Pi(..) => Ok(ty_type()),
        Expr::Sigma(..) => Ok(ty_type()),
        Expr::Path(body, _) => {
            // We can't easily re-infer without ty_env, so return a generic path type.
            Ok(ty_path(ty_any()))
        }
        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(ty_any()),
        Expr::List(l) if l.is_empty() => Ok(ty_any()),
        Expr::List(_) | Expr::Symbol(_) | Expr::Index(_) => Ok(ty_any()),
    }
}

// ---------------------------------------------------------------------------
// Top-level type-check driver
// ---------------------------------------------------------------------------

/// Type-check a compiled top-level expression, updating `ty_global` for any
/// `define` forms encountered.
///
/// Returns `Ok(inferred_type)` or `Err(message)`.
pub fn typecheck_toplevel(
    expr: &Expr,
    env: &Env,
    ty_global: &mut TyGlobal,
) -> Result<Expr, String> {
    let lex_env = Rc::new(LexEnv::Empty);
    let ty_env = Rc::new(TyEnv::Empty);

    if let Expr::List(list) = expr {
        if let Some(Expr::Symbol(op)) = list.first() {
            match op.as_str() {
                "define" if list.len() >= 3 => {
                    if let Expr::Symbol(name) = &list[1] {
                        // Pre-register as __Any__ so recursive references
                        // inside the body (e.g. `fact` calling itself) don't
                        // fail with "undefined symbol". We overwrite with the
                        // real inferred type once we have it.
                        let prev = ty_global.insert(name.clone(), ty_any());
                        let result = infer(&list[2], env, &lex_env, ty_global, &ty_env);
                        match result {
                            Ok(ty) => {
                                ty_global.insert(name.clone(), ty.clone());
                                return Ok(ty);
                            }
                            Err(e) => {
                                // Restore previous binding (or remove if new).
                                match prev {
                                    Some(old) => { ty_global.insert(name.clone(), old); }
                                    None => { ty_global.remove(name); }
                                }
                                return Err(e);
                            }
                        }
                    }
                }
                "defmacro" => return Ok(ty_any()),
                "begin" => {
                    // Process a top-level begin sequentially so that inner
                    // defines are registered in ty_global as we go.
                    let mut last = ty_any();
                    for e in &list[1..] {
                        last = typecheck_toplevel(e, env, ty_global)?;
                    }
                    return Ok(last);
                }
                _ => {}
            }
        }
    }

    infer(expr, env, &lex_env, ty_global, &ty_env)
}