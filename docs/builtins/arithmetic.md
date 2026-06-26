---
title : Arithmetic
sidebar:
  order: 3
---

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

Requires at least one argument. Negation of `i64::MIN` (`-9223372036854775808`) raises an overflow error.

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
Returns the remainder of dividing the first number by the second.

```
(% n1 n2)  →  Number
```

Requires exactly two arguments.
