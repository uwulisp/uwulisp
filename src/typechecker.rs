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
//! |-----------------------|-----------|---------- |
//! | Number literal        | ✓ (Num)   | ✓        |
//! | Symbol (global var)   | ✓         | ✓        |
//! | Index (local var)     | ✓         | ✓        |
//! | `(lambda arity body)` | –         | ✓ against Pi |
//! | `(path 1.0 body)`     | –         | ✓ against PathTy |
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
//! We use two special sentinel symbols (not user-accessible):
//!
//! - `__Num__`   — the type of all numbers.
//! - `__Type__`  — the universe (kind) of all types (Pi, Sigma, Path types live here).
//! - `__Path__ dom` — the type of path values whose endpoints live in `dom`.
//! - `__Any__`   — a top/"unknown" type used when inference cannot determine more.

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

fn ty_bool() -> Expr {
    // Booleans are represented as numbers in this Lisp, so they share __Num__.
    ty_num()
}

// ---------------------------------------------------------------------------
// Type equality (structural, up to alpha-equivalence via De Bruijn)
// ---------------------------------------------------------------------------

/// Returns true when two type expressions are definitionally equal.
/// For now we use structural equality; for dependent types we would need
/// to reduce both sides first.
fn types_equal(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (Expr::Symbol(sa), Expr::Symbol(sb)) => sa == sb,
        (Expr::Number(na), Expr::Number(nb)) => (na - nb).abs() < f64::EPSILON,
        (Expr::Index(ia), Expr::Index(ib)) => ia == ib,
        (Expr::List(la), Expr::List(lb)) => {
            la.len() == lb.len() && la.iter().zip(lb.iter()).all(|(x, y)| types_equal(x, y))
        }
        // Pi types: compare domains and codomains
        (Expr::Pi(da, ca, _), Expr::Pi(db, cb, _)) => {
            types_equal(da, db) && types_equal(ca, cb)
        }
        // Sigma types
        (Expr::Sigma(da, ca, _), Expr::Sigma(db, cb, _)) => {
            types_equal(da, db) && types_equal(ca, cb)
        }
        // Path types
        (Expr::Path(ba, _), Expr::Path(bb, _)) => types_equal(ba, bb),
        _ => false,
    }
}

fn is_any(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Any__")
}

fn is_num(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Num__")
}

fn is_type_universe(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Type__")
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
    match expr {
        // ----- Atoms -------------------------------------------------------
        Expr::Number(_) => Ok(ty_num()),

        Expr::Symbol(s) => {
            // Check our type environment first, then fall back to __Any__.
            if let Some(ty) = ty_global.get(s) {
                return Ok(ty.clone());
            }
            // Builtin functions: we know their types informally.
            match s.as_str() {
                "+" | "-" | "*" | "/" => Ok(ty_any()), // (Num…) -> Num; simplified
                "=" | "<" | ">" | "<=" | ">=" | "not" => Ok(ty_any()),
                "list" | "car" | "cdr" | "cons" | "null?" => Ok(ty_any()),
                "print" => Ok(ty_any()),
                "i0" | "i1" => Ok(ty_num()),
                "refl" => Ok(ty_any()),
                "pi?" | "sigma?" => Ok(ty_num()),
                _ => {
                    // Check the value env to see if we can get type information.
                    match env_get(env, s) {
                        Ok(v) => infer_value_type(&v),
                        Err(_) => Err(format!("type error: undefined symbol '{}'", s)),
                    }
                }
            }
        }

        Expr::Index(i) => ty_env
            .get(*i)
            .ok_or_else(|| format!("type error: unbound index #{}", i)),

        // ----- Already-evaluated values (from builtins/global env) ----------
        Expr::Func(_) => Ok(ty_any()),
        Expr::Lambda(..) => Ok(ty_any()),
        Expr::Macro(..) => Ok(ty_any()),
        Expr::Path(..) => Ok(ty_any()),
        Expr::Pi(..) => Ok(ty_type()),
        Expr::Sigma(..) => Ok(ty_type()),

        // ----- Lists (special forms and applications) ----------------------
        Expr::List(list) => {
            if list.is_empty() {
                // () is the unit value; treat it as __Any__.
                return Ok(ty_any());
            }

            if let Expr::Symbol(op) = &list[0] {
                match op.as_str() {
                    "quote" => return Ok(ty_any()),
                    "quasiquote" => return Ok(ty_any()),

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

            // Function application
            infer_application(list, env, lex_env, ty_global, ty_env)
        }
    }
}

/// Check that a compiled expression has an expected type, reporting a
/// descriptive error if not.
pub fn check(
    expr: &Expr,
    expected: &Expr,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<(), String> {
    // Special case: lambda body can be checked against Pi type.
    if let (Expr::List(list), Expr::Pi(dom, cod, pi_lex)) = (expr, expected) {
        if matches!(list.first(), Some(Expr::Symbol(s)) if s == "lambda") {
            return check_lambda_against_pi(list, dom, cod, pi_lex, env, lex_env, ty_global, ty_env);
        }
    }

    // Special case: path body checked against PathTy.
    if let (Expr::List(list), Expr::List(path_ty)) = (expr, expected) {
        if path_ty.len() == 2 {
            if let Expr::Symbol(s) = &path_ty[0] {
                if s == "__Path__" {
                    if let Some(Expr::Symbol(op)) = list.first() {
                        if op == "path" {
                            return check_path_against_pathty(list, &path_ty[1], env, lex_env, ty_global, ty_env);
                        }
                    }
                }
            }
        }
    }

    // General case: infer and compare.
    let inferred = infer(expr, env, lex_env, ty_global, ty_env)?;

    // __Any__ on either side means "don't check".
    if is_any(&inferred) || is_any(expected) {
        return Ok(());
    }

    if types_equal(&inferred, expected) {
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
        return Err("type error: if requires at least 2 branches".into());
    }
    // Condition must be numeric (Booleans are Num in this Lisp).
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
        // If both branches have the same type, that's the result type.
        if types_equal(&then_ty, &else_ty) || is_any(&else_ty) {
            Ok(then_ty)
        } else if is_any(&then_ty) {
            Ok(else_ty)
        } else {
            // Allow mismatched branches but warn with __Any__.
            // (A full dependent type system would unify these.)
            Ok(ty_any())
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
    // Infer the type of the value expression (the define result type is the value's type).
    infer(&list[2], env, lex_env, ty_global, ty_env)
}

fn infer_lambda(
    _list: &[Expr],
    _env: &Env,
    _lex_env: &Rc<LexEnv>,
    _ty_global: &TyGlobal,
    _ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // Core lambda: (lambda <arity:Number> <body>)
    // Without explicit parameter types we can only say the result is __Any__.
    Ok(ty_any())
}

fn infer_begin(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
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
    let mut current_ty_env = ty_env.clone();
    if let Expr::List(bindings) = &list[1] {
        for b in bindings {
            if let Expr::List(pair) = b {
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
    // Core path: (path 1.0 body).  The bound variable is the interval ∈ [0,1],
    // so it has type __Num__.  We infer the body type and wrap in __Path__.
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
    let p_ty = infer(&list[1], env, lex_env, ty_global, ty_env)?;
    // t must be numeric (interval point).
    let t_ty = infer(&list[2], env, lex_env, ty_global, ty_env)?;
    if !is_any(&t_ty) && !is_num(&t_ty) {
        return Err(format!(
            "type error: papply interval point must be a number, got {:?}",
            t_ty
        ));
    }
    // Extract the domain type from the path type.
    match &p_ty {
        Expr::List(l) if l.len() == 2 => {
            if let Expr::Symbol(s) = &l[0] {
                if s == "__Path__" {
                    return Ok(l[1].clone());
                }
            }
            Ok(ty_any())
        }
        _ if is_any(&p_ty) => Ok(ty_any()),
        _ => Err(format!(
            "type error: papply requires a path, got {:?}",
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
    // We evaluate the pi-expr to get an actual Pi value, then piapply it.
    let pi_val = eval(&list[1], env, &Rc::new(LexEnv::Empty))
        .unwrap_or(Expr::Symbol("__unknown__".into()));
    match pi_val {
        Expr::Pi(_dom, cod, pi_lex_env) => {
            // Evaluate the argument to substitute into the codomain.
            let v = eval(&list[2], env, &Rc::new(LexEnv::Empty))
                .unwrap_or(Expr::Symbol("__unknown__".into()));
            let new_lex = Rc::new(LexEnv::Node(v, pi_lex_env));
            eval(&cod, env, &new_lex)
                .map_err(|e| format!("type error in piapply codomain: {}", e))
        }
        _ => Ok(ty_any()),
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
    let sigma_val = eval(&list[1], env, &Rc::new(LexEnv::Empty))
        .unwrap_or(Expr::Symbol("__unknown__".into()));
    match sigma_val {
        Expr::Sigma(_dom, cod, sig_lex_env) => {
            let v = eval(&list[2], env, &Rc::new(LexEnv::Empty))
                .unwrap_or(Expr::Symbol("__unknown__".into()));
            let new_lex = Rc::new(LexEnv::Node(v, sig_lex_env));
            eval(&cod, env, &new_lex)
                .map_err(|e| format!("type error in sigmacod: {}", e))
        }
        _ => Ok(ty_any()),
    }
}

fn infer_application(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // Check all argument types (even if we can't verify against parameter types).
    for arg in &list[1..] {
        infer(arg, env, lex_env, ty_global, ty_env)?;
    }
    // The return type of arbitrary function application is __Any__ unless we
    // know the function's type precisely.
    let fn_ty = infer(&list[0], env, lex_env, ty_global, ty_env)?;
    match fn_ty {
        Expr::Pi(_dom, cod, _pi_lex) => {
            // Non-dependent: the codomain doesn't mention the bound variable.
            Ok(*cod)
        }
        _ => Ok(ty_any()),
    }
}

// ---------------------------------------------------------------------------
// Check helpers
// ---------------------------------------------------------------------------

fn check_lambda_against_pi(
    list: &[Expr],
    dom: &Expr,
    cod: &Expr,
    pi_lex: &Rc<LexEnv>,
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<(), String> {
    // The lambda body is checked against the Pi codomain with the domain
    // type pushed onto the type environment.
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
    // The path body is checked against the domain type with __Num__ (the
    // interval variable) pushed onto the type environment.
    let new_ty_env = Rc::new(TyEnv::Node(ty_num(), ty_env.clone()));
    check(&list[2], dom, env, lex_env, ty_global, &new_ty_env)
}

// ---------------------------------------------------------------------------
// Infer the type of an *already-evaluated* Expr value (used for globals).
// ---------------------------------------------------------------------------

fn infer_value_type(v: &Expr) -> Result<Expr, String> {
    match v {
        Expr::Number(_) => Ok(ty_num()),
        Expr::Func(_) => Ok(ty_any()),
        Expr::Lambda(..) => Ok(ty_any()),
        Expr::Macro(..) => Ok(ty_any()),
        Expr::Path(..) => Ok(ty_any()),
        Expr::Pi(..) => Ok(ty_type()),
        Expr::Sigma(..) => Ok(ty_type()),
        Expr::List(l) if l.is_empty() => Ok(ty_any()),
        Expr::List(_) => Ok(ty_any()),
        Expr::Symbol(_) => Ok(ty_any()),
        _ => Ok(ty_any()),
    }
}

// ---------------------------------------------------------------------------
// Top-level type-check driver
// ---------------------------------------------------------------------------

/// Type-check a compiled top-level expression, updating `ty_global` for any
/// `define` or `defmacro` forms encountered.
///
/// Returns `Ok(inferred_type)` or `Err(message)`.
pub fn typecheck_toplevel(
    expr: &Expr,
    env: &Env,
    ty_global: &mut TyGlobal,
) -> Result<Expr, String> {
    let lex_env = Rc::new(LexEnv::Empty);
    let ty_env = Rc::new(TyEnv::Empty);

    // Specially handle define so we can register the type globally.
    if let Expr::List(list) = expr {
        if let Some(Expr::Symbol(op)) = list.first() {
            if op == "define" && list.len() >= 3 {
                if let Expr::Symbol(name) = &list[1] {
                    let ty = infer(&list[2], env, &lex_env, ty_global, &ty_env)?;
                    ty_global.insert(name.clone(), ty.clone());
                    return Ok(ty);
                }
            }
            if op == "defmacro" {
                return Ok(ty_any());
            }
        }
    }

    infer(expr, env, &lex_env, ty_global, &ty_env)
}