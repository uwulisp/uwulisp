//! Bidirectional type checker for the Lisp/cubical interpreter.
//!
//! This module implements a simple bidirectional type checker that works
//! alongside the evaluator. It operates on *compiled* (De Bruijn) expressions
//! and uses two environments:
//!
//! - `Env` / `TyGlobal`: maps global names → their types (Expr values).
//! - `TyEnv`: a linked-list of local-variable types, parallel to `LexEnv`.
//!
//! ## Supported forms
//!
//! | Expression            | Inference | Checking |
//! |-----------------------|-----------|----------|
//! | Number literal        | ✓ (Num)   | ✓        |
//! | Symbol (global var)   | ✓         | ✓        |
//! | Index (local var)     | ✓         | ✓        |
//! | `(lambda arity body)` | ✓ (Pi)    | ✓ against Pi |
//! | `(path 1.0 body)`     | ✓         | ✓ against PathTy |
//! | `(pi dom cod)`        | ✓ (Type)  | ✓        |
//! | `(sigma dom cod)`     | ✓ (Type)  | ✓        |
//! | `(if c t e)`          | ✓ (join)  | ✓        |
//! | `(let binds body)`    | ✓         | ✓        |
//! | `(begin e…)`          | ✓         | ✓        |
//! | `(papply p t)`        | ✓         | ✓        |
//! | `(piapply f v)`       | ✓         | ✓        |
//! | `(sigmacod s v)`      | ✓         | ✓        |
//! | Function application  | ✓         | ✓        |
//!
//! ## Type universe
//!
//! Types are themselves `Expr` values evaluated at type-check time.
//! We use sentinel symbols (not user-accessible):
//!
//! - `__Num__`       — the type of all numbers.
//! - `__Type__`      — the universe of all types (Pi, Sigma, Path types live here).
//! - `__Path__ dom`  — the type of path values whose endpoints live in `dom`.
//! - `__Any__`       — a top/"unknown" type used when inference cannot determine more.
//!
//! ## Module structure
//!
//! | File             | Contents                                         |
//! |------------------|--------------------------------------------------|
//! | `ty_env.rs`      | [`TyEnv`] and [`TyGlobal`] types                 |
//! | `sentinels.rs`   | Sentinel constructors and predicates             |
//! | `equality.rs`    | Type equality (structural + normalized)          |
//! | `value_type.rs`  | [`infer_value_type`] for already-evaluated exprs |
//! | `infer.rs`       | [`infer`] and all special-form helpers           |
//! | `check.rs`       | [`check`] and check helpers                      |
//! | `toplevel.rs`    | [`typecheck_toplevel`] driver                    |

pub mod check;
pub mod equality;
pub mod infer;
pub mod sentinels;
pub mod toplevel;
pub mod ty_env;
pub mod value_type;

// Convenience re-exports for callers that previously used the flat module.
pub use check::check;
pub use infer::infer;
pub use toplevel::typecheck_toplevel;
pub use ty_env::{TyEnv, TyGlobal};