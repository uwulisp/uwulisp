# map about how **uwulisp** code is executed

### 1. Lexical Analysis and Parsing (Frontend)
The execution of any `uwulisp` expression begins in [src/reader.rs](/src/reader.rs).
* **Tokenization**: The [tokenize](/src/reader.rs#L31) function scans the source code character-by-character into a stream of `Token`s (e.g., parentheses, quotes, strings, and atoms/symbols). Line comments starting with `;` and whitespaces are ignored.
* **AST Construction**: The [parse_all](/src/reader.rs#L191) function converts these tokens into the Abstract Syntax Tree (AST) represented by the [Expr](/src/expr.rs#L26) enum. 
* **Desugaring**: Syntactic abbreviations like `'expr`, `` `expr ``, `,expr`, and `,@expr` are automatically transformed into their standard Lisp representation: `(quote expr)`, `(quasiquote expr)`, `(unquote expr)`, and `(unquote-splicing expr)`.

---

### 2. Execution Routing & AST Verification
Once the AST is built, the main entry point is [eval](/src/eval.rs#L68) in [src/eval.rs](/src/eval.rs). If `uwulisp` is compiled with the `--features vm` compiler flag, it delegates execution to the Virtual Machine via [vm_eval](/src/vm/mod.rs#L83) in [src/vm/mod.rs](/src/vm/mod.rs). 

Before compiling, the compiler runs [is_compilable](/src/vm/compiler.rs#L1008):
1. **Tree-Walker Fallback**: If the expression contains uncompilable constructs—such as macro declarations (`defmacro`) or `CubicalTerm` instances (used in Cubical Type Theory located in [src/cubical/](/src/cubical/))—the compiler rejects it and routes execution to the **Tree-Walking Interpreter**.
2. **VM Compilation**: If the expression is fully compilable, it is sent to the bytecode compiler.

---

### 3. Tree-Walking Interpreter (Stack-Free Fallback)
If routed to the tree-walker, it is evaluated by [eval_tree](/src/eval.rs#L83). 
* **Trampoline Loop**: To avoid growing the Rust call stack, the tree-walker is structured as a trampoline loop using a `Step` enum (`Step::Value` or `Step::TailCall`). 
* **Tail-Call Optimization (TCO)**: When evaluating tail-call positions (such as the branch of an `if`, the last expression of a `begin` block, or a lambda body), `eval_step` returns `Step::TailCall { expr, env }`. The loop then processes the next step iteratively rather than recursing.
* **Lexical Environments and Garbage Collection**: Variables are looked up and bound in environment frames stored on a mark-and-sweep [Heap](/src/gc.rs). If the number of live environment slots exceeds `GC_THRESHOLD` (1024), a garbage collection collection cycle is triggered during closure applications.

---

### 4. Bytecode Compiler & Virtual Machine (VM)
If the expression is compiled for the VM:
* **Macro Expansion**: The compiler first expands macros eagerly using `expand_all` in [src/vm/compiler.rs](/src/vm/compiler.rs).
* **Bytecode Generation**: [Compiler::compile](/src/vm/compiler.rs#L83) compiles the expanded AST into a flat [Chunk](/src/vm/bytecode.rs#L252) of [Op](/src/vm/bytecode.rs#L164) instructions (like `LoadConst`, `LoadVar`, `Jump`, `JumpIfFalse`, `Call`, and `TailCall`). Sub-lambdas are compiled into sub-chunks.
* **Caching**: Chunks and compilation checks are stored in a thread-local `CACHE` in [src/vm/mod.rs](/src/vm/mod.rs) to avoid recompiling structurally identical forms (like loops or repeated function definitions).
* **Execution**: The VM executes the chunk via [VM::run](/src/vm/machine.rs#L292) in [src/vm/machine.rs](/src/vm/machine.rs) using an operand stack (`stack: Vec<VmValue>`) and a frame stack (`frames: Vec<CallFrame>`).
* **VM TCO**: When a `TailCall(N)` opcode is executed, the VM pops the arguments and rewrites the current call frame in-place rather than pushing a new `CallFrame`.

---

### 5. Just-In-Time (JIT) Compiler
When compiled with `--features jit` on an `x86_64` Linux target:
* **Hotness Tracking**: The JIT cache tracks how many times a compiled `Chunk` has been executed.
* **Native Compilation**: Once a hot threshold is met, [JitCompiler::compile_chunk](/src/vm/jit_compiler.rs#L12) uses a custom x86 assembler in [src/tinyasm/](/src/tinyasm/) to compile the VM bytecode directly into native `x86_64` machine code instructions inside a `JitMemory` page.
* **Direct Execution**: The VM executes this machine code directly by invoking the function pointer, passing a mutable ABI pointer (`JitFrame`). Any operations that cannot be handled easily in assembly (like global variable lookups, env allocation, or built-in functions) branch back to C-style helpers defined in [src/vm/jit_abi.rs](/src/vm/jit_abi.rs).

---