# Language Grammar Reference

This document describes the syntax and grammar of the cubical Lisp interpreter,
covering all special forms, builtin procedures, type formers, and their
reduction rules.

---

## Lexical Structure

The reader tokenises source text by splitting on whitespace after expanding
parentheses and the quote shorthand into separate tokens.

```
token  ::= '(' | ')' | "'" | number | symbol
number ::= [-]?[0-9]+ ('.' [0-9]+)?
symbol ::= any sequence of non-whitespace, non-paren characters

```

The `'expr` shorthand is expanded by the reader into `(quote expr)` before
parsing. There are no string literals, booleans, or comments in the base
language.

---

## Top-level Grammar

A source file is a sequence of one or more expressions.

```
program  ::= expr+
expr     ::= atom | list
atom     ::= number | symbol
list     ::= '(' expr* ')'

```

---

## Special Forms

Special forms are recognised by their leading symbol and are *not* evaluated
as ordinary function calls. Their arguments may be unevaluated, bound, or
treated structurally.

### `quote`

```
(quote expr)
'expr          ; reader shorthand

```

Returns `expr` unevaluated. No sub-expressions are evaluated.

---

### `quasiquote`, `unquote`, `unquote-splicing`

```
(quasiquote template)
`template                     ; reader shorthand (if supported)

(unquote expr)                ; inside a quasiquote — splices one value
,expr

(unquote-splicing list-expr)  ; inside a quasiquote — splices a list inline
,@list-expr

```

Within a `quasiquote`, sub-expressions are returned literally *except* where
`unquote` or `unquote-splicing` appear at the current nesting depth.
Nested `quasiquote` forms increase the depth; nested `unquote` forms decrease it.

---

### `define`

```
(define name expr)

```

Evaluates `expr` and binds the result to `name` in the global environment.
`name` must be a symbol. Returns the bound value.

---

### `lambda`

**Surface syntax** (before compilation):

```
(lambda (param ...) body)

```

**Core syntax** (after compilation, De Bruijn form):

```
(lambda arity body)

```

Creates a closure over the current lexical environment. Parameters are
referenced in `body` via De Bruijn indices (`#0`, `#1`, …) rather than
names. `arity` is a number literal recording the parameter count.

---

### `if`

```
(if condition then-expr)
(if condition then-expr else-expr)

```

Evaluates `condition`. If it is truthy (non-zero number or non-empty list),
evaluates and returns `then-expr`; otherwise evaluates and returns `else-expr`.
If no `else-expr` is given and the condition is falsy, returns `()`.

Truthiness rules:

* `0` and `()` are **falsy**.
* Every other value is **truthy**.

---

### `let`

**Surface syntax**:

```
(let ((name expr) ...) body ...)

```

Evaluates each binding expression sequentially. Later binding expressions are
evaluated in the environment already extended by all preceding bindings, so a
later RHS *can* refer to an earlier binding by name. Bindings are however
*not* mutually recursive — a binding's own RHS cannot refer to itself (use
`letrec` for that). Evaluates each `body` expression in sequence, returning
the last.

---

### `letrec`

**Surface syntax**:

```
(letrec ((name expr) ...) body ...)

```

Like `let`, but all binding names are in scope for *every* RHS expression and
for the body, enabling mutual recursion and self-referential definitions.
Evaluation pre-allocates slots for every binding before evaluating any RHS;
lambdas defined here close over the fully-resolved environment so recursive
calls work correctly at runtime.

```uwu
(letrec ((even? (lambda (n) (if (= n 0) 1 (odd?  (- n 1)))))
         (odd?  (lambda (n) (if (= n 0) 0 (even? (- n 1))))))
  (even? 10))   ; => 1.0

```

---

### `begin`

```
(begin expr ...)

```

Evaluates each expression in order and returns the value of the last one.
Primarily used to sequence side effects.

---

### `defmacro`

```
(defmacro name (param ...) body)

```

Defines a macro. When `(name arg ...)` is later encountered, the *unevaluated*
argument expressions are substituted for `param ...` in `body`, and the
resulting expression is compiled and evaluated. Macros operate on surface
S-expressions, not on values.

---

## Path / Interval Forms (Cubical)

These forms implement a simplified model of cubical type theory where the
interval `I = [0, 1]` is a numeric range.

### `path`

**Surface syntax**:

```
(path (i) body)

```

**Core syntax**:

```
(path 1.0 body)

```

Creates a path value: a function `I → A` that binds the interval variable `i`
(De Bruijn index `#0` in the compiled body). Analogous to a `lambda` of arity 1
whose domain is the interval `[0, 1]`.

The arity argument must be the literal `1.0`; any other value is an error.
Free variables in `body` are captured from the current lexical environment at
construction time; the interval variable `i` is pushed at index `0` by
`papply` when the path is applied.

---

### `papply`

```
(papply path t)

```

Applies a path at interval point `t`, which must be a number in `[0, 1]`.
Any value in the closed interval is accepted — including interior points such as
`0.5` — making composition, transport, and `hcomp` possible.
Reduces to `body[i := t]`. The endpoints recover the path's boundary:

```
(papply p i0)  ≡  p(0)   ; left endpoint
(papply p i1)  ≡  p(1)   ; right endpoint

```

---

### `funext` — Function Extensionality

```
(funext f g p)

```

Given two functions `f` and `g` of the same Π-type and a pointwise homotopy
`p` between them, produces a `Path` value witnessing that `f` and `g` are
equal as functions.

| Argument | Expected type |
| --- | --- |
| `f` | `Π(x : A), B x` |
| `g` | `Π(x : A), B x` |
| `p` | `Π(x : A), Path(B x) (f x) (g x)` — a function returning, for each `x`, a path from `f x` to `g x` |

The resulting path satisfies the β-rules at its endpoints:

```
(papply (funext f g p) i0)  ≡  f
(papply (funext f g p) i1)  ≡  g

```

At any interior point `i`, applying the result gives `λ x. (papply (p x) i)`.

```uwu
(define add0-path
  (funext (lambda (n) (+ n 0))
          (lambda (n) n)
          (lambda (n) (refl n))))
; (papply add0-path i0) ≡ (lambda (n) (+ n 0))
; (papply add0-path i1) ≡ (lambda (n) n)

```

```
(refl x)

```

Returns the constant path at `x`: a path that ignores its interval argument
and always evaluates to `x`. This is the reflexivity/identity path — evidence
that `x` is equal to itself.

```
(papply (refl x) t)  ≡  x    for all t ∈ [0, 1]

```

**Implementation note:** `x` is captured at De Bruijn index `1` in the path's
lexical environment (not quoted). `papply` pushes the interval value at index
`0`, shifting `x` to index `1`; the body `#1` then retrieves it correctly at
every interval point.

---

### Interval constants (builtins)

```
i0   ; = 0.0, the left  endpoint of I
i1   ; = 1.0, the right endpoint of I

```

---

### `symm` — Path Reversal

```
(symm p)

```

Given `p : Path a b`, returns a path `Path b a` traversing `p` in reverse.

```
(papply (symm p) t)  ≡  (papply p (- 1 t))

```

---

### `trans` — Path Composition

```
(trans p q)

```

Given `p : Path a b` and `q : Path b c`, returns a path `Path a c` that
concatenates them. Uses double-speed scheduling: the first half of `[0, 1]`
runs `p` and the second half runs `q`.

```
(papply (trans p q) t)  ≡  (papply p (* 2 t))        when t < 0.5
                        ≡  (papply q (- (* 2 t) 1))   when t ≥ 0.5

```

The endpoints satisfy `(papply (trans p q) i0) ≡ a` and
`(papply (trans p q) i1) ≡ c`.

---

### `cong` — Congruence (`ap`)

```
(cong f p)

```

Given a function `f : A → B` and `p : Path a b` in `A`, returns a path
`Path (f a) (f b)` in `B`. This is the standard `ap`/congruence principle.

```
(papply (cong f p) t)  ≡  (f (papply p t))

```

```uwu
(define double (lambda (x) (* x 2)))
(define p (path (i) (+ i 1)))          ; path from 1 to 2
(papply (cong double p) i0)             ; => 2.0  (double of p(0) = 1)
(papply (cong double p) i1)             ; => 4.0  (double of p(1) = 2)
(papply (cong double p) 0.5)            ; => 3.0  (double of p(0.5) = 1.5)

```

---

## Dependent Type Forms

### `pi` — Dependent Function Type (Π-type)

**Surface syntax**:

```
(pi (x) domain codomain)

```

**Core syntax** (after compilation):

```
(pi domain codomain)

```

Constructs a Π-type value: the type of functions from `domain` to
`codomain`, where `codomain` may mention the bound variable `x` (De Bruijn
`#0`). For a non-dependent arrow, `codomain` simply ignores `x`.

```uwu
(pi (x) Nat Nat)          ; the non-dependent arrow Nat → Nat
(pi (n) Nat (Vec n))      ; a genuinely dependent type: vectors of length n

```

---

### `piapply`

```
(piapply pi-type value)

```

Instantiates a Π-type at `value`, evaluating the codomain with the bound
variable set to `value`. Returns the *type* of applying a function of this
Π-type to that concrete argument.

```uwu
(piapply (pi (n) Nat (Vec n)) 3)   ; => (Vec 3)

```

---

### `sigma` — Dependent Pair Type (Σ-type)

**Surface syntax**:

```
(sigma (x) domain codomain)

```

**Core syntax**:

```
(sigma domain codomain)

```

Constructs a Σ-type value: the type of pairs `(a, b)` where `a : domain` and
`b : codomain(a)`, where `codomain` may mention the bound variable `x`.

```uwu
(sigma (n) Nat (Vec n))   ; pairs of a length n and a vector of that length

```

---

### `sigmacod`

```
(sigmacod sigma-type value)

```

Instantiates the codomain of a Σ-type at `value` (the first component of a
pair). Returns the *type* of the second component when the first component
equals `value`.

```uwu
(sigmacod (sigma (n) Nat (Vec n)) 3)   ; => (Vec 3)

```

---

## Glue Types (Cubical Equivalence)

Glue types encode equivalences between types, following the cubical notion of
glueing a fiber type onto a base type along a face.

### `glue-type`

```
(glue-type base-type equiv)

```

The Glue type former. `base-type` is the base type `A`; `equiv` is a function
`f : B → A` witnessing an equivalence from fiber type `B` to `A`. Returns a
`GlueType` value.

```uwu
(glue-type Num double)   ; Num glued to Num via the doubling map

```

---

### `glue`

```
(glue val equiv)

```

Introduction form. `val` is a value on the `B`-side (the fiber); `equiv` is
the forward function `f : B → A`. Produces a `Glue` term that records both
the fiber value and the equivalence.

```uwu
(glue 21 double)   ; the fiber value 21, glued via doubling

```

---

### `unglue`

```
(unglue glue-term)

```

Elimination form. Extracts the base-type (`A`-side) image of a `Glue` term
by applying the stored equivalence to the stored fiber value.

**Reduction rule (β):**

```
(unglue (glue v f))  ≡  (f v)

```

```uwu
(unglue (glue 21 double))   ; => 42

```
---

## Inline Assembler (JIT)

The environment includes a low-level `asm` built-in that interfaces with a JIT compiler to emit and execute native x86-64 machine code directly from the interpreter.

### `asm`

```
(asm '(instruction ...))

```

Accepts a **single quoted list** of instructions. It compiles them into an executable buffer, maps memory protections, executes the routine, and returns the numerical value left in the `RAX` register.

> **Note on Safety**: This executes arbitrary machine instructions in the interpreter's process space. Malformed instructions, bad memory addresses, or improper stack management will trigger a segmentation fault or unrecoverable crash.

### Operand Syntax

Instructions support three categories of operands, mapped to native x86-64 parameters:

1. **Registers**: Evaluated from case-insensitive symbols matching standard x86-64 64-bit registers:
`RAX`, `RCX`, `RDX`, `RBX`, `RSP`, `RBP`, `RSI`, `RDI`, `R8`, `R9`, `R10`, `R11`, `R12`, `R13`, `R14`, `R15`.
2. **Immediates**: Evaluated from raw numeric literals. Immediates must fit within a signed 32-bit integer range ($[-2^{31}, 2^{31} - 1]$); numbers outside this range will trigger an out-of-bounds assembly error.
3. **Memory Addresses**: Expressed using the list form `(mem base displacement)` where `base` is a register symbol and `displacement` is an explicit index number (e.g., `(mem rbp -8)` or `(mem rax 0)`). Omitting a base register symbol builds an absolute numeric address.

### Supported Instruction Set

| Instruction Type | Forms |
| --- | --- |
| **Data Movement** | `(mov dest src)`, `(push op)`, `(pop op)` |
| **Arithmetic** | `(add dest src)`, `(sub dest src)`, `(imul dest src)`, `(mul op)`, `(div op)` |
| **Bitwise / Shifts** | `(and dest src)`, `(or dest src)`, `(xor dest src)`, `(not op)`, `(shl dest count)`, `(shr dest count)` |
| **Compare / Test** | `(cmp op1 op2)`, `(test op1 op2)` |
| **Control Flow** | `(call op)`, `(ret)`, `(syscall)` |
| **Labels & Jumps** | `(label name)`, `(jmp name)`, `(je name)`, `(jne name)`, `(jl name)`, `(jle name)`, `(jge name)`, `(jg name)` |


---

## Builtin Procedures

Builtins are ordinary first-class values in the global environment. They are
called like any function: `(name arg ...)`.

### Arithmetic

| Form | Description |
| --- | --- |
| `(+ x ...)` | Sum of zero or more numbers. |
| `(- x)` | Negation. |
| `(- x y ...)` | Left-associative subtraction. |
| `(* x ...)` | Product of zero or more numbers. |
| `(/ x y ...)` | Left-associative division. Errors on divide-by-zero. |

### Comparisons

All comparison operators return `1.0` for true and `0.0` for false.

| Form | Description |
| --- | --- |
| `(= a b)` | Numeric equality. |
| `(< a b)` | Less than. |
| `(> a b)` | Greater than. |
| `(<= a b)` | Less than or equal. |
| `(>= a b)` | Greater than or equal. |
| `(not x)` | `1.0` if `x` is falsy, `0.0` otherwise. |

### Lists

| Form | Description |
| --- | --- |
| `(list x ...)` | Constructs a list from its arguments. |
| `(car lst)` | Returns the first element of a non-empty list. |
| `(cdr lst)` | Returns the list with its first element removed. |
| `(cons x lst)` | Prepends `x` to `lst`. |
| `(null? x)` | `1.0` if `x` is the empty list, `0.0` otherwise. |

### Miscellaneous

| Form | Description |
| --- | --- |
| `(print x ...)` | Prints all arguments with `{}` (Display) formatting, then a newline. Returns `()`. |
| `(read)` | Reads a line of input from standard input, parses it as a single S-expression, and returns it. |
| `(write x)` | Prints the representation of `x` to standard output without a newline, and flushes output. Returns `()`. |
| `(newline)` | Prints a newline to standard output. Returns `()`. |

### Runtime Type Predicates

All predicates take exactly one argument and return `1.0` (true) or `0.0` (false).

| Form | True when argument is… |
| --- | --- |
| `(pi? x)` | A `Pi` type value. |
| `(sigma? x)` | A `Sigma` type value. |
| `(path? x)` | A `Path` value. |
| `(glue? x)` | A `Glue` introduction term. |
| `(glue-type? x)` | A `GlueType` type former. |

---

## De Bruijn Compilation

User-written code uses named variables; the compiler (`compiler.rs`) converts
these to **De Bruijn indices** before evaluation.

* Variables bound by `lambda`, `path`, `pi`, `sigma`, `let`, or `letrec` are replaced
with `#N` where `N` is the number of enclosing binders between the use and
the binding site.
* `#0` refers to the innermost (most recently bound) variable.
* Global names (not in any enclosing binder) are left as symbols and resolved
at runtime from the global environment.

```uwu
; surface syntax
(lambda (x) (lambda (y) (+ x y)))

; compiled core
(lambda 1 (lambda 1 (+ #1 #0)))
;                        ^  ^
;                        |  y  (innermost binder, index 0)
;                        x  (one binder out, index 1)

```

The `let` form is also compiled into De Bruijn form; bindings are pushed
left-to-right, so the last binding is at index `#0` in the body.

---

## Lexical Environments

Evaluation uses two parallel environments:

| Environment | Type | Purpose |
| --- | --- | --- |
| Global (`Env`) | `Rc<RefCell<HashMap<String, Expr>>>` | Global name → value bindings, shared and mutable. |
| Lexical (`LexEnv`) | Linked list of `Expr` | Local variable stack, indexed by De Bruijn index. Immutable; extended by `lambda` application, `path`/`pi`/`sigma`/`let`. |

`LexEnv::get(i)` walks `i` steps down the linked list and returns the value
at that depth. Extended by `lambda` application, `path`/`pi`/`sigma`/`let`/`letrec`.

Two helpers are available for the global environment:

| Function | Signature | Behaviour |
| --- | --- | --- |
| `env_get` | `(&Env, &str) -> Result<Expr, String>` | Returns the value or an *undefined symbol* error. |
| `env_get_opt` | `(&Env, &str) -> Option<Expr>` | Returns `None` instead of an error when the name is absent. Prefer this for optional lookups (e.g. macro dispatch). |

`make_env()` (re-exported from `env.rs`) is an alias for `new_env()` intended
for use in cubical closures that need a throwaway global env to satisfy the
`eval` signature when evaluating closed path bodies.

---

## Type System Sentinels

The typechecker (`typechecker.rs`) represents types as `Expr` values using
sentinel symbols which are accessible from user code for type annotations and signatures.

| Sentinel | Meaning | Evaluation Behavior | Type |
| --- | --- | --- | --- |
| `__Num__` | The type of all number literals. | Evaluates to itself. | `__Type__` |
| `__Type__` | The universe containing `Pi`, `Sigma`, `GlueType`, and `Path` types. | Evaluates to itself. | `__Type__` |
| `__Any__` | Top / unknown type; subsumes everything. | Evaluates to itself. | `__Type__` |
| `(__Path__ T)` | The type of path values whose body has type `T`. | Evaluates structurally: `(__Path__ eval(T))`. | `__Type__` |
| `__GlueType__` | The type of `GlueType` type-former values. | Evaluates to itself. | `__Type__` |
| `(__Glue__ T)` | The type of `Glue` intro terms whose base type is `T`. | Evaluates structurally: `(__Glue__ eval(T))`. | `__Type__` |

Type checking is **bidirectional**: `infer` synthesises a type from an
expression; `check` verifies an expression against an expected type.
`__Any__` on either side of a `check` succeeds unconditionally, making the
system gradually typed.

---

## CLI and REPL Modes

The interpreter supports multiple execution modes:

* **Interactive REPL**: Run `cargo run` without arguments in a terminal.
* **Batch Stdio**: Pipe expressions into `cargo run` (e.g. `echo "(+ 1 2)" | cargo run`).
* **File Mode**: Run `cargo run <filepath>` to execute a Lisp file.
* **Test Mode**: Run `cargo run -- --test` to execute the internal test harness.

---

## Example Programs

```uwu
; Factorial (recursive)
(define fact (lambda (n) (if (< n 1) 1 (* n (fact (- n 1))))))
(fact 10)   ; => 3628800

; Linear interpolation path from 1 to 5
(define interp (path (i) (+ (* (- 1 i) 1) (* i 5))))
(papply interp 0.5)   ; => 3.0

; Dependent vector-length type
(define vec-type (pi (n) Nat (* n n)))
(piapply vec-type 4)   ; => 16

; Glue: doubling equivalence
(define double (lambda (x) (* x 2)))
(define gv (glue 21 double))
(unglue gv)   ; => 42

; Path of Glue terms, unglued at each endpoint
(define gpath (path (i) (glue (* i 10) double)))
(unglue (papply gpath 0.0))   ; => 0.0
(unglue (papply gpath 0.5))   ; => 10.0
(unglue (papply gpath 1.0))   ; => 20.0

; letrec: mutually recursive even/odd predicates
(letrec ((even? (lambda (n) (if (= n 0) 1 (odd?  (- n 1)))))
         (odd?  (lambda (n) (if (= n 0) 0 (even? (- n 1))))))
  (list (even? 4) (odd? 3)))   ; => (1.0 1.0)

; let: sequential bindings (later RHSes see earlier bindings)
(let ((x 3)
      (y (* x 2)))   ; x is already bound here
  (+ x y))           ; => 9.0

; symm: reverse a path
(define p (path (i) (+ i 1)))    ; path from 1 to 2
(papply (symm p) i0)              ; => 2.0
(papply (symm p) i1)              ; => 1.0

; trans: concatenate two paths
(define p1 (path (i) i))          ; path from 0 to 1
(define p2 (path (i) (+ i 1)))    ; path from 1 to 2
(papply (trans p1 p2) i0)          ; => 0.0
(papply (trans p1 p2) 0.5)         ; => 1.0
(papply (trans p1 p2) i1)          ; => 2.0

; cong: apply a function pointwise along a path
(define double (lambda (x) (* x 2)))
(papply (cong double p1) 0.5)      ; => 1.0  (double of 0.5)

; funext: path between definitionally equal functions
(define add0-path
  (funext (lambda (n) (+ n 0))
          (lambda (n) n)
          (lambda (n) (refl n))))
(papply add0-path i0)   ; => (lambda 1 (+ #0 0))  — recovers the left function
(papply add0-path i1)   ; => (lambda 1 #0)        — recovers the right function

; JIT Assembler Loop: Counts up to 5 and returns RAX
(asm '(
  (mov rax 0)
  (label loop)
  (add rax 1)
  (cmp rax 5)
  (jne loop)
  (ret)
)) ; => 5.0

```