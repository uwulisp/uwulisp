//! Infer the type of an already-evaluated `Expr` value.
//!
//! Used primarily for globals, where the value has already been reduced by
//! the evaluator and we need to reconstruct a best-effort type from its shape.

use crate::expr::Expr;
use crate::typechecker::sentinels::{
    ty_any, ty_glue, ty_glue_type, ty_num, ty_path, ty_type,
};

/// Infer the type of an *already-evaluated* `Expr` value.
pub fn infer_value_type(v: &Expr) -> Result<Expr, String> {
    match v {
        Expr::Number(_) => Ok(ty_num()),
        Expr::Pi(..) => Ok(ty_type()),
        Expr::Sigma(..) => Ok(ty_type()),
        Expr::Path(_, _) => {
            // We can't easily re-infer without ty_env, so return a generic path type.
            Ok(ty_path(ty_any()))
        }
        Expr::GlueType(..) => Ok(ty_glue_type()),
        Expr::Glue(..) => Ok(ty_glue(ty_any())),
        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(ty_any()),
        Expr::List(l) if l.is_empty() => Ok(ty_any()),
        Expr::List(l) => {
            if let Some(Expr::Symbol(op)) = l.first() {
                if op == "__Path__" || op == "__Glue__" {
                    return Ok(ty_type());
                }
            }
            Ok(ty_any())
        }
        Expr::Symbol(s) => {
            if s == "__Num__" || s == "__Type__" || s == "__Any__" || s == "__GlueType__" {
                Ok(ty_type())
            } else {
                Ok(ty_any())
            }
        }
        Expr::Index(_) => Ok(ty_any()),
    }
}