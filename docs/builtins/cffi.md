---
title : C FFI
sidebar:
  order: 9
---

### `lisp-dlopen`
Loads a shared library at runtime.

```
(lisp-dlopen path)  →  Int
```

`path` is a string or symbol naming the shared library (e.g. `"libm.so.6"`). Returns an opaque handle (integer) that can be passed to `lisp-dlsym` and `lisp-dlclose`.

---

### `lisp-dlsym`
Looks up a symbol (function or global) in a loaded library.

```
(lisp-dlsym handle name)  →  Int
```

`handle` must be a value returned by `lisp-dlopen`. `name` is the symbol name as a string or symbol (e.g. `"sqrt"`). Returns the symbol's address as an integer (function pointer).

---

### `lisp-dlclose`
Closes a shared library handle.

```
(lisp-dlclose handle)
```

Returns `()`.

---

### `ccall`
Calls a C function pointer.

```
(ccall fn [ret-type] arg1 arg2 ...)  →  Number | Int | ()
```

`fn` must be a function pointer (integer, as returned by `lisp-dlsym`). Accepts 0–6 arguments.

#### `Return type annotation`

An optional return-type keyword may be placed as the second argument (before any call arguments):

| Keyword   | C return type | Lisp result    |
|-----------|---------------|----------------|
| `:int`    | `int64_t`     | `Expr::Int`    |
| `:float`  | `double`      | `Expr::Float`  |
| `:void`   | *(none)*      | `Expr::List(vec![])` = `()` |
| `:ptr`    | `void*`       | `Expr::Int`    |

When omitted, the return type is inferred:
- **All arguments are `Float`** → `Expr::Float`
- **Otherwise** → `Expr::Int`

#### `Argument marshaling`

Arguments are automatically marshaled between Lisp and C types:

| Lisp type      | C type        |
|----------------|---------------|
| `Int`          | `int64_t`     |
| `Float`        | `double`      |
| `Bool`         | `int64_t` (1/0) |
| `Str`          | `const char*` |
| `Symbol`       | `const char*` |

String/Symbol arguments are converted to temporary C strings that are freed after the call returns (no memory leak). Prefer passing raw pointers as `Int` for non-trivial string lifetimes.

#### `Per-argument type annotations`

Each call argument may optionally be wrapped in a list `(type value)` to override the inferred type:

```
(ccall fn (:int x) (:float y) ...)
```

Supported type specifiers: `:int`, `:float`, `:ptr`, `:str`, `:string`, `:bool`.

This is useful when a C function expects a different numeric type than the Lisp value would imply.

#### `Mixed float/integer arguments`

Arguments of mixed float and integer types are supported for up to 6 arguments. The calling convention follows the x86-64 SysV ABI (and generalizes to any platform Rust supports) by using `transmute` to the exact `extern "C" fn(…)` signature matching the C function's prototype.

#### `Limitations`

- Maximum 6 arguments (x86-64 SysV ABI register limit).
- Mixed float/integer arguments beyond 6 are not supported; use all-float or all-integer mode.
- Function pointer must use the `extern "C" calling convention.
- No automatic struct/union marshaling — pass structs by pointer as `Int`.

#### `Examples`

```lisp
;; Load libm and call sqrt, inferring float return:
(define lib (lisp-dlopen "libm.so.6"))
(define sqrt (lisp-dlsym lib "sqrt"))
(ccall sqrt 9.0)           ;; → 3.0

;; Explicit return type:
(ccall sqrt :float 9.0)    ;; → 3.0
(ccall sqrt :void 9.0)     ;; → ()

;; Integer function (e.g. from libc):
(define libc (lisp-dlopen "libc.so.6"))
(define abs (lisp-dlsym libc "abs"))
(ccall abs -42)             ;; → 42
(ccall abs :int -42)        ;; → 42

;; Mixed argument types — hypot(double x, double y):
(define hypot (lisp-dlsym lib "hypot"))
(ccall hypot 3.0 4.0)      ;; → 5.0
```
