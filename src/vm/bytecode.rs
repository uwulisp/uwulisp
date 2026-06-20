//! Bytecode representation for the uwulisp VM (phase 1: data structures only).
//!
//! # Design notes
//!
//! ## `Value` vs `Expr`
//!
//! `Expr` cannot be used directly as a VM value because:
//! * `Expr::Func` wraps an `Rc<dyn Fn(...)>` which is not `Clone` in a
//!   straightforward way (it _is_ `Clone` via `Rc::clone`, but the trait
//!   bound on `dyn Fn` makes it awkward to use generically).
//! * `Expr::Lambda` carries a `GcHandle` to a live GC frame, coupling the
//!   value type to the interpreter's GC lifecycle.
//! * `Expr::CubicalTerm` is intentionally opaque and uncompilable.
//!
//! `Value` is a clean, fully-owned, `Clone` type that the VM can move around
//! freely.  At the boundaries (loading constants, returning results) we convert
//! between `Value` and `Expr` via `expr_to_value` / `value_to_expr`.
//!
//! ## `Chunk` and `sub_chunks`
//!
//! Each lambda body is compiled into its own `Chunk` stored in the *parent*
//! chunk's `sub_chunks` vector.  `MakeFunc { code_offset, .. }` then refers to
//! that index.  This keeps all code in a contiguous, arena-like structure and
//! avoids heap-allocating separate code objects per lambda at compile time.
//!
//! ## Jump offsets
//!
//! `Jump` and `JumpIfFalse` carry absolute indices into `ops`.  The compiler
//! uses a two-pass technique within each chunk: it emits a placeholder with
//! offset `0` and then back-patches it once the target instruction is known
//! (see `compiler.rs`).

use crate::expr::Expr;

// ── Value ─────────────────────────────────────────────────────────────────────

/// A VM-friendly mirror of `Expr`.
///
/// Every variant is `Clone` and owns its data — no `Rc`, no `GcHandle`.
/// Built-in functions are represented as a named string so they can be stored
/// as constants; the VM will look them up in the environment at runtime rather
/// than embedding the function pointer directly.
///
/// `CubicalTerm` is deliberately absent: the compiler rejects any expression
/// containing one before producing a `Value`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A floating-point number (same as `Expr::Number`).
    Number(f64),
    /// A string literal.
    Str(String),
    /// A symbol / identifier.  Used for quoted symbols and as the string form
    /// of unresolved names in constant data.
    Symbol(String),
    /// A proper list of values (the result of `quote` on a list, etc.).
    List(Vec<Value>),
    /// The empty list `()` — distinct from `List(vec![])` only for clarity;
    /// the two are semantically equivalent.
    Nil,
}

// ── Expr ↔ Value conversion ───────────────────────────────────────────────────

/// Convert an `Expr` to a `Value`.
///
/// Returns `Err` if the expression contains anything that has no `Value`
/// equivalent: `Expr::Func`, `Expr::Lambda`, `Expr::Macro`, or
/// `Expr::CubicalTerm`.
pub fn expr_to_value(expr: &Expr) -> Result<Value, String> {
    match expr {
        Expr::Number(n) => Ok(Value::Number(*n)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Symbol(s) => Ok(Value::Symbol(s.clone())),
        Expr::List(items) => {
            let vs: Result<Vec<Value>, String> = items.iter().map(expr_to_value).collect();
            Ok(Value::List(vs?))
        }
        Expr::Func(_) => Err("cannot convert built-in function to Value".into()),
        Expr::Lambda(..) => Err("cannot convert lambda to Value (use MakeFunc)".into()),
        Expr::Macro(..) => Err("cannot convert macro to Value".into()),
        Expr::CubicalTerm(_) => Err("uncompilable: CubicalTerm".into()),
    }
}

/// Convert a `Value` back to an `Expr`.
///
/// This is always total — every `Value` variant has an `Expr` counterpart.
pub fn value_to_expr(val: Value) -> Expr {
    match val {
        Value::Number(n) => Expr::Number(n),
        Value::Str(s) => Expr::Str(s),
        Value::Symbol(s) => Expr::Symbol(s),
        Value::List(items) => Expr::List(items.into_iter().map(value_to_expr).collect()),
        Value::Nil => Expr::List(vec![]),
    }
}

// ── Op ───────────────────────────────────────────────────────────────────────

/// A single VM instruction.
///
/// All operands are embedded directly in the variant (no separate constant
/// pool), which keeps the instruction set simple for phase 1.  A future
/// optimising VM could split large constants into a pool.
#[derive(Debug, Clone)]
pub enum Op {
    /// Push an immediate constant onto the value stack.
    LoadConst(Value),

    /// Look up a variable by name in the current environment and push the
    /// result.
    LoadVar(String),

    /// Pop the top of the stack and bind it to `name` in the current
    /// environment frame (equivalent to `define`).
    StoreVar(String),

    /// Unconditional jump to an absolute instruction index within this chunk.
    Jump(usize),

    /// Pop the top of the stack; if falsy, jump to the given absolute index.
    JumpIfFalse(usize),

    /// Return from the current call frame: pop and return the top of the stack.
    Return,

    /// Create a closure from `sub_chunks[code_offset]` and the current
    /// environment, then push it onto the stack.
    ///
    /// `code_offset` is an index into the *parent* `Chunk`'s `sub_chunks`
    /// vector, not an instruction offset.
    MakeFunc {
        /// Index into the parent chunk's `sub_chunks` array.
        code_offset: usize,
        /// The formal parameter names the closure expects.
        params: Vec<String>,
    },

    /// Call the function on top of the stack with `n` arguments (which are
    /// already on the stack below the function, pushed left-to-right).
    Call(usize),

    /// Like `Call`, but signals that this is in tail position.  The VM
    /// (phase 2) will reuse the current stack frame instead of allocating a
    /// new one.
    TailCall(usize),

    /// Pop `n` values from the stack (right-to-left, so the first-pushed ends
    /// up at the front of the list) and push the resulting `Value::List`.
    MakeList(usize),

    /// Discard the top of the stack (used to throw away the value of a
    /// non-tail expression in a `begin` sequence or a `define` side-effect).
    Pop,
}

// ── Chunk ────────────────────────────────────────────────────────────────────

/// A compiled unit of code: a flat instruction sequence plus any nested
/// lambda bodies (stored as sub-chunks so the parent can reference them by
/// index via `MakeFunc`).
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The instruction sequence for this code unit.
    pub ops: Vec<Op>,
    /// Lambda bodies compiled from `lambda` forms inside this chunk.
    /// `Op::MakeFunc { code_offset, .. }` indexes into this vector.
    pub sub_chunks: Vec<Chunk>,
}

impl Chunk {
    /// Create a new, empty chunk.
    pub fn new() -> Self {
        Chunk {
            ops: Vec::new(),
            sub_chunks: Vec::new(),
        }
    }

    /// Append an instruction and return its index.
    pub fn emit(&mut self, op: Op) -> usize {
        let idx = self.ops.len();
        self.ops.push(op);
        idx
    }

    /// Back-patch the jump target at `patch_idx` (must be a `Jump` or
    /// `JumpIfFalse` instruction) with the real destination `target`.
    pub fn patch_jump(&mut self, patch_idx: usize, target: usize) {
        match &mut self.ops[patch_idx] {
            Op::Jump(dest) | Op::JumpIfFalse(dest) => *dest = target,
            other => panic!(
                "patch_jump: instruction at {} is not a jump: {:?}",
                patch_idx, other
            ),
        }
    }

    /// Add a sub-chunk (a compiled lambda body) and return its index in
    /// `self.sub_chunks`.
    pub fn add_sub_chunk(&mut self, chunk: Chunk) -> usize {
        let idx = self.sub_chunks.len();
        self.sub_chunks.push(chunk);
        idx
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}
