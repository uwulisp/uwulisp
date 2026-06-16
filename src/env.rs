// Environment types/helpers live alongside Expr in expr.rs (since EnvData
// stores Expr values and Env appears in the Expr::Lambda variant). This
// module simply re-exports them under a more conventional name.
pub use crate::expr::{env_get, env_set, new_env, Env, EnvData};