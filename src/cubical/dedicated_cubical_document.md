---
title : Cubical Surface Language — Syntax Reference
sidebar:
  order: 10
---

This document describes the concrete syntax of the cubical surface language parsed by `parser.rs`. The language is a dependently-typed calculus with cubical type theory extensions: path types, interval expressions, Glue types, and higher inductive types.

---

## Top-level Declarations

A program is a sequence of declarations. Two forms are allowed.

### Value definition

```
def <name> : <type> = <term>
```

Example:

```
def id : (A : U0) -> A -> A = \A x. x
```

### Datatype declaration

```
data <Name> =
  | <con1> : <type>
  | <con2> : <type>
  ...
```

Constructors must return the declared datatype. A datatype must have at least one constructor.

Example:

```
data Nat = | zero : Nat | suc : Nat -> Nat
```

#### Path constructors

A constructor whose return type is followed by `[ <face0> , <face1> ]` is a **path constructor** — it specifies a path whose endpoints are `face0` and `face1`.

```
data S1 =
  | base : S1
  | loop : S1 [ base , base ]
```

---

## Comments

Line comments begin with `--` and extend to the end of the line.

```
-- This is a comment
```

---

## Terms

### Universes

| Syntax | Meaning |
|--------|---------|
| `U0`, `U1`, `U2`, … | Universe at level *n* |
| `Type` | Alias for `U0` |

### Variables

Plain identifiers resolve first as local variables (de Bruijn), then as top-level globals, then as constructors.

Identifier characters: start with a letter or `_`; continue with letters, digits, `_`, `'`, `?`, `!`, `-`.

### Lambda abstraction

```
\x. <body>
\x y z. <body>        -- multi-binder shorthand
```

Alternative keyword form:

```
fun x y z => <body>
```

The `λ` Unicode character is also accepted in place of `\`.

### Function application

```
<f> <arg1> <arg2> ...
```

Application is left-associative and is written by juxtaposition.

### Dependent function type (Π)

Non-dependent arrow:

```
<A> -> <B>
```

Dependent Pi (binder in scope in `B`):

```
(x : A) -> B
```

Explicit Pi former:

```
Pi (x : A). B
Π (x : A). B
```

### Pair / Sigma type

Pair term:

```
(<a> , <b>)
```

Non-dependent product:

```
<A> * <B>
```

Dependent Sigma (binder in scope in `B`):

```
(x : A) * B
```

Explicit Sigma former:

```
Sigma (x : A). B
Σ (x : A). B
```

### Projections

```
fst <pair>
snd <pair>
```

### Type ascription

```
(<term> : <type>)
```

---

## Interval Expressions

The interval type is written `I` or `𝕀`.

| Syntax | Meaning |
|--------|---------|
| `i0` or `0` | Left endpoint |
| `i1` or `1` | Right endpoint |
| `i /\ j` or `i ∧ j` | Meet (min) |
| `i \/ j` or `i ∨ j` | Join (max) |
| `~ i` or `¬ i` | Negation (flip) |

Operator precedence (highest to lowest): `~` > `/\` > `\/`.

---

## Path Types and Path Abstraction

### Path type

```
Path <A> <u> <v>
```

A path in type `A` from `u` to `v`.

### Path abstraction (interval lambda)

```
<i> <body>
```

Binds an interval variable `i` in `body`. The `⟨` and `⟩` Unicode angle brackets are also accepted.

### Path application

```
<p> @ <i>
```

Applies path `p` to interval expression `i`.

---

## Elimination

```
elim <motive> { | <con1> <binders> => <body1> | <con2> <binders> => <body2> ... } <scrutinee>
```

`->` may be used in place of `=>` in case branches.

The motive may optionally be wrapped in brackets:

```
elim[<motive>] { ... } <scrutinee>
```

Example:

```
elim motive { | zero => base_case | suc n => step } value
```

---

## Cubical Primitives

### Transport

```
transport <path> <element>
```

Transports `element` along the path `path`.

### Homogeneous composition

```
hcomp <type> <phi> <system> <base>
```

### Univalence and equivalences

| Syntax | Arguments | Meaning |
|--------|-----------|---------|
| `Equiv A B` | `A B` | Type of equivalences from `A` to `B` |
| `mkEquiv A B f g eta eps` | `A B f g eta eps` | Construct an equivalence |
| `equivFwd e x` | `e x` | Apply the forward map of equivalence `e` to `x` |
| `ua e` | `e` | Univalence: path from equivalence |

### Glue types

| Syntax | Arguments | Meaning |
|--------|-----------|---------|
| `Glue A phi te` | `A phi te` | Glue type |
| `glueElem phi t a` | `phi t a` | Construct a glue element (also `glue`) |
| `unglue phi te g` | `phi te g` | Unglue an element |

---

## Operator Precedence Summary

From lowest to highest binding:

| Level | Construct |
|-------|-----------|
| 1 (lowest) | `\x.`, `fun x =>`, `<i>`, `Pi`, `Sigma`, `,` (pair) |
| 2 | `->`, `*` (non-dependent arrow/product, right-assoc) |
| 3 | `\/` (interval join) |
| 4 | `/\` (interval meet) |
| 5 | `~` (interval negation, prefix) |
| 6 | `@` (path application, left-assoc) |
| 7 | juxtaposition (function application, left-assoc) |
| 8 | `fst`, `snd`, `ua`, `transport`, `equivFwd` (prefix) |
| 9 (highest) | atoms: identifiers, integer literals, parenthesised terms |

---

## Unicode Aliases

The following Unicode symbols are accepted as alternatives to their ASCII counterparts.

| Unicode | ASCII equivalent |
|---------|-----------------|
| `λ` | `\` (lambda) |
| `Π` | `Pi` |
| `Σ` | `Sigma` |
| `𝕀` | `I` (interval type) |
| `⟨` / `⟩` | `<` / `>` (path binder) |
| `×` | `*` (product) |
| `∧` | `/\` (meet) |
| `∨` | `\/` (join) |
| `¬` | `~` (negation) |

---

## Grammar Summary (BNF-style)

```
program  ::= decl*
decl     ::= 'def' ident ':' term '=' term
           | 'data' ident '=' ('|' con_decl)+

con_decl ::= ident ':' term ('[' term ',' term ']')?

term     ::= '\' ident+ '.' term
           | 'fun' ident+ '=>' term
           | '<' ident '>' term
           | 'Pi' '(' ident ':' term ')' '.' term
           | 'Sigma' '(' ident ':' term ')' '.' term
           | arrow_star ',' term
           | arrow_star

arrow_star ::= join ('->' arrow_star | '*' arrow_star)?

join     ::= meet ('\/' meet)*
meet     ::= tilde ('/\' tilde)*
tilde    ::= '~' tilde | papp
papp     ::= app ('@' tilde)*
app      ::= prefix_or_atom prefix_or_atom*

prefix_or_atom ::= 'fst' prefix_or_atom
                 | 'snd' prefix_or_atom
                 | 'ua' prefix_or_atom
                 | 'transport' prefix_or_atom prefix_or_atom
                 | 'equivFwd' prefix_or_atom prefix_or_atom
                 | 'Path' prefix_or_atom prefix_or_atom prefix_or_atom
                 | 'hcomp' prefix_or_atom prefix_or_atom prefix_or_atom prefix_or_atom
                 | 'Equiv' prefix_or_atom prefix_or_atom
                 | 'mkEquiv' prefix_or_atom x6
                 | 'Glue' prefix_or_atom prefix_or_atom prefix_or_atom
                 | 'glueElem' prefix_or_atom prefix_or_atom prefix_or_atom
                 | 'unglue' prefix_or_atom prefix_or_atom prefix_or_atom
                 | 'elim' term '{' cases '}' term
                 | atom

atom     ::= ident | '0' | '1' | '(' term ')'
```