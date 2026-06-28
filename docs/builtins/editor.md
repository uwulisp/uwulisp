---
title : Terminal & I/O
sidebar:
  order: 10
---

### `write`

Writes a string to standard output **without** a trailing newline. Returns `()`.

```
(write str)  →  ()
```

```lisp
(write "hello ")  ;; prints "hello " with no newline
```

---

### `writeline`

Writes a string to standard output followed by a newline. Returns `()`.

```
(writeline str)  →  ()
```

```lisp
(writeline "hello")  ;; prints "hello\n"
```

---

### `read-byte`

Reads a single byte from standard input. Returns an integer in `0–255`, or `-1` on end-of-file.

```
(read-byte)  →  Int
```

---

### `raw-mode`

Enables or disables **raw terminal mode** on Unix systems (a no-op on other platforms). In raw mode, input is available byte-by-byte without line buffering, echoing is disabled, and signal-generating characters (`^C`, `^\`, `^Z`) are passed through as data.

```
(raw-mode #t)   ;; enter raw mode
(raw-mode #f)   ;; restore cooked (canonical) mode
```

> **Note:** Always restore cooked mode before your program exits, or the terminal may be left in an unusable state. The `raw-mode` builtin does **not** automatically restore mode on exit.

---

### `terminal-size`

Returns the terminal dimensions as a list `(rows cols)`. When not connected to a terminal (e.g. piped input), defaults to `(24 80)`.

```
(terminal-size)  →  (rows cols)
```

```lisp
(terminal-size)  ;; → (24 80) for example
```

---

### `exit`

Terminates the process immediately with an optional exit code. Code defaults to `0` when omitted.

```
(exit)         ;; exit 0
(exit code)    ;; exit with code
```

---

### `string-ref`

Returns the Unicode codepoint (integer) of the character at the given index, or `-1` if the index is out of range. Indexing is by Unicode character (not byte).

```
(string-ref s index)  →  Int
```

```lisp
(string-ref "abcdef" 2)  ;; → 99  (the codepoint for #\c)
```

---

### `string-split`

Splits a string by a separator string and returns a list of substrings. An empty separator splits by character.

```
(string-split s separator)  →  List of Strings
```

```lisp
(string-split "a,b,c" ",")   ;; → ("a" "b" "c")
(string-split "hello" "")    ;; → ("h" "e" "l" "l" "o")
```

---

### `string-index-of`

Returns the starting index (integer) of the first occurrence of `substr` in `s`, or `-1` if not found.

```
(string-index-of s substr)  →  Int
```

```lisp
(string-index-of "foobar" "bar")  ;; → 3
(string-index-of "foobar" "baz")  ;; → -1
```

---

### `string-contains?`

Returns `#t` if the string contains the given substring, `#f` otherwise.

```
(string-contains? s substr)  →  Bool
```

```lisp
(string-contains? "hello world" "world")  ;; → #t
(string-contains? "hello" "xyz")          ;; → #f
```
