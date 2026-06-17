mod asm;
mod cubical;
mod base;
mod utils;

use crate::env::{Env, new_env};
use crate::expr::Expr;
use crate::gc::Heap;

/// Extracts a number from an Expr, or errors with context.
pub(crate) fn num(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n) => Ok(*n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Extracts a string slice from an Expr::Str, or errors with context.
pub(crate) fn str_arg(e: &Expr) -> Result<&str, String> {
    match e {
        Expr::Str(s) => Ok(s.as_str()),
        other => Err(format!("expected string, got {:?}", other)),
    }
}

/// Renders an Expr for `print`/`display`: strings print as their raw text
/// (no surrounding quotes), everything else uses its normal Debug form.
pub(crate) fn display_str(e: &Expr) -> String {
    match e {
        Expr::Str(s) => s.clone(),
        other        => format!("{:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Builds the global environment populated with all builtin procedures.
///
/// ### Signature change from the Rc era
///
/// Previously returned a self-contained `Env` (`Rc<RefCell<EnvData>>`).
/// Now takes the interpreter's single `Heap` so the global frame is
/// allocated on the shared heap — the only place GC-managed frames may live.
/// Returns a `GcHandle` (aliased as `Env`) that the caller holds as the
/// permanent GC root.
pub fn global_env(heap: &mut Heap) -> Env {
    let env = new_env(heap, None);

    base::register_arithmetic(env, heap);
    base::register_comparisons(env, heap);
    base::register_lists(env, heap);
    utils::register_strings(env, heap);
    utils::register_misc(env, heap);
    utils::register_file(env, heap);
    utils::register_io(env, heap);
    utils::register_os(env, heap);
    cubical::register_cubical(env, heap);
    asm::register_assembler(env, heap);
    asm::register_load_asm(env, heap);

    env
}