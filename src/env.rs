// Environment types/helpers live alongside Expr in expr.rs (since EnvData
// stores Expr values and Env appears in the Expr::Lambda variant). This
// module simply re-exports them under a more conventional name.
pub use crate::expr::{env_get, env_get_opt, env_set, new_env, Env};

/// Alias for `new_env` used by cubical.rs closures that need a throwaway
/// global env to satisfy the `eval` signature when evaluating closed path
/// bodies (i.e. bodies whose only free variable is the interval, which is
/// supplied via the lexical env rather than the global env).
#[inline(always)]
pub fn make_env() -> Env {
    new_env()
}