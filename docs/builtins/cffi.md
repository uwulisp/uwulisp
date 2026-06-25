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

> **Note:** `ccall` is a **special form** (not a regular function). Typed argument pairs like `(:ptr p)` are parsed specially — the value expression is evaluated, but the type keyword is left as metadata. Because of this, `ccall` always falls back to the tree-walker and is never compiled to bytecode.

#### `Limitations`

- Maximum 6 arguments (x86-64 SysV ABI register limit).
- Mixed float/integer arguments beyond 6 are not supported; use all-float or all-integer mode.
- Function pointer must use the `extern "C" calling convention.
- No automatic struct/union marshaling — pass structs by pointer as `(:ptr ...)`.

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

---

### `malloc`
Allocates raw C-compatible memory.

```
(malloc size)  →  Int (pointer)
```

`size` is the number of bytes (must be positive). Returns a pointer as an integer, or raises an error if allocation fails.

```lisp
(define p (malloc 8))    ;; allocate 8 bytes
```

---

### `free`
Frees memory allocated with `malloc`.

```
(free ptr)
```

`ptr` is a pointer (integer, previously returned by `malloc`). Returns `()`.

```lisp
(free p)
```

---

### `mem-ref`
Reads a typed value from raw memory.

```
(mem-ref ptr offset type)  →  Int | Float
```

| Parameter | Description |
|-----------|-------------|
| `ptr`     | Pointer (integer) |
| `offset`  | Byte offset from `ptr` (integer) |
| `type`    | Type specifier keyword |

Supported type specifiers:

| Keyword(s) | C type | Size |
|------------|--------|------|
| `:int8` / `:i8` | `int8_t` | 1 byte |
| `:uint8` / `:u8` | `uint8_t` | 1 byte |
| `:int16` / `:i16` | `int16_t` | 2 bytes |
| `:uint16` / `:u16` | `uint16_t` | 2 bytes |
| `:int32` / `:i32` / `:int` | `int32_t` | 4 bytes |
| `:uint32` / `:u32` | `uint32_t` | 4 bytes |
| `:int64` / `:i64` | `int64_t` | 8 bytes |
| `:uint64` / `:u64` | `uint64_t` | 8 bytes |
| `:float` / `:f32` | `float` | 4 bytes |
| `:double` / `:f64` / `:float64` | `double` | 8 bytes |
| `:ptr` | `void*` | 8 bytes (64-bit) |

```lisp
(mem-ref p 0 :int32)      ;; read int32 at offset 0
(mem-ref p 4 :double)     ;; read double at offset 4
```

---

### `mem-set!`
Writes a typed value to raw memory.

```
(mem-set! ptr offset type value)
```

Parameters follow the same scheme as `mem-ref`. The `value` must match the type (integer for `:int*` / `:uint*` / `:ptr`, float for `:float` / `:double`). Returns `()`.

```lisp
(mem-set! p 0 :int32 42)    ;; write 42 as int32 at offset 0
(mem-set! p 4 :double 3.14) ;; write 3.14 as double at offset 4
```

---

### Struct interop example

These primitives let any C struct be read/written by offset. Combined with `ccall` and the `:ptr` type annotation, full C struct interop is possible:

```lisp
;; A C struct: typedef struct { int32_t x; int32_t y; } Point;
;; Compile: gcc -shared -fPIC -o libtest.so test.c

(define lib (lisp-dlopen "./libtest.so"))
(define sum-point (lisp-dlsym lib "sum_point"))
(define create-point (lisp-dlsym lib "create_point"))

;; Self-allocated struct: malloc 8 bytes, set fields, pass to C
(define p (malloc 8))
(mem-set! p 0 :int32 10)
(mem-set! p 4 :int32 20)
(ccall sum-point :int (:ptr p))     ;; → 30

;; C-allocated struct: read fields back
(define p2 (ccall create-point :ptr (:int 100) (:int 200)))
(mem-ref p2 0 :int32)               ;; → 100
(mem-ref p2 4 :int32)               ;; → 200

;; Cleanup
(ccall free-point :void (:ptr p2))
(free p)
```
