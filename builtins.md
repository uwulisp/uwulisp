# Builtin Functions Reference

This document describes all builtin procedures registered in the global environment. Functions are grouped by category.

---

## Arithmetic

### `+`
Sums zero or more numbers.

```
(+ n1 n2 ...)  →  Number
```

Returns `0` when called with no arguments.

---

### `-`
Subtracts numbers. With a single argument, negates it.

```
(- n)           →  Number   ; negation
(- n1 n2 ...)   →  Number   ; subtraction
```

Requires at least one argument.

---

### `*`
Multiplies zero or more numbers.

```
(* n1 n2 ...)  →  Number
```

Returns `1` when called with no arguments.

---

### `/`
Divides the first number by each subsequent number.

```
(/ n1 n2 ...)  →  Number
```

Requires at least one argument. Raises an error on division by zero.

### `%`
modulo operator

```
(% 3 2) → Number
```

Requires two argument and return Number

---

## Comparisons

All numeric comparisons take exactly two number arguments and return `1.0` for true, `0.0` for false.

| Function | Description         |
|----------|---------------------|
| `=`      | Equal               |
| `<`      | Less than           |
| `>`      | Greater than        |
| `<=`     | Less than or equal  |
| `>=`     | Greater than or equal |

```
(= a b)   →  1.0 or 0.0
(< a b)   →  1.0 or 0.0
```

---

### `not`
Logical negation. Returns `1.0` if the argument is falsy, `0.0` otherwise.

```
(not x)  →  1.0 or 0.0
```

Expects exactly one argument.

---

## Lists

### `list`
Constructs a list from its arguments.

```
(list x1 x2 ...)  →  List
```

---

### `car`
Returns the first element of a list.

```
(car lst)  →  Expr
```

Errors on an empty list or a non-list argument.

---

### `cdr`
Returns all elements of a list except the first.

```
(cdr lst)  →  List
```

Errors on an empty list or a non-list argument.

---

### `cons`
Prepends a value to a list (or wraps two values into a list).

```
(cons x lst)  →  List
```

Expects exactly two arguments. If the second argument is a list, `x` is prepended; otherwise both values are collected into a new list.

---

### `null?`
Returns `1.0` if the argument is an empty list, `0.0` otherwise.

```
(null? x)  →  1.0 or 0.0
```

### `map`

```
(map f list)
```

applies `f` to each element, returns a new list of results

### `filter`

```
(filter pred list)
```

keeps elements where `pred` returns truthy

### `fold`

```
(fold f init list)
```
 
left fold, calling `f(acc, elem)` each step, seeded with `init`

---

## Strings

### `string?`
Returns `1.0` if the argument is a string, `0.0` otherwise.

```
(string? x)  →  1.0 or 0.0
```

---

### `string-append`
Concatenates zero or more strings.

```
(string-append s1 s2 ...)  →  String
```

---

### `string-length`
Returns the number of Unicode characters in a string.

```
(string-length s)  →  Number
```

---

### `String comparisons`

All string comparisons take exactly two string arguments and return `1.0` or `0.0`.

| Function      | Description               |
|---------------|---------------------------|
| `string=?`    | Equal                     |
| `string<?`    | Less than (lexicographic) |
| `string>?`    | Greater than              |
| `string<=?`   | Less than or equal        |
| `string>=?`   | Greater than or equal     |

```
(string=? a b)  →  1.0 or 0.0
```

---

### `string->number`
Parses a string as a floating-point number.

```
(string->number s)  →  Number
```

Errors if the string is not a valid number.

---

### `number->string`
Converts a number to its string representation.

```
(number->string n)  →  String
```

---

### `string->symbol`
Converts a string to a symbol.

```
(string->symbol s)  →  Symbol
```

---

### `symbol->string`
Converts a symbol to a string.

```
(symbol->string sym)  →  String
```

---

### `string-upcase`
Returns the string converted to uppercase.

```
(string-upcase s)  →  String
```

---

### `string-downcase`
Returns the string converted to lowercase.

```
(string-downcase s)  →  String
```

---

### `substring`
Extracts a slice of a string by character index (end-exclusive, like Scheme).

```
(substring s start end)  →  String
```

Errors if `start > end` or either index is out of range.

---

## Miscellaneous

### `print`
Prints each argument followed by a space, then a newline, then returns an empty list. Note that a trailing space is emitted after the last argument (not just between arguments).

```
(print x1 x2 ...)  →  ()
```

Strings are printed as raw text (without surrounding quotes); all other values use their debug representation.

### `thread-eval`

Evaluates a source string on a worker OS thread and returns the final expression result.

```
(thread-eval "(+ 1 2)")  →  3
```

The worker gets a fresh global environment. It can use builtins and definitions included in the source string, but it does not share the caller's current variables, functions, or GC heap. Returned values may be numbers, strings, symbols, or lists of those values.

### `parallel-eval`

Evaluates a list of source strings concurrently, one worker thread per string, and returns the results in the same order as the inputs.

```
(parallel-eval (list "(+ 1 2)" "(* 3 4)"))  →  (3 12)
```

Each worker is isolated in the same way as `thread-eval`.

### `read-line`

Reads a single line of input from standard input.

```
(read-line)          →  String
(read-line prompt)   →  String

```

If an optional `prompt` string or expression is provided, it is printed to stdout without a trailing newline before reading input. The returned string strips trailing `\n` and `\r` characters.

---

### `file-read`

Reads the entire contents of a file into a string.

```
(file-read path)  →  String

```

`path` must be a string specifying the file location. Errors if the file cannot be opened or read.

---

### `file-write`

Overwrites a file with the provided string content. Creates the file if it does not exist.

```
(file-write path content)  →  ()

```

Both arguments must be strings. Returns an empty list on success.

---

### `file-append`

Appends string content to the end of a file. Creates the file if it does not exist.

```
(file-append path content)  →  ()

```

Both arguments must be strings. Returns an empty list on success.

---

### `file-exists?`

Checks whether a file or directory exists at the given path.

```
(file-exists? path)  →  1.0 or 0.0

```

`path` must be a string. Returns `1.0` if it exists, `0.0` otherwise.

---

### `file-delete`

Deletes a file from the file system.

```
(file-delete path)  →  ()

```

`path` must be a string. Returns an empty list on success, or raises an error if deletion fails.

---

### `shell`

Executes a command via the system shell (`sh -c`), blocks until completion, and returns the captured standard output.

```
(shell cmd)  →  String

```

`cmd` must be a command string. Standard error is ignored unless redirected within the command string.

---

### `shell-status`

Executes a command via the system shell (`sh -c`), blocks until completion, and returns the exit status code.

```
(shell-status cmd)  →  Number

```

`cmd` must be a command string. Returns the integer exit code (or `-1.0` if the process was terminated by a signal or the exit code cannot be retrieved) represented as a float.

---

## Cubical Type Theory

These builtins provide a surface-level Lisp API over the internal cubical type theory (CTT) kernel. Every cubical builtin returns an `Expr::CubicalTerm` wrapping the corresponding `Term` variant. Arguments that are expected to be cubical terms must themselves be `Expr::CubicalTerm` values.

### `Interval Atoms`

#### `interval-zero`
The interval endpoint `0`.

```
(interval-zero)  →  TInterval(I0)
```

No arguments.

---

#### `interval-one`
The interval endpoint `1`.

```
(interval-one)  →  TInterval(I1)
```

No arguments.

---

#### `interval-var`
An interval variable identified by a numeric index.

```
(interval-var n)  →  TInterval(IVar(n))
```

`n` is cast to `i32`.

---

#### `interval-meet`
The meet (minimum / conjunction) of two interval expressions, normalised to DNF immediately.

```
(interval-meet a b)  →  TCube(dnf)
```

Both arguments must be `TInterval` terms (i.e. produced by `interval-var`, `interval-zero`, or `interval-one`). Passing a pre-normalised `TCube` is an error.

---

#### `interval-join`
The join (maximum / disjunction) of two interval expressions, normalised to DNF immediately.

```
(interval-join a b)  →  TCube(dnf)
```

Both arguments must be `TInterval` terms. Passing a pre-normalised `TCube` is an error.

---

#### `interval-neg`
The negation of an interval expression, normalised to DNF immediately.

```
(interval-neg a)  →  TCube(dnf)
```

Argument must be a `TInterval` term. Passing a pre-normalised `TCube` is an error.

---

### `Variables and Universes`

#### `var`
A de Bruijn-indexed term variable.

```
(var n)  →  TVar(n)
```

`n` is cast to `i32`.

---

#### `univ`
A universe at the given level.

```
(univ level)  →  TUniv(level)
```

`level` is cast to `i32`.

---

#### `interval-type`
The interval type constant `𝕀` itself (not a term of the interval).

```
(interval-type)  →  TIntervalTy
```

No arguments.

---

### `Functions`

#### `lambda`
A term-level lambda abstraction.

```
(clambda name body)  →  TAbs(name, body)
```

`name` must be a symbol; `body` must be a cubical term.

use clambda for prevent variable shadowinig

---

#### `app`
Function application.

```
(app f x)  →  TApp(f, x)
```

Both arguments must be cubical terms.

---

#### `pi`
A dependent function type (Π-type).

```
(pi name domain codomain)  →  TPi(name, domain, codomain)
```

`name` must be a symbol; `domain` and `codomain` must be cubical terms.

---

### `Path Types`

#### `path-type`
The identity/path type between two terms over a given type.

```
(path-type A a b)  →  TPath(A, a, b)
```

All three arguments must be cubical terms.

---

#### `path-lambda`
A path abstraction (binder over the interval).

```
(path-lambda name body)  →  PLam(name, body)
```

`name` must be a symbol; `body` must be a cubical term.

---

#### `path-app`
Applies a path to an interval point.

```
(path-app p i)  →  PApp(p, i)
```

Both arguments must be cubical terms.

---

### `Sigma Types and Pairs`

#### `sigma`
A dependent pair type (Σ-type).

```
(sigma name domain codomain)  →  TSigma(name, domain, codomain)
```

`name` must be a symbol; `domain` and `codomain` must be cubical terms.

---

#### `pair`
A dependent pair value.

```
(pair a b)  →  TPair(a, b)
```

Both arguments must be cubical terms.

---

#### `fst`
Projects the first component of a pair.

```
(fst p)  →  TFst(p)
```

`p` must be a cubical term.

---

#### `snd`
Projects the second component of a pair.

```
(snd p)  →  TSnd(p)
```

`p` must be a cubical term.

---

### `Composition and Transport`

#### `hcomp`
Homogeneous composition. Fills a cube with a given face constraint.

```
(hcomp A phi tube base)  →  THComp(A, phi, tube, base)
```

| Argument | Role |
|----------|------|
| `A`      | The type |
| `phi`    | Face formula |
| `tube`   | The partial element (the tube) |
| `base`   | The base element at `i=0` |

All four arguments must be cubical terms.

---

#### `transport`
Transports a term along a path of types.

```
(transport path x)  →  TTransport(path, x)
```

Both arguments must be cubical terms. `path` is a path in a universe; `x` is the element to transport.

---

### `Equivalences and Univalence`

#### `equiv`
The type of equivalences between two types.

```
(equiv A B)  →  TEquiv(A, B)
```

Both arguments must be cubical terms.

---

#### `make-equiv`
Constructs an equivalence from its components.

```
(make-equiv A B f g eta eps)  →  TMkEquiv(A, B, f, g, eta, eps)
```

| Argument | Role |
|----------|------|
| `A`      | Source type |
| `B`      | Target type |
| `f`      | Forward map `A → B` |
| `g`      | Backward map `B → A` |
| `eta`    | Left inverse homotopy |
| `eps`    | Right inverse homotopy |

All six arguments must be cubical terms.

---

#### `equiv-fwd`
Applies the forward direction of an equivalence to a term.

```
(equiv-fwd e x)  →  TEquivFwd(e, x)
```

Both arguments must be cubical terms.

---

#### `ua`
Univalence: converts an equivalence into a path between types.

```
(ua e)  →  TUa(e)
```

`e` must be a cubical term representing an equivalence.

---

### `Glue Types`

Glue types are used to implement the computational content of univalence.

#### `glue`
Forms a Glue type. The third argument `T` must bundle the equivalent-type family and the equivalence together as a `pair` term.

```
(glue A phi T)  →  TGlue(A, phi, T)
```

`T` should be constructed as `(pair type equiv)`. All three arguments must be cubical terms.

---

#### `glue-elem`
Constructs a term of a Glue type.

```
(glue-elem phi t a)  →  TGlueElem(phi, t, a)
```

| Argument | Role |
|----------|------|
| `phi`    | Face formula |
| `t`      | Element on the glued side |
| `a`      | Underlying element in the base type |

All three arguments must be cubical terms.

---

#### `unglue`
Extracts the underlying base-type element from a glued term.

```
(unglue phi te g)  →  TUnglue(phi, te, g)
```

| Argument | Role |
|----------|------|
| `phi`    | Face formula |
| `te`     | Bundled `(type, equiv)` pair |
| `g`      | The glued term to unglue |

All three arguments must be cubical terms.

---

### `Evaluation and Type-Checking`

#### `ctt-eval`
Normalises a closed cubical term.

```
(ctt-eval t)  →  CubicalTerm
```

Returns the normal form of `t`.

---

#### `ctt-infer`
Infers the type of a closed cubical term.

```
(ctt-infer t)  →  CubicalTerm
```

Returns the inferred type as a cubical term. Errors if type inference fails.

---

#### `ctt-check`
Checks that a term has a given type in the empty context.

```
(ctt-check t ty)  →  1.0
```

Returns `1.0` on success. Raises a Lisp error on type-checking failure.

---

#### `ctt-equal?`
Tests definitional equality of two closed cubical terms.

```
(ctt-equal? t u)  →  1.0 or 0.0
```

Returns `1.0` if `t` and `u` are definitionally equal, `0.0` otherwise.

---

## `Assembler`

### `asm`
Assembles and JIT-executes a list of x86-64 instructions, returning the value left in `RAX` as a number.

```
(asm instructions)  →  Number
```
Reads, parses, assembles, and JIT-executes an external NASM-style x86-64 assembly file, returning the value left in RAX as a number.

```
(load-asm filename)  →  Number
```
StringList is list of filename strings

```
(load-asm-parallel StringList) -> (list)
```

`instructions` must be a list of instruction lists. Each instruction list begins with a mnemonic symbol followed by its operands. The assembled machine code is written to executable memory and called immediately; the `i64` value in `RAX` at the time of `ret` is returned as a Lisp `Number` (cast to `f64`).

**Example:**

```lisp
(asm '(
  (mov rax 0)
  (label loop)
  (add rax 1)
  (cmp rax 5)
  (jne loop)
  (ret)
))
; → 5.0
```

#### `Operand forms`

| Form                  | Syntax                    | Example           |
|-----------------------|---------------------------|-------------------|
| Register              | symbol                    | `rax`, `r8`       |
| Immediate (i32)       | number                    | `42`, `-1`        |
| Memory (base + disp)  | `(mem <reg> <disp>)`      | `(mem rsp -8)`    |

Immediate values must fit in a signed 32-bit integer.

#### `Supported registers`

`rax`, `rcx`, `rdx`, `rbx`, `rsp`, `rbp`, `rsi`, `rdi`, `r8`–`r15` (case-insensitive).

#### `Supported instructions`

**Data movement**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `mov`    | dst src          | Move                     |
| `push`   | src              | Push onto stack          |
| `pop`    | dst              | Pop from stack           |
| `lea`    | dst src          | load effective address   |

**Arithmetic**

| Mnemonic | Operands         | Description                        |
|----------|------------------|------------------------------------|
| `add`    | dst src          | Add                                |
| `sub`    | dst src          | Subtract                           |
| `imul`   | dst src          | Signed multiply (two-operand)      |
| `mul`    | src              | Unsigned multiply (`rax × src`)    |
| `div`    | src              | Unsigned divide (`rax ÷ src`)      |

**Bitwise / shift**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `and`    | dst src          | Bitwise AND              |
| `or`     | dst src          | Bitwise OR               |
| `xor`    | dst src          | Bitwise XOR              |
| `not`    | dst              | Bitwise NOT              |
| `shl`    | dst count        | Shift left               |
| `shr`    | dst count        | Shift right (logical)    |

**Compare / test**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `cmp`    | a b              | Set flags for `a − b`    |
| `test`   | a b              | Set flags for `a & b`    |

**Control flow**

| Mnemonic  | Operand        | Description                        |
|-----------|----------------|------------------------------------|
| `call`    | target         | Call                               |
| `ret`     | —              | Return                             |
| `syscall` | —              | System call                        |
| `label`   | name           | Define a label (symbol)            |
| `jmp`     | label          | Unconditional jump                 |
| `je`      | label          | Jump if equal (ZF=1)               |
| `jne`     | label          | Jump if not equal (ZF=0)           |
| `jl`      | label          | Jump if less (SF≠OF)               |
| `jle`     | label          | Jump if less or equal              |
| `jge`     | label          | Jump if greater or equal           |
| `jg`      | label          | Jump if greater                    |

Errors if an unrecognised mnemonic is encountered, an operand is out of range, or assembly/JIT allocation fails.
