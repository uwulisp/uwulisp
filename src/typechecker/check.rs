//! Type checking — bidirectional checking mode and helpers.

use std::rc::Rc;

use crate::env::Env;
use crate::expr::{Expr, LexEnv};
use crate::typechecker::equality::types_equal_normalized;
use crate::typechecker::infer::infer;
use crate::typechecker::sentinels::{as_path_ty, is_any, ty_num};
use crate::typechecker::ty_env::{TyEnv, TyGlobal};

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