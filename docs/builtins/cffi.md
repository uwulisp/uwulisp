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
(ccall fn arg1 arg2 ...)  →  Number | Int
```

`fn` must be a function pointer (integer, as returned by `lisp-dlsym`). Accepts 0–6 arguments.

#### `Argument marshaling`

Arguments are automatically marshaled between Lisp and C types:

| Lisp type      | C type      |
|----------------|-------------|
| `Int`          | `int64_t`   |
| `Float`        | `double`    |
| `Bool`         | `int64_t` (1/0) |
| `Str`          | `const char*` (leaked) |
| `Symbol`       | `const char*` (leaked) |

#### `Return type`

- **All arguments are `Float`** → `ccall` uses the double calling convention and returns `Expr::Float`.
- **Otherwise** → `ccall` uses the integer calling convention and returns `Expr::Int`.

String/Symbol arguments are converted to C strings via `CString::into_raw` (the memory is **leaked** — the C function is expected to use the string before returning, or the caller must manage the memory manually). Prefer passing raw pointers as `Int` for non-trivial string lifetimes.

#### `Limitations`

- Maximum 6 arguments (x86-64 SysV ABI register limit).
- Mixed float/integer arguments are not supported in a single call — use all-float or all-integer mode.
- Function pointer must use the `extern "C"` calling convention.
- No automatic struct/union marshaling — pass structs by pointer as `Int`.

#### `Example`

```lisp
;; Call sqrt from libm:
(define lib (lisp-dlopen "libm.so.6"))
(define sqrt (lisp-dlsym lib "sqrt"))
(ccall sqrt 9.0)   ;; → 3.0

;; Integer function (e.g. from libc):
(define libc (lisp-dlopen "libc.so.6"))
(define abs (lisp-dlsym libc "abs"))
(ccall abs -42)     ;; → 42
```
