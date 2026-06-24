---
title : List
sidebar:
  order: 5
---

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

---

### `map`
Applies a function to each element of a list and returns a new list of results.

```
(map f lst)  →  List
```

---

### `filter`
Returns a new list containing only the elements for which `pred` returns a truthy value.

```
(filter pred lst)  →  List
```

---

### `fold`
Left fold over a list. Calls `f(acc elem)` at each step, seeded with `init`.

```
(fold f init lst)  →  Expr
```
