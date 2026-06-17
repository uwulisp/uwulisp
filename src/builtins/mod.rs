mod asm;
mod cubical;
mod base;
mod utils;

use crate::env::{Env, new_env};
use crate::expr::Expr;


/// Extracts a number from an Expr, or errors with context.
fn num(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n) => Ok(*n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Extracts a string slice from an Expr::Str, or errors with context.
fn str_arg(e: &Expr) -> Result<&str, String> {
    match e {
        Expr::Str(s) => Ok(s.as_str()),
        other => Err(format!("expected string, got {:?}", other)),
    }
}

/// Renders an Expr for `print`/`display`: strings print as their raw text
/// (no surrounding quotes), everything else uses its normal Debug form.
fn display_str(e: &Expr) -> String {
    match e {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    }
}
// ─────────────────────────────────────────────────────────────────────────────

/// Builds the global environment populated with builtin procedures.
pub fn global_env() -> Env {
    let env = new_env(None);

    base::register_arithmetic(&env);
    base::register_comparisons(&env);
    base::register_lists(&env);
    utils::register_strings(&env);
    utils::register_misc(&env);
    utils::register_file(&env);
    utils::register_io(&env);
    utils::register_os(&env);
    cubical::register_cubical(&env);
    asm::register_assembler(&env);
    asm::register_load_asm(&env);

    env
}