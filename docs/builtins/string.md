---
title : String
sidebar:
  order: 6
---

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

### String Comparisons

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