---
title : Builtin Functions Reference
sidebar:
  order: 2
---


This document describes all builtin procedures registered in the global environment. Functions are grouped by category.

See also:
- [Terminal & I/O](editor/) — raw terminal control, byte-level I/O, and string utilities for interactive programs.
- [Ahead-of-Time Compilation](aot/) — `aot-compile` and `aot-load` builtins.

### Complex numbers

pi-lisp supports **complex numbers** as a first-class numeric type. You can write them directly using rectangular notation:

- `1+2i`, `3-4i` — rectangular form
- `5i`, `-i`, `i` — pure imaginary / imaginary unit

> **Note:** Because `i` is parsed as the imaginary unit, it cannot be used as a variable name. Use `ii`, `idx`, or another name for loop variables.

All arithmetic (`+`, `-`, `*`, `/`) transparently promotes real numbers to complex when any argument is complex. The builtins `real-part`, `imag-part`, `magnitude`, `angle`, `make-rectangular`, and `make-polar` provide further complex-number operations. See the [Arithmetic](arithmetic/) page for details.

