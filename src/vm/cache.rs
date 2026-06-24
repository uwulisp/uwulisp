//! Compile cache for the pilisp VM.
//!
//! # Overview
//!
//! [`CompileCache`] stores two independent maps keyed by a stable string
//! representation of an [`Expr`]:
//!
//! * **`chunks`** — maps `expr_key → Chunk`.  Once compiled, a chunk is
//!   reused on every subsequent call with a structurally identical expression.
//!   This map is *never* invalidated: compiled chunks are pure functions of the
//!   source expression and do not depend on runtime environment *values*.
//!   Variable lookups happen at VM runtime via `LoadVar`, not at compile time.
//!
//! * **`compilable`** — maps `expr_key → bool` (result of `is_compilable`).
//!   This map *is* invalidated whenever a new macro is defined via `defmacro`,
//!   because new macros change what `is_compilable` returns for other expressions
//!   (a symbol that used to be a plain function call is suddenly a macro call,
//!   which the compiler cannot handle directly).
//!
//! # Cache key
//!
//! The key is `format!("{:?}", expr)`.  Two calls with structurally identical
//! expressions (same AST) will produce the same key.  This is not perfect
//! (e.g. two lambdas with different captured-env GcHandles look the same in
//! the Debug output), but it is correct and cheap for the typical top-level
//! `define` / arithmetic patterns that dominate a Lisp workload.

use crate::expr::Expr;
use crate::vm::bytecode::Chunk;
use std::collections::HashMap;

/// Two-level compile cache: `is_compilable` results + compiled `Chunk`s.
pub struct CompileCache {
    /// expr_key → compiled Chunk.
    /// Never cleared — compiled code is environment-value-independent.
    pub chunks: HashMap<String, Chunk>,
    /// expr_key → is_compilable result.
    /// Cleared on every `defmacro` because new macros alter compilability.
    pub compilable: HashMap<String, bool>,
}

impl CompileCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        CompileCache {
            chunks: HashMap::new(),
            compilable: HashMap::new(),
        }
    }

    /// Stable cache key: the `Debug` format of an `Expr`.
    ///
    /// Two structurally identical expressions produce the same key, making it
    /// suitable for caching compilation results that are independent of the
    /// runtime environment.
    pub fn key(expr: &Expr) -> String {
        format!("{:?}", expr)
    }

    /// Look up a cached `Chunk` for the given key.
    pub fn get_chunk(&self, key: &str) -> Option<&Chunk> {
        self.chunks.get(key)
    }

    /// Store a compiled `Chunk` under the given key.
    pub fn insert_chunk(&mut self, key: String, chunk: Chunk) {
        self.chunks.insert(key, chunk);
    }

    /// Look up a cached `is_compilable` result for the given key.
    pub fn get_compilable(&self, key: &str) -> Option<bool> {
        self.compilable.get(key).copied()
    }

    /// Store an `is_compilable` result under the given key.
    pub fn insert_compilable(&mut self, key: String, result: bool) {
        self.compilable.insert(key, result);
    }

    /// Invalidate the `compilable` cache.
    ///
    /// Must be called whenever a new macro is defined via `defmacro`, because
    /// new macros change what `is_compilable` returns for other expressions.
    /// The `chunks` cache is *not* cleared here — compiled chunks remain valid.
    pub fn invalidate_compilable(&mut self) {
        self.compilable.clear();
    }
}

impl Default for CompileCache {
    fn default() -> Self {
        Self::new()
    }
}
