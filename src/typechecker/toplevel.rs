//! Top-level type-checking driver.
//!
//! Handles `define`, `defmacro`, and `begin` forms at the top level,
//! updating `ty_global` for each binding as it is encountered.

use std::rc::Rc;

use crate::env::Env;
use crate::expr::{Expr, LexEnv};
use crate::typechecker::infer::infer;
use crate::typechecker::sentinels::ty_any;
use crate::typechecker::ty_env::{TyEnv, TyGlobal};

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
                                    Some(old) => {
                                        ty_global.insert(name.clone(), old);
                                    }
                                    None => {
                                        ty_global.remove(name);
                                    }
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