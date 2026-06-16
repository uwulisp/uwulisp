mod base;
mod stdio;
mod cubical;
mod asm;

use crate::env::{new_env, Env};
use crate::expr::Expr;

/// Extracts a number from an Expr, or errors with context.
fn num(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n) => Ok(*n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Builds the global environment populated with builtin procedures.
pub fn global_env() -> Env {
    let env = new_env();

    base::register_arithmetic(&env);
    base::register_comparisons(&env);
    base::register_lists(&env);
    stdio::register_misc(&env);
    cubical::register_intervals(&env);
    cubical::register_pi_types(&env);
    cubical::register_sigma_types(&env);
    cubical::register_glue_types(&env);
    asm::assemble(&env);

    env
}