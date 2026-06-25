---
title : Assembly
sidebar:
  order: 8
---

### `asm`
Assembles and JIT-executes a list of S-expression x86-64 instructions, returning the value left in `RAX`.

```
(asm instructions)  →  Number
```

`instructions` must be a list of instruction lists. Each instruction list begins with a mnemonic symbol followed by its operands. The assembled machine code is written to executable memory and called immediately; the `i64` value in `RAX` at the time of `ret` is returned as a Lisp `Number`.

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

### `load-asm`
Reads, parses (NASM-style text), assembles, and JIT-executes an external x86-64 assembly file.

```
(load-asm filename)  →  Number
```

### `load-asm-parallel`
Like `load-asm` but processes multiple files in parallel.

```
(load-asm-parallel (filenames...))  →  (list numbers...)
```

---

#### `Operand forms`

**S-expression syntax** (used by `asm`):

| Form                  | Syntax                    | Example           |
|-----------------------|---------------------------|-------------------|
| Register              | symbol                    | `rax`, `r8`       |
| Control register      | symbol                    | `cr0`             |
| XMM register          | symbol                    | `xmm0`            |
| Immediate (i32)       | number                    | `42`, `-1`        |
| Memory (base + disp)  | `(mem <reg> <disp>)`      | `(mem rsp -8)`    |

**NASM text syntax** (used by `load-asm` / `load-asm-parallel`):

| Form                  | Syntax                    | Example           |
|-----------------------|---------------------------|-------------------|
| Register              | bare name                 | `rax`, `r8`       |
| Control register      | bare name                 | `cr0`             |
| XMM register          | bare name                 | `xmm0`            |
| Immediate (i32)       | decimal/hex number        | `42`, `0xff`      |
| Memory                | `[base+disp]`             | `[rsp-8]`         |

Immediate values must fit in a signed 32-bit integer.

#### `Supported registers`

| Type               | Registers                                |
|--------------------|------------------------------------------|
| General-purpose    | `rax`, `rcx`, `rdx`, `rbx`, `rsp`, `rbp`, `rsi`, `rdi`, `r8`–`r15` |
| Control            | `cr0`, `cr1`, `cr2`, `cr3`, `cr4`, `cr8` |
| XMM (SSE)          | `xmm0`–`xmm15`                          |

All register names are case-insensitive.

#### `Supported instructions`

**Data movement**

| Mnemonic | Operands         | Description                        |
|----------|------------------|------------------------------------|
| `mov`    | dst src          | Move (reg/imm/mem)                 |
| `movcr`  | dst src          | Move to/from control register      |
| `push`   | src              | Push onto stack                    |
| `pop`    | dst              | Pop from stack                     |
| `lea`    | dst src          | Load effective address             |

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
| `call`    | target         | Call register or memory            |
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

**SSE2 packed (128-bit SIMD)**

| Mnemonic  | Operands         | Description                        |
|-----------|------------------|------------------------------------|
| `movdqa`  | dst src          | Aligned move (xmm/m128 ↔ xmm)      |
| `movdqu`  | dst src          | Unaligned move (xmm/m128 ↔ xmm)    |
| `paddb`   | dst src          | Packed add bytes                   |
| `paddw`   | dst src          | Packed add words                   |
| `paddd`   | dst src          | Packed add dwords                  |
| `paddq`   | dst src          | Packed add qwords                  |
| `psubb`   | dst src          | Packed subtract bytes              |
| `psubw`   | dst src          | Packed subtract words              |
| `psubd`   | dst src          | Packed subtract dwords             |
| `psubq`   | dst src          | Packed subtract qwords             |
| `pxor`    | dst src          | Packed bitwise XOR                 |
| `pand`    | dst src          | Packed bitwise AND                 |
| `por`     | dst src          | Packed bitwise OR                  |
| `pcmpeqb` | dst src          | Packed compare equal bytes         |
| `pcmpeqw` | dst src          | Packed compare equal words         |
| `pcmpeqd` | dst src          | Packed compare equal dwords        |

**Scalar SSE (double-precision float)**

| Mnemonic   | Operands         | Description                            |
|------------|------------------|----------------------------------------|
| `movsd`    | dst src          | Move scalar double (xmm ↔ xmm/m64)     |
| `addsd`    | dst src          | Add scalar double                      |
| `subsd`    | dst src          | Subtract scalar double                 |
| `mulsd`    | dst src          | Multiply scalar double                 |
| `divsd`    | dst src          | Divide scalar double                   |
| `cvtsi2sd` | dst src          | Convert int32/int64 to double          |
| `cvttsd2si`| dst src          | Truncate double to int32/int64         |
| `ucomisd`  | dst src          | Unordered compare scalar double        |
| `xorps`    | dst src          | Bitwise XOR (zero XMM with `xorps xmm, xmm`) |

Errors if an unrecognised mnemonic is encountered, an operand is out of range, or assembly/JIT allocation fails.