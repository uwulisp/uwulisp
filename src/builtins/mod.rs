#[cfg(all(feature = "jit", target_arch = "x86_64"))]
mod asm;
mod base;
mod network;
mod utils;

use std::rc::Rc;

use crate::cubical;
use crate::env::{Env, new_env, env_set};
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
        other => format!("{:?}", other),
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
    base::register_higher_order(env, heap);
    utils::register_strings(env, heap);
    utils::register_misc(env, heap);
    utils::register_threading(env, heap);
    utils::register_file(env, heap);
    utils::register_io(env, heap);
    utils::register_os(env, heap);
    register_load_ctt(env, heap);
    network::register_network(env, heap);
    #[cfg(all(feature = "jit", target_arch = "x86_64"))]
    asm::register_assembler(env, heap);
    #[cfg(all(feature = "jit", target_arch = "x86_64"))]
    asm::register_load_asm(env, heap);
    #[cfg(all(feature = "jit", target_arch = "x86_64"))]
    asm::register_load_asm_parallel(env, heap);

    env
}

fn register_load_ctt(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "ctt-load".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err(format!("ctt-load: expected 1 argument, got {}", args.len()));
            }
            let filename = match &args[0] {
                Expr::Str(s) => s.clone(),
                Expr::Symbol(s) => s.clone(),
                other => {
                    return Err(format!(
                        "ctt-load: filename must be a string or symbol, got {:?}",
                        other
                    ));
                }
            };
            let output = cubical::run(&filename).map_err(|e| e.to_string())?;
            Ok(Expr::List(vec![
                Expr::Str(output.name),
                Expr::CubicalTerm(Box::new(output.ty)),
                Expr::CubicalTerm(Box::new(output.value)),
            ]))
        })),
    );
}
