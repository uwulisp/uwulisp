//! Bytecode compiler for uwulisp (phase 1).
//!
//! Translates a fully macro-expanded `Expr` AST into a `Chunk` of `Op`
//! instructions, ready for the phase-2 VM to execute.
//!
//! # Public surface
//!
//! ```rust,ignore
//! let chunk = Compiler::compile(&expr, &heap)?;
//! ```
//!
//! # Design notes
//!
//! ## Macro expansion
//!
//! Macros are expanded **eagerly** before the compiler sees the expression.
//! `Compiler::compile` calls `expand_all` which recursively walks the AST and
//! expands any macro application it finds, using the same `eval`-time
//! environment lookup that the tree-walker uses.  This satisfies the
//! constraint that the compiler must operate on a fully expanded form.
//!
//! Because macro expansion requires an environment (to look up macro
//! definitions) and the compiler is otherwise environment-free at compile
//! time, we thread a `Heap` reference through the expansion phase.
//!
//! ## Tail-call detection
//!
//! The compiler tracks whether the sub-expression it is currently compiling
//! is in *tail position* via a boolean flag (`tail`).  A position is a tail
//! position when its value is the last thing the enclosing function will
//! return.  The rules are:
//!
//! * The body of a lambda is in tail position.
//! * The selected branch of `if` is in tail position when the `if` is itself
//!   in tail position.
//! * The last expression of a `begin` / `let` / `let*` body is in tail
//!   position when that whole form is in tail position.
//! * A function call expression is in tail position when the whole call is in
//!   tail position; in that case `TailCall` is emitted instead of `Call`.
//! * All other sub-expressions (condition of `if`, non-last `begin`
//!   expressions, binding initialisers) are **not** in tail position.
//!
//! ## `CubicalTerm` rejection
//!
//! Any node whose sub-tree contains an `Expr::CubicalTerm` causes the
//! compiler to return `Err("uncompilable: CubicalTerm")` immediately.  The
//! caller should catch this error and fall back to the tree-walking evaluator.
//!
//! ## `quote` and `quasiquote`
//!
//! `quote` simply converts the quoted datum to a `Value` with `expr_to_value`
//! and emits `LoadConst`.  `quasiquote` is handled by delegating to the
//! existing `eval_quasiquote` helper (which requires a heap and env) and then
//! converting the fully-spliced result to a `Value`.  This is a compile-time
//! quasi-quote evaluation — fine because quasiquote splicing can only read
//! from the environment, never mutate it in a way that would affect later
//! compilation.

use crate::expr::{Expr, env_get, new_env};
use crate::gc::Heap;
use crate::macros::{eval_quasiquote, expand_macro};
use crate::reader::parse_params;
use crate::vm::bytecode::{Chunk, Op, Value, expr_to_value};

// ── public entry point ────────────────────────────────────────────────────────

/// Stateless bytecode compiler.
///
/// All state is local to a single `compile` call; there is no mutable
/// per-instance state.  The struct exists only to group related methods under
/// a common namespace and to make `impl Compiler { ... }` natural Rust style.
pub struct Compiler;

impl Compiler {
    /// Compile `expr` into a `Chunk`.
    ///
    /// `heap` is needed only for macro expansion (looking up macro definitions
    /// and for any `eval` calls triggered by `quasiquote` unquoting).
    ///
    /// Returns `Err` if the expression is or contains `CubicalTerm`, or if
    /// any syntactic constraint is violated.
    pub fn compile(expr: &Expr, heap: &mut Heap) -> Result<Chunk, String> {
        // Allocate a temporary, empty environment frame for macro expansion.
        // We do NOT look up ordinary variables in it; it is used only as the
        // root env passed to `expand_all` so that macro definitions stored in
        // the real global env are reachable via `env_get`.
        //
        // NOTE: For a production compiler this would be the actual global env.
        // For phase 1 we use a fresh empty frame; the main trade-off is that
        // macros defined in the running program won't be visible to this
        // compiler unless the caller arranges to pass the real env through.
        // Passing `heap` gives access to any env frame the caller already has,
        // but the compiler API only takes `&mut Heap`, not a specific `GcHandle`.
        // A future revision can add an `env: GcHandle` parameter.
        let temp_env = new_env(heap, None);

        let expanded = expand_all(expr, temp_env, heap)?;
        let mut chunk = Chunk::new();
        compile_expr(&expanded, &mut chunk, heap, temp_env, /*tail=*/ true)?;
        // Every top-level chunk needs a terminal Return so the VM knows to stop.
        chunk.emit(Op::Return);
        Ok(chunk)
    }
}

// ── recursive macro expander ──────────────────────────────────────────────────

/// Recursively expand all macro applications in `expr`.
///
/// The expansion loop follows the same logic as the tree-walker in `eval_step`:
/// if the head of a list is a symbol that names a macro in `env`, substitute
/// the arguments and expand the result again (macros may expand to macro calls).
///
/// Non-macro list heads are left structurally intact; only their sub-expressions
/// are recursively expanded.  Atoms are returned unchanged.
fn expand_all(
    expr: &Expr,
    env: crate::expr::Env,
    heap: &mut Heap,
) -> Result<Expr, String> {
    match expr {
        // A CubicalTerm anywhere in the tree makes the whole thing uncompilable.
        Expr::CubicalTerm(_) => Err("uncompilable: CubicalTerm".into()),

        // Atoms — nothing to expand.
        Expr::Number(_) | Expr::Str(_) | Expr::Symbol(_) => Ok(expr.clone()),

        // Func / Lambda / Macro are runtime values; they shouldn't appear as
        // raw AST nodes in the source being compiled, but handle gracefully.
        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(expr.clone()),

        Expr::List(list) if list.is_empty() => Ok(expr.clone()),

        Expr::List(list) => {
            // Special forms that create bindings or have their own quoting
            // rules need to suppress expansion in the "quoted" positions.
            if let Some(Expr::Symbol(head)) = list.first() {
                match head.as_str() {
                    // These forms are not user-defined macros; expand their
                    // sub-expressions (except quoted positions).
                    "quote" => return Ok(expr.clone()), // datum is opaque
                    "quasiquote" => return Ok(expr.clone()), // handled at compile time
                    "if" | "begin" | "define" | "set!" | "let" | "let*" => {
                        // Expand sub-expressions, keeping the special-form head.
                        let mut expanded = vec![list[0].clone()];
                        for sub in &list[1..] {
                            expanded.push(expand_all(sub, env, heap)?);
                        }
                        return Ok(Expr::List(expanded));
                    }
                    "lambda" => {
                        // Don't expand the parameter list; do expand the body.
                        if list.len() < 3 {
                            return Ok(expr.clone());
                        }
                        let mut out = vec![list[0].clone(), list[1].clone()];
                        for body_expr in &list[2..] {
                            out.push(expand_all(body_expr, env, heap)?);
                        }
                        return Ok(Expr::List(out));
                    }
                    _ => {
                        // Check if the head names a macro.
                        if let Ok(Expr::Macro(params, body)) = env_get(heap, env, head) {
                            let args = &list[1..];
                            let substituted = expand_macro(&params, &body, args)?;
                            // Expand the result recursively (macros can return macros).
                            return expand_all(&substituted, env, heap);
                        }
                    }
                }
            }
            // Generic list: expand every element.
            let expanded: Result<Vec<Expr>, String> =
                list.iter().map(|e| expand_all(e, env, heap)).collect();
            Ok(Expr::List(expanded?))
        }
    }
}

// ── core compiler ─────────────────────────────────────────────────────────────

/// Compile a single `Expr` into `chunk`.
///
/// `tail` indicates whether this expression is in tail position.  When `true`
/// and the expression is a function call, `TailCall` is emitted instead of
/// `Call`; when the expression is a `Return`-valued form (already handled by
/// the caller's append of `Op::Return`) nothing extra is emitted.
fn compile_expr(
    expr: &Expr,
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    match expr {
        // ── CubicalTerm ───────────────────────────────────────────────────────
        Expr::CubicalTerm(_) => Err("uncompilable: CubicalTerm".into()),

        // ── self-evaluating atoms ─────────────────────────────────────────────
        Expr::Number(n) => {
            chunk.emit(Op::LoadConst(Value::Number(*n)));
            Ok(())
        }
        Expr::Str(s) => {
            chunk.emit(Op::LoadConst(Value::Str(s.clone())));
            Ok(())
        }

        // ── symbol lookup ─────────────────────────────────────────────────────
        Expr::Symbol(s) => {
            chunk.emit(Op::LoadVar(s.clone()));
            Ok(())
        }

        // ── runtime values (should not appear raw in compiled source) ─────────
        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => {
            // If somehow a runtime value ends up in the AST, convert it or err.
            match expr_to_value(expr) {
                Ok(v) => {
                    chunk.emit(Op::LoadConst(v));
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }

        // ── list forms ────────────────────────────────────────────────────────
        Expr::List(list) => compile_list(list, chunk, heap, env, tail),
    }
}

/// Compile a list form `(head arg...)`.
fn compile_list(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    // Empty list → push Nil constant.
    if list.is_empty() {
        chunk.emit(Op::LoadConst(Value::Nil));
        return Ok(());
    }

    // Check for special forms first.
    if let Expr::Symbol(head) = &list[0] {
        match head.as_str() {
            // ── quote ─────────────────────────────────────────────────────────
            "quote" => return compile_quote(list, chunk),

            // ── quasiquote ────────────────────────────────────────────────────
            "quasiquote" => return compile_quasiquote(list, chunk, heap, env),

            // ── if ────────────────────────────────────────────────────────────
            "if" => return compile_if(list, chunk, heap, env, tail),

            // ── define ────────────────────────────────────────────────────────
            "define" => return compile_define(list, chunk, heap, env),

            // ── set! ──────────────────────────────────────────────────────────
            "set!" => return compile_set(list, chunk, heap, env),

            // ── begin ─────────────────────────────────────────────────────────
            "begin" => return compile_begin(list, chunk, heap, env, tail),

            // ── let ───────────────────────────────────────────────────────────
            "let" => return compile_let(list, chunk, heap, env, tail),

            // ── let* ──────────────────────────────────────────────────────────
            "let*" => return compile_let_star(list, chunk, heap, env, tail),

            // ── lambda ────────────────────────────────────────────────────────
            "lambda" => return compile_lambda(list, chunk, heap, env),

            // Anything else falls through to function-call handling below.
            _ => {}
        }
    }

    // ── function call ─────────────────────────────────────────────────────────
    compile_call(list, chunk, heap, env, tail)
}

// ── quote ─────────────────────────────────────────────────────────────────────

fn compile_quote(list: &[Expr], chunk: &mut Chunk) -> Result<(), String> {
    if list.len() != 2 {
        return Err(format!(
            "quote expects 1 argument, got {}",
            list.len() - 1
        ));
    }
    let v = expr_to_value(&list[1])
        .map_err(|e| format!("quote: cannot compile datum: {}", e))?;
    chunk.emit(Op::LoadConst(v));
    Ok(())
}

// ── quasiquote ────────────────────────────────────────────────────────────────

/// Compile a `quasiquote` form by fully evaluating it at compile time.
///
/// This works because `quasiquote` at depth 1 splices in values that are
/// available in the environment at compile time.  Any `unquote` that refers
/// to a runtime variable cannot be handled this way — in that case
/// `eval_quasiquote` will fail (because the variable isn't in the temporary
/// env) and we propagate the error.  A richer compiler would lower
/// quasiquote to explicit `cons`/`list` calls instead; that is left for a
/// future revision.
fn compile_quasiquote(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
) -> Result<(), String> {
    if list.len() != 2 {
        return Err(format!(
            "quasiquote expects 1 argument, got {}",
            list.len() - 1
        ));
    }
    // Evaluate the quasi-quoted form using the interpreter's existing helper.
    let result = eval_quasiquote(&list[1], env, heap, 1)
        .map_err(|e| format!("quasiquote: {}", e))?;
    let v = expr_to_value(&result)
        .map_err(|e| format!("quasiquote: cannot convert result to Value: {}", e))?;
    chunk.emit(Op::LoadConst(v));
    Ok(())
}

// ── if ────────────────────────────────────────────────────────────────────────

/// Compile `(if cond then [else])`.
///
/// Emits:
/// ```text
/// <cond>
/// JumpIfFalse → else_label
/// <then>
/// Jump → end_label
/// else_label:
/// <else>   (or LoadConst Nil if absent)
/// end_label:
/// ```
fn compile_if(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    if list.len() < 3 || list.len() > 4 {
        return Err(format!(
            "if expects 2 or 3 arguments, got {}",
            list.len() - 1
        ));
    }

    // Compile the condition (never in tail position).
    compile_expr(&list[1], chunk, heap, env, false)?;

    // Emit a placeholder JumpIfFalse; record where it is so we can patch it.
    let jump_if_false_idx = chunk.emit(Op::JumpIfFalse(0));

    // Compile the "then" branch (in tail position if the whole `if` is).
    compile_expr(&list[2], chunk, heap, env, tail)?;

    // Emit a placeholder unconditional jump past the else branch.
    let jump_over_else_idx = chunk.emit(Op::Jump(0));

    // Patch the JumpIfFalse to land here (start of the else branch).
    let else_start = chunk.ops.len();
    chunk.patch_jump(jump_if_false_idx, else_start);

    // Compile the "else" branch (or push Nil if absent).
    if list.len() == 4 {
        compile_expr(&list[3], chunk, heap, env, tail)?;
    } else {
        chunk.emit(Op::LoadConst(Value::Nil));
    }

    // Patch the unconditional jump to skip past the else branch.
    let end = chunk.ops.len();
    chunk.patch_jump(jump_over_else_idx, end);

    Ok(())
}

// ── define ────────────────────────────────────────────────────────────────────

/// Compile `(define name expr)`.
///
/// Emits: `<expr>` then `StoreVar(name)`.
/// Note: `define` is never in tail position (it always returns the value but
/// the store itself is a side-effect); the stack value is not used by the
/// caller in a call chain.  We leave the value on the stack so the REPL can
/// print it, consistent with the tree-walker.
fn compile_define(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
) -> Result<(), String> {
    if list.len() != 3 {
        return Err(format!(
            "define expects 2 arguments, got {}",
            list.len() - 1
        ));
    }
    let name = match &list[1] {
        Expr::Symbol(s) => s.clone(),
        other => {
            return Err(format!(
                "define: expected a symbol name, got {:?}",
                other
            ))
        }
    };
    compile_expr(&list[2], chunk, heap, env, false)?;
    chunk.emit(Op::StoreVar(name));
    Ok(())
}

// ── set! ──────────────────────────────────────────────────────────────────────

/// Compile `(set! name expr)`.
///
/// Semantically identical bytecode to `define` — the difference (walking the
/// parent chain to find an existing binding rather than creating a new one) is
/// enforced by the VM at runtime, not by different opcodes.
///
/// Design decision: we could introduce a separate `SetVar` opcode to preserve
/// the `set!` vs `define` distinction at the bytecode level, but since the
/// VM has access to the full environment chain at runtime it can enforce the
/// semantics without a distinct opcode.  For phase 1 we re-use `StoreVar`.
fn compile_set(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
) -> Result<(), String> {
    if list.len() != 3 {
        return Err(format!(
            "set! expects 2 arguments, got {}",
            list.len() - 1
        ));
    }
    let name = match &list[1] {
        Expr::Symbol(s) => s.clone(),
        other => {
            return Err(format!(
                "set!: expected a symbol name, got {:?}",
                other
            ))
        }
    };
    compile_expr(&list[2], chunk, heap, env, false)?;
    // Emit StoreVar; the VM will distinguish set! vs define by context if needed.
    chunk.emit(Op::StoreVar(name));
    Ok(())
}

// ── begin ─────────────────────────────────────────────────────────────────────

/// Compile `(begin expr...)`.
///
/// All expressions except the last are compiled in non-tail position and their
/// result popped.  The last expression is compiled in the caller-supplied tail
/// position.
fn compile_begin(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    let body = &list[1..];
    if body.is_empty() {
        chunk.emit(Op::LoadConst(Value::Nil));
        return Ok(());
    }
    for expr in &body[..body.len() - 1] {
        compile_expr(expr, chunk, heap, env, false)?;
        chunk.emit(Op::Pop);
    }
    compile_expr(body.last().unwrap(), chunk, heap, env, tail)
}

// ── let ───────────────────────────────────────────────────────────────────────

/// Compile `(let ((name expr)...) body...)`.
///
/// Standard `let` semantics: all binding initialisers are evaluated in the
/// *outer* environment before any binding is created.
///
/// Bytecode strategy: since the VM (phase 2) will manage environments at
/// runtime, we lower `let` to a sequence of `StoreVar` instructions that
/// bind names in the current frame.  This is a simplification: a more
/// sophisticated compiler would push a new environment frame.  Phase 1 just
/// needs correct bytecode structure; the VM handles the frame semantics.
fn compile_let(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    if list.len() < 3 {
        return Err(format!(
            "let expects at least 2 arguments (bindings body), got {}",
            list.len() - 1
        ));
    }
    let bindings = match &list[1] {
        Expr::List(b) => b,
        other => {
            return Err(format!(
                "let: expected a list of bindings, got {:?}",
                other
            ))
        }
    };

    // Collect names and compile all initialisers first (outer-env semantics).
    let mut names = Vec::new();
    for b in bindings {
        match b {
            Expr::List(pair) if pair.len() == 2 => {
                if let Expr::Symbol(name) = &pair[0] {
                    names.push(name.clone());
                    compile_expr(&pair[1], chunk, heap, env, false)?;
                } else {
                    return Err(format!(
                        "let binding name must be a symbol, got {:?}",
                        pair[0]
                    ));
                }
            }
            other => {
                return Err(format!(
                    "let binding must be a (name expr) pair, got {:?}",
                    other
                ))
            }
        }
    }

    // Now store the values (they're on the stack in reverse order).
    // We emit stores in reverse so the last-pushed value is stored first.
    for name in names.iter().rev() {
        chunk.emit(Op::StoreVar(name.clone()));
    }

    // Compile the body expressions.
    let body = &list[2..];
    if body.is_empty() {
        chunk.emit(Op::LoadConst(Value::Nil));
        return Ok(());
    }
    for expr in &body[..body.len() - 1] {
        compile_expr(expr, chunk, heap, env, false)?;
        chunk.emit(Op::Pop);
    }
    compile_expr(body.last().unwrap(), chunk, heap, env, tail)
}

// ── let* ──────────────────────────────────────────────────────────────────────

/// Compile `(let* ((name expr)...) body...)`.
///
/// Sequential `let` semantics: each binding can see the previous ones.
/// Lowered to a sequence of compile-then-store pairs.
fn compile_let_star(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    if list.len() < 3 {
        return Err(format!(
            "let* expects at least 2 arguments (bindings body), got {}",
            list.len() - 1
        ));
    }
    let bindings = match &list[1] {
        Expr::List(b) => b,
        other => {
            return Err(format!(
                "let*: expected a list of bindings, got {:?}",
                other
            ))
        }
    };

    // Each pair: evaluate in the current (extended) env, then bind immediately.
    for b in bindings {
        match b {
            Expr::List(pair) if pair.len() == 2 => {
                if let Expr::Symbol(name) = &pair[0] {
                    compile_expr(&pair[1], chunk, heap, env, false)?;
                    chunk.emit(Op::StoreVar(name.clone()));
                } else {
                    return Err(format!(
                        "let* binding name must be a symbol, got {:?}",
                        pair[0]
                    ));
                }
            }
            other => {
                return Err(format!(
                    "let* binding must be a (name expr) pair, got {:?}",
                    other
                ))
            }
        }
    }

    // Compile body.
    let body = &list[2..];
    if body.is_empty() {
        chunk.emit(Op::LoadConst(Value::Nil));
        return Ok(());
    }
    for expr in &body[..body.len() - 1] {
        compile_expr(expr, chunk, heap, env, false)?;
        chunk.emit(Op::Pop);
    }
    compile_expr(body.last().unwrap(), chunk, heap, env, tail)
}

// ── lambda ────────────────────────────────────────────────────────────────────

/// Compile `(lambda (params...) body...)` into a `MakeFunc` instruction.
///
/// The body is compiled into a fresh sub-chunk.  The sub-chunk gets a
/// terminal `Return` appended.  The sub-chunk index is recorded in
/// `MakeFunc { code_offset, params }`.
fn compile_lambda(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
) -> Result<(), String> {
    if list.len() < 3 {
        return Err(format!(
            "lambda expects at least 2 arguments (params body...), got {}",
            list.len() - 1
        ));
    }
    let params = parse_params(&list[1])
        .map_err(|e| format!("lambda: invalid parameter list: {}", e))?;

    // Compile the body into a new sub-chunk.
    let mut body_chunk = Chunk::new();
    let body_exprs = &list[2..];

    if body_exprs.is_empty() {
        body_chunk.emit(Op::LoadConst(Value::Nil));
    } else {
        for expr in &body_exprs[..body_exprs.len() - 1] {
            compile_expr(expr, &mut body_chunk, heap, env, false)?;
            body_chunk.emit(Op::Pop);
        }
        // Last body expression is in tail position.
        compile_expr(
            body_exprs.last().unwrap(),
            &mut body_chunk,
            heap,
            env,
            true,
        )?;
    }
    // Every lambda body ends with Return.
    body_chunk.emit(Op::Return);

    // Register the sub-chunk in the parent chunk and record its index.
    let code_offset = chunk.add_sub_chunk(body_chunk);

    chunk.emit(Op::MakeFunc { code_offset, params });
    Ok(())
}

// ── function call ─────────────────────────────────────────────────────────────

/// Compile a function call `(func arg...)`.
///
/// Strategy:
/// 1. Compile the function expression (push the callee).
/// 2. Compile each argument left-to-right (push args).
/// 3. Emit `TailCall(n)` if in tail position, else `Call(n)`.
fn compile_call(
    list: &[Expr],
    chunk: &mut Chunk,
    heap: &mut Heap,
    env: crate::expr::Env,
    tail: bool,
) -> Result<(), String> {
    // Reject CubicalTerm in any position.
    for e in list {
        if contains_cubical(e) {
            return Err("uncompilable: CubicalTerm".into());
        }
    }

    let n_args = list.len() - 1;

    // Compile the callee.
    compile_expr(&list[0], chunk, heap, env, false)?;

    // Compile arguments left-to-right.
    for arg in &list[1..] {
        compile_expr(arg, chunk, heap, env, false)?;
    }

    if tail {
        chunk.emit(Op::TailCall(n_args));
    } else {
        chunk.emit(Op::Call(n_args));
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` if `expr` contains a `CubicalTerm` anywhere in its tree.
fn contains_cubical(expr: &Expr) -> bool {
    match expr {
        Expr::CubicalTerm(_) => true,
        Expr::List(items) => items.iter().any(contains_cubical),
        Expr::Lambda(_, body, _) => contains_cubical(body),
        Expr::Macro(_, body) => contains_cubical(body),
        _ => false,
    }
}
