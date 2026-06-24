---
title : Assembly
sidebar:
  order: 8
---

### `asm`
Assembles and JIT-executes a list of x86-64 instructions, returning the value left in `RAX` as a number.

```
(asm instructions)  →  Number
```
Reads, parses, assembles, and JIT-executes an external NASM-style x86-64 assembly file, returning the value left in RAX as a number.

```
(load-asm filename)  →  Number
```
StringList is list of filename strings

```
(load-asm-parallel StringList) -> (list)
```

`instructions` must be a list of instruction lists. Each instruction list begins with a mnemonic symbol followed by its operands. The assembled machine code is written to executable memory and called immediately; the `i64` value in `RAX` at the time of `ret` is returned as a Lisp `Number` (cast to `f64`).

**Example:**

```lisp
(asm '(
  (mov rax 0)
  (label loop)
  (add rax 1)
  (cmp rax 5)
  (jne loop)
  (ret)
))
; → 5.0
```

#### `Operand forms`

| Form                  | Syntax                    | Example           |
|-----------------------|---------------------------|-------------------|
| Register              | symbol                    | `rax`, `r8`       |
| Immediate (i32)       | number                    | `42`, `-1`        |
| Memory (base + disp)  | `(mem <reg> <disp>)`      | `(mem rsp -8)`    |

Immediate values must fit in a signed 32-bit integer.

#### `Supported registers`

`rax`, `rcx`, `rdx`, `rbx`, `rsp`, `rbp`, `rsi`, `rdi`, `r8`–`r15` (case-insensitive).

#### `Supported instructions`

**Data movement**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `mov`    | dst src          | Move                     |
| `push`   | src              | Push onto stack          |
| `pop`    | dst              | Pop from stack           |
| `lea`    | dst src          | load effective address   |

**Arithmetic**

| Mnemonic | Operands         | Description                        |
|----------|------------------|------------------------------------|
| `add`    | dst src          | Add                                |
| `sub`    | dst src          | Subtract                           |
| `imul`   | dst src          | Signed multiply (two-operand)      |
| `mul`    | src              | Unsigned multiply (`rax × src`)    |
| `div`    | src              | Unsigned divide (`rax ÷ src`)      |

**Bitwise / shift**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `and`    | dst src          | Bitwise AND              |
| `or`     | dst src          | Bitwise OR               |
| `xor`    | dst src          | Bitwise XOR              |
| `not`    | dst              | Bitwise NOT              |
| `shl`    | dst count        | Shift left               |
| `shr`    | dst count        | Shift right (logical)    |

**Compare / test**

| Mnemonic | Operands         | Description              |
|----------|------------------|--------------------------|
| `cmp`    | a b              | Set flags for `a − b`    |
| `test`   | a b              | Set flags for `a & b`    |

**Control flow**

| Mnemonic  | Operand        | Description                        |
|-----------|----------------|------------------------------------|
| `call`    | target         | Call                               |
| `ret`     | —              | Return                             |
| `syscall` | —              | System call                        |
| `label`   | name           | Define a label (symbol)            |
| `jmp`     | label          | Unconditional jump                 |
| `je`      | label          | Jump if equal (ZF=1)               |
| `jne`     | label          | Jump if not equal (ZF=0)           |
| `jl`      | label          | Jump if less (SF≠OF)               |
| `jle`     | label          | Jump if less or equal              |
| `jge`     | label          | Jump if greater or equal           |
| `jg`      | label          | Jump if greater                    |

Errors if an unrecognised mnemonic is encountered, an operand is out of range, or assembly/JIT allocation fails.