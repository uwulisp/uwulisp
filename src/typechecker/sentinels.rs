//! Sentinel type constructors and predicates.
//!
//! Sentinels are special `Expr` values used as type-universe markers.
//! They are not user-accessible symbols.
//!
//! | Sentinel             | Meaning                                              |
//! |----------------------|------------------------------------------------------|
//! | `__Num__`            | The type of all numbers.                             |
//! | `__Type__`           | The universe of all types.                           |
//! | `__Any__`            | Top / unknown type used when inference cannot determine more. |
//! | `(__Path__ dom)`     | The type of path values whose endpoints live in `dom`. |
//! | `__GlueType__`       | The type of `GlueType` type-formers.                 |
//! | `(__Glue__ base)`    | The type of a `Glue` intro term with base type `base`. |

use crate::expr::Expr;

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

pub fn ty_num() -> Expr {
    Expr::Symbol("__Num__".into())
}

pub fn ty_type() -> Expr {
    Expr::Symbol("__Type__".into())
}

pub fn ty_any() -> Expr {
    Expr::Symbol("__Any__".into())
}

pub fn ty_path(dom: Expr) -> Expr {
    Expr::List(vec![Expr::Symbol("__Path__".into()), dom])
}

/// `(__GlueType__ base equiv-ty)` — the type of `GlueType` type-formers.
pub fn ty_glue_type() -> Expr {
    Expr::Symbol("__GlueType__".into())
}

/// `(__Glue__ base)` — the type of a `Glue` intro term whose base type is `base`.
pub fn ty_glue(base: Expr) -> Expr {
    Expr::List(vec![Expr::Symbol("__Glue__".into()), base])
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

pub fn is_any(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Any__")
}

pub fn is_num(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Num__")
}

pub fn _is_type_universe(t: &Expr) -> bool {
    matches!(t, Expr::Symbol(s) if s == "__Type__")
}

/// Matches `(__Path__ dom)` and returns `Some(dom)`.
pub fn as_path_ty(t: &Expr) -> Option<&Expr> {
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

/// Matches `(__Glue__ base)` and returns `Some(base)`.
pub fn as_glue_ty(t: &Expr) -> Option<&Expr> {
    if let Expr::List(l) = t {
        if l.len() == 2 {
            if let Expr::Symbol(s) = &l[0] {
                if s == "__Glue__" {
                    return Some(&l[1]);
                }
            }
        }
    }
    None
}