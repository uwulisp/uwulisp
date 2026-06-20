> You are an expert Rust systems programmer. The VM currently falls back to the tree-walker when it encounters `Expr::Lambda` values loaded from the environment (e.g. when calling a previously `define`d function). The goal is to eliminate this fallback so that lambda calls stay entirely within the VM, which should make the VM faster than the tree-walker.
>
> ## Current bottleneck
>
> When the VM executes `Op::LoadVar("factorial")` and finds `Expr::Lambda(params, body, env_handle)` in the environment, `expr_to_vm_value` returns `Err("uncompilable: Lambda")`. This causes `vm_eval` to fall back to `tree_eval` for the entire call, defeating the purpose of the VM.
>
> The root cause is in `expr_to_vm_value` (or equivalent) in `machine.rs` or `bytecode.rs` — it does not know how to convert an `Expr::Lambda` that was created by the tree-walker into a `Value::Closure` that the VM can call.
>
> ## What I want you to implement
>
> ### 1. `vm/bytecode.rs` — fix `expr_to_vm_value` for `Expr::Lambda`
>
> When converting `Expr::Lambda(params, body_expr, captured_env)` to a `Value`:
> - Compile the body expression into a `Chunk` using `Compiler::compile`
> - Wrap it as `Value::Closure { params, body_chunk, body_expr, env: captured_env }`
> - Cache the compiled chunk in `CACHE` (the thread-local cache from `cache.rs`) so repeated calls to the same lambda don't recompile
>
> ```rust
> Expr::Lambda(params, body_expr, captured_env) => {
>     let key = format!("lambda:{:?}", body_expr);
>     let chunk = CACHE.with(|c| c.borrow().get_chunk(&key).cloned())
>         .unwrap_or_else(|| {
>             let chunk = Compiler::compile(&body_expr, ...)
>                 .unwrap_or_else(|_| fallback_chunk(&body_expr));
>             CACHE.with(|c| c.borrow_mut().insert_chunk(key.clone(), chunk.clone()));
>             chunk
>         });
>     Ok(Value::Closure {
>         params: params.clone(),
>         body_chunk: Rc::new(chunk),
>         body_expr: body_expr.clone(),
>         env: captured_env,
>     })
> }
> ```
>
> If the lambda body is not compilable (contains `CubicalTerm` etc.), fall back gracefully:
> ```rust
> fn fallback_chunk(body_expr: &Expr) -> Chunk {
>     // A chunk with a single Op::TreeEval(body_expr.clone()) instruction
>     // that tells the VM to hand off to tree_eval for this body
> }
> ```
>
> ### 2. `vm/bytecode.rs` — add `Op::TreeEval`
>
> ```rust
> Op::TreeEval(Expr),  // fall back to tree-walker for this expression
> ```
>
> ### 3. `vm/machine.rs` — handle `Op::TreeEval`
>
> ```rust
> Op::TreeEval(expr) => {
>     // Build the argument bindings from the current frame's env
>     // (params are already bound via StoreVar from the Call dispatch)
>     let result = tree_eval(&expr, self.current_frame().env, self.heap)?;
>     let val = expr_to_vm_value(&result, self.heap)
>         .unwrap_or(Value::List(vec![]));
>     self.stack.push(val);
>     // Immediately return from this frame
>     self.do_return()?;
> }
> ```
>
> ### 4. `vm/machine.rs` — fix `Call` dispatch for `Value::Closure`
>
> Currently `Call` with a `Value::Closure` pushes a new `CallFrame`. This should already work. Verify that:
> - The closure's `env` (`GcHandle`) is used as the parent for the new child frame
> - Parameters are bound correctly via `env_set`
> - `Op::Return` correctly pops the frame and pushes the return value
>
> If any of these are broken for tree-walker-created lambdas (as opposed to VM-compiled ones), fix them now.
>
> ### 5. `vm/compiler.rs` — `is_compilable` should not block lambda calls
>
> Currently `is_compilable` may return `false` for expressions that call a symbol which resolves to `Expr::Lambda` in the env. This is wrong — lambda calls are now handled by the VM via `expr_to_vm_value`. Update `is_compilable` to only return `false` for `Expr::Macro`, not for `Expr::Lambda`.
>
> ### 6. Correctness requirements
>
> All of the following must work entirely within the VM (no tree-walker fallback for the hot path):
>
> ```lisp
> ; direct recursive call
> (define factorial (lambda (n)
>   (if (= n 0) 1 (* n (factorial (- n 1))))))
> (factorial 10)   ; → 3628800
>
> ; mutual recursion
> (define is-even? (lambda (n)
>   (if (= n 0) 1 (is-odd? (- n 1)))))
> (define is-odd? (lambda (n)
>   (if (= n 0) 0 (is-even? (- n 1)))))
> (is-even? 100)   ; → 1
>
> ; higher-order
> (define apply-twice (lambda (f x) (f (f x))))
> (apply-twice (lambda (x) (* x 2)) 3)   ; → 12
>
> ; lambda returned from define, then called
> (define make-adder (lambda (n) (lambda (x) (+ x n))))
> ((make-adder 5) 10)   ; → 15
> ```
>
> All 24 existing tests must continue to pass.
>
> ### 7. Benchmark target
>
> After this change, run:
> ```bash
> time ./target/release/uwulisp-vm hello1.uwu
> time ./target/release/uwulisp-tree hello1.uwu
> ```
> The VM should be **equal to or faster than** the tree-walker on `hello1.uwu`.
>
> ### 8. What NOT to change
>
> - Do not change `CubicalTerm` handling — it still falls back via `Op::TreeEval`
> - Do not change `defmacro` handling
> - Do not change the cache invalidation logic
>
> After implementing, show: the updated `expr_to_vm_value`, the `Op::TreeEval` dispatch, benchmark results before and after, and confirm all 24 tests pass.