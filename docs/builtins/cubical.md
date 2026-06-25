---
title: Cubical Type Theory
sidebar:
  order: 8
---

Builtins for loading and evaluating cubical type theory files (`.ctt` / `.pic`). Implemented in `src/builtins/mod.rs:79–128` and `src/cubical/mod.rs`.

---

### `ctt-load`
Loads and evaluates a cubical type theory source file.

```
(ctt-load path)  →  (name type value)
```

`path` is a string or symbol naming a `.ctt` or `.pic` file to load. The file is parsed, typechecked, and evaluated, producing a result with three components returned as a list:

| Position | Type | Description |
|----------|------|-------------|
| 0 | `Str` | The exported name of the loaded definition |
| 1 | `CubicalTerm` | The type of the loaded definition |
| 2 | `CubicalTerm` | The value of the loaded definition |

`CubicalTerm` values are opaque — they self-evaluate and are only inspected by cubical builtins. The result can be passed to further cubical operations.

```lisp
;; Load a file defining `Nat` and `plus`
(define result (ctt-load "Nat.pic"))
(define name   (car result))          ;; "plus"
(define type   (car (cdr result)))    ;; CubicalTerm (type)
(define value  (car (cdr (cdr result)))) ;; CubicalTerm (value)
```

---

### `eval-pic`
Evaluates cubical type theory source code from a string.

```
(eval-pic source)  →  (name type value)
```

`source` is a string containing inline CTT source. The return format is identical to `ctt-load`.

```lisp
;; Evaluate an inline cubical program
(define result (eval-pic "
  data Bool =
    | true : Bool
    | false : Bool

  def not : Bool -> Bool =
    \\b. match b return Bool with
      | true => false
      | false => true
"))
(define name   (car result))          ;; "not"
(define type   (car (cdr result)))    ;; CubicalTerm (type)
(define value  (car (cdr (cdr result)))) ;; CubicalTerm (value)
```

> **Note:** The cubical parser uses `data Name = | con : Type` (equals sign, not colon, after the name) and `def name : type = body` (equals sign, not `:=`). Functions are defined with lambda syntax `\args. body`. For more examples see [`test.pic`](/test.pic) and [`Nat.pic`](/Nat.pic).
