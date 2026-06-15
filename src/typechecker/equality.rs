//! Type equality: structural and normalization-based comparison.
//!
//! Before comparing, [`types_equal_normalized`] attempts to reduce both sides
//! via `eval`. On failure it falls back to pure structural comparison so that
//! type-checking of ground (non-dependent) terms is not disrupted by
//! evaluation errors in unevaluable open terms.

use std::rc::Rc;

use crate::env::Env;
use crate::eval::eval;
use crate::expr::{Expr, LexEnv};

/// Attempt to reduce `e` in the given environment (works for closed terms).
pub fn try_reduce(e: &Expr, env: &Env) -> Expr {
    eval(e, env, &Rc::new(LexEnv::Empty)).unwrap_or_else(|_| e.clone())
}

/// Normalize then compare; falls back to structural equality on error.
pub fn types_equal_normalized(a: &Expr, b: &Expr, env: &Env) -> bool {
    let a_nf = try_reduce(a, env);
    let b_nf = try_reduce(b, env);
    types_equal_structural(&a_nf, &b_nf)
}

pub fn types_equal_structural(a: &Expr, b: &Expr) -> bool {
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