---
title : Comparisons
sidebar:
  order: 4
---

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
