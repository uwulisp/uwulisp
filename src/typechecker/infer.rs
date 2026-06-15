//! Type inference — public entry point and all special-form helpers.

use std::rc::Rc;

use crate::env::{env_get, Env};
use crate::eval::eval;
use crate::expr::{Expr, LexEnv, is_sentinel_symbol};
use crate::typechecker::check::check;
use crate::typechecker::equality::types_equal_normalized;
use crate::typechecker::sentinels::{
    as_glue_ty, as_path_ty, is_any, is_num, ty_any, ty_glue, ty_glue_type, ty_num, ty_path,
    ty_type,
};
use crate::typechecker::ty_env::{TyEnv, TyGlobal};
use crate::typechecker::value_type::infer_value_type;

// ---------------------------------------------------------------------------
// Public entry point
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

// ---------------------------------------------------------------------------
// Core dispatch
// ---------------------------------------------------------------------------

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
            if is_sentinel_symbol(s) {
                if s == "__Path__" || s == "__Glue__" {
                    return Ok(ty_any());
                } else {
                    return Ok(ty_type());
                }
            }
            match s.as_str() {
                "+" | "-" | "*" | "/" | "%" => Ok(ty_any()),
                "=" | "<" | ">" | "<=" | ">=" | "not" => Ok(ty_any()),
                "list" | "car" | "cdr" | "cons" | "null?" => Ok(ty_any()),
                "print" | "read" | "write" | "newline" => Ok(ty_any()),
                "i0" | "i1" => Ok(ty_num()),
                "refl" => Ok(ty_any()),
                "pi?" | "sigma?" | "path?" | "glue?" | "glue-type?" => Ok(ty_num()),
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
        Expr::GlueType(..) => Ok(ty_glue_type()),
        Expr::Glue(_, equiv) => {
            // The base type is whatever the equiv maps into. Without evaluating
            // it fully we approximate as __Glue__ __Any__.
            let _ = infer_value_type(equiv);
            Ok(ty_glue(ty_any()))
        }

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

                    "glue-type" => return Ok(ty_glue_type()),
                    "glue" => return infer_glue(list, env, lex_env, ty_global, ty_env),
                    "unglue" => return infer_unglue(list, env, lex_env, ty_global, ty_env),
                    "__Path__" => {
                        if list.len() != 2 {
                            return Err("type error: __Path__ expects 1 argument".into());
                        }
                        check(&list[1], &ty_type(), env, lex_env, ty_global, ty_env)?;
                        return Ok(ty_type());
                    }
                    "__Glue__" => {
                        if list.len() != 2 {
                            return Err("type error: __Glue__ expects 1 argument".into());
                        }
                        check(&list[1], &ty_type(), env, lex_env, ty_global, ty_env)?;
                        return Ok(ty_type());
                    }

                    "defmacro" => return Ok(ty_any()),
                    _ => {}
                }
            }

            infer_application(list, env, lex_env, ty_global, ty_env)
        }
    }
}

// ---------------------------------------------------------------------------
// Special-form helpers
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

fn infer_glue(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (glue val equiv)
    // val  : B   (the fiber-side value)
    // equiv: B → A  (the forward equivalence)
    // result type: __Glue__ __Any__  (we'd need full type annotation to recover A precisely)
    if list.len() != 3 {
        return Err("type error: glue expects (glue <val> <equiv>)".into());
    }
    infer(&list[1], env, lex_env, ty_global, ty_env)?;
    infer(&list[2], env, lex_env, ty_global, ty_env)?;
    Ok(ty_glue(ty_any()))
}

fn infer_unglue(
    list: &[Expr],
    env: &Env,
    lex_env: &Rc<LexEnv>,
    ty_global: &TyGlobal,
    ty_env: &Rc<TyEnv>,
) -> Result<Expr, String> {
    // (unglue g)
    // g : __Glue__ A  →  result : A
    // We try to extract A from the inferred glue type; otherwise __Any__.
    if list.len() != 2 {
        return Err("type error: unglue expects (unglue <glue-term>)".into());
    }
    let g_ty = infer(&list[1], env, lex_env, ty_global, ty_env)?;
    if let Some(base) = as_glue_ty(&g_ty) {
        return Ok(base.clone());
    }
    if is_any(&g_ty) {
        return Ok(ty_any());
    }
    Err(format!(
        "type error: unglue requires a Glue term, got {:?}",
        g_ty
    ))
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