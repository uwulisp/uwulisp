// Environment types/helpers live alongside Expr in expr.rs (since EnvData
// stores Expr values and Env appears in the Expr::Lambda variant). This
// module simply re-exports them under a more conventional name.
//
// Note: `EnvData` is now defined in `gc.rs` (the GC heap owns it), but
// re-exported here through `expr.rs` so that existing import paths like
// `use crate::env::EnvData` continue to work unchanged.
pub use crate::expr::{Env, env_get, env_set, new_env};
// EnvData, GcHandle, and Heap are not re-exported here — nothing in the
// crate imported them via this path.  Import directly from crate::gc.
