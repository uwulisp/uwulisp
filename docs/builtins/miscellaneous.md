---
title : Miscellaneous
sidebar:
  order: 7
---

### `print`
Prints each argument followed by a space, then a newline, then returns an empty list. Note that a trailing space is emitted after the last argument (not just between arguments).

```
(print x1 x2 ...)  →  ()
```

Strings are printed as raw text (without surrounding quotes); all other values use their debug representation.

### `thread-eval`

Evaluates a source string on a worker OS thread and returns the final expression result.

```
(thread-eval "(+ 1 2)")  →  3
```

The worker gets a fresh global environment. It can use builtins and definitions included in the source string, but it does not share the caller's current variables, functions, or GC heap. Returned values may be numbers, strings, symbols, or lists of those values.

### `parallel-eval`

Evaluates a list of source strings concurrently, one worker thread per string, and returns the results in the same order as the inputs.

```
(parallel-eval (list "(+ 1 2)" "(* 3 4)"))  →  (3 12)
```

Each worker is isolated in the same way as `thread-eval`.

### `read-line`

Reads a single line of input from standard input.

```
(read-line)          →  String
(read-line prompt)   →  String

```

If an optional `prompt` string or expression is provided, it is printed to stdout without a trailing newline before reading input. The returned string strips trailing `\n` and `\r` characters.

---

### `file-read`

Reads the entire contents of a file into a string.

```
(file-read path)  →  String

```

`path` must be a string specifying the file location. Errors if the file cannot be opened or read.

---

### `file-write`

Overwrites a file with the provided string content. Creates the file if it does not exist.

```
(file-write path content)  →  ()

```

Both arguments must be strings. Returns an empty list on success.

---

### `file-append`

Appends string content to the end of a file. Creates the file if it does not exist.

```
(file-append path content)  →  ()

```

Both arguments must be strings. Returns an empty list on success.

---

### `file-exists?`

Checks whether a file or directory exists at the given path.

```
(file-exists? path)  →  1.0 or 0.0

```

`path` must be a string. Returns `1.0` if it exists, `0.0` otherwise.

---

### `file-delete`

Deletes a file from the file system.

```
(file-delete path)  →  ()

```

`path` must be a string. Returns an empty list on success, or raises an error if deletion fails.

---

### `shell`

Executes a command via the system shell (`sh -c`), blocks until completion, and returns the captured standard output.

```
(shell cmd)  →  String

```

`cmd` must be a command string. Standard error is ignored unless redirected within the command string.

---

### `shell-status`

Executes a command via the system shell (`sh -c`), blocks until completion, and returns the exit status code.

```
(shell-status cmd)  →  Number

```

`cmd` must be a command string. Returns the integer exit code (or `-1.0` if the process was terminated by a signal or the exit code cannot be retrieved) represented as a float.