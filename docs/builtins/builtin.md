---
title : Builtin Functions Reference
sidebar:
  order: 2
---


This document describes all builtin procedures registered in the global environment. Functions are grouped by category.

See also:
- [Terminal & I/O](editor/) — raw terminal control, byte-level I/O, and string utilities for interactive programs.
- [Ahead-of-Time Compilation](aot/) — `aot-compile` and `aot-load` builtins.
- [CLOS Object System](clos/) — object-oriented programming with classes, generic functions, and multiple dispatch.

### Complex numbers

pi-lisp supports **complex numbers** as a first-class numeric type. You can write them directly using rectangular notation:

- `1+2i`, `3-4i` — rectangular form
- `5i`, `-i`, `+i` — pure imaginary / imaginary unit

> **Note:** Bare `i` is now parsed as a **symbol**, so it can be used as a variable name. Use `+i` or `-i` for the imaginary unit.

All arithmetic (`+`, `-`, `*`, `/`) transparently promotes real numbers to complex when any argument is complex. The builtins `real-part`, `imag-part`, `magnitude`, `angle`, `make-rectangular`, and `make-polar` provide further complex-number operations. See the [Arithmetic](arithmetic/) page for details.

### List predicate

| Function | Signature | Description |
|----------|-----------|-------------|
| `list?` | `(list? x) → Bool` | Returns `#t` if the argument is a list (including the empty list) |

### CLOS builtins

Primitives for the CLOS object system (`lib/clos.pi`):

| Function | Signature | Description |
|----------|-----------|-------------|
| `class-of` | `(class-of obj) → Symbol` | Returns the class name of any value |
| `subtypep` | `(subtypep child parent) → Bool` | Checks subtype relationship |
| `clos-instance?` | `(clos-instance? obj) → Bool` | Predicate for CLOS instances |

See the [CLOS documentation](clos/) for full details.

