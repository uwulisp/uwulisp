# Task: Write a Parser for a Cubical Type Theory Proof Assistant

## Context

I am building a proof assistant based on **Cubical Type Theory** in Rust. The core type-checker, evaluator, and syntax are already implemented. I need you to write a **parser** that reads surface syntax and produces the internal `Term`, `Datatype`, `ConSig`, `PConSig`, and `ElimCase` types used by the existing codebase.

---

## Existing Codebase Summary

The codebase lives under `crate::cubical` and has these modules:

### `syntax.rs` — Internal Term Representation

```rust
pub type Name = String;
pub type Level = i32;

pub enum Term {
    TVar(i32),                                // de Bruijn index
    TApp(Box<Term>, Box<Term>),
    TAbs(Name, Box<Term>),
    TUniv(Level),                             // U0, U1, ...
    TIntervalTy,                              // 𝕀
    TPi(Name, Box<Term>, Box<Term>),          // (x : A) -> B
    TInterval(I),                             // interval expressions: I0, I1, IVar, Meet, Join, Neg
    TCube(DNF),                               // cube / face formula
    TPath(Box<Term>, Box<Term>, Box<Term>),   // Path A u v
    PLam(Name, Box<Term>),                    // ⟨i⟩ body
    PApp(Box<Term>, Box<Term>),               // p @ r
    THComp(Box<Term>, Box<Term>, Box<Term>, Box<Term>), // hcomp A phi u u0
    TEquiv(Box<Term>, Box<Term>),             // Equiv A B
    TMkEquiv(...),                            // mkEquiv A B f g eta eps
    TEquivFwd(Box<Term>, Box<Term>),          // equivFwd e x
    TUa(Box<Term>),                           // ua e
    TTransport(Box<Term>, Box<Term>),         // transport p x
    TGlue(Box<Term>, Box<Term>, Box<Term>),   // Glue A phi te
    TGlueElem(Box<Term>, Box<Term>, Box<Term>), // glueElem phi t a
    TUnglue(Box<Term>, Box<Term>, Box<Term>), // unglue phi te g
    TSigma(Name, Box<Term>, Box<Term>),       // (x : A) × B  or  Sigma (x : A). B
    TPair(Box<Term>, Box<Term>),              // (a , b)
    TFst(Box<Term>),                          // fst p
    TSnd(Box<Term>),                          // snd p

    // Inductive / HIT
    TData(Name),                              // a declared datatype used as a type
    TCon(Name, Name, Vec<Term>),              // con: datatype, constructor, args
    TPCon(Name, Name, Vec<Term>, Box<Term>),  // path-con: datatype, constructor, args, interval
    TElim(Box<Term>, Vec<ElimCase>, Box<Term>), // elim motive { cases } scrutinee
}

pub struct ElimCase {
    pub con: Name,
    pub binders: Vec<Name>,  // outermost-first; for path-cons, last binder = interval
    pub body: Box<Term>,
}

pub struct ConSig  { pub name: Name, pub arg_tys: Vec<Term> }
pub struct PConSig { pub name: Name, pub arg_tys: Vec<Term>, pub face0: Term, pub face1: Term }
pub struct Datatype { pub name: Name, pub cons: Vec<ConSig>, pub pcons: Vec<PConSig> }
```

### `interval.rs` — Interval Expressions

```rust
pub enum I {
    I0,
    I1,
    IVar(i32),       // de Bruijn index for interval variables
    Meet(Box<I>, Box<I>),
    Join(Box<I>, Box<I>),
    Neg(Box<I>),
}
```

Interval variables are scoped separately from term variables. The parser needs to maintain a separate interval variable environment (a `Vec<Name>`) in addition to the term variable environment.

### `env.rs` — Global Environment

```rust
pub struct Env {
    pub defs: Vec<(Name, Term, Term)>,  // (name, type, value), most-recent first
    pub datatypes: Vec<Datatype>,
}
```

The parser must produce a sequence of top-level **declarations** (definitions and datatype declarations) that can be inserted into `Env` via `env.define(name, ty, val)` and `env.declare_datatype(dt)`.

---

## Parser Requirements

### 1. Architecture

- Write a **hand-written recursive-descent parser** in Rust (do not use `nom`, `pest`, `lalrpop`, or any parser combinator / generator crate).
- The parser should be in a new file `src/cubical/parser.rs` (add `pub mod parser;` to `mod.rs`).
- Use `crate::cubical::syntax::{Term, ElimCase, ConSig, PConSig, Datatype, Name}` and `crate::cubical::interval::I`.
- Expose at minimum:
  ```rust
  pub fn parse_term(src: &str) -> Result<Term, ParseError>
  pub fn parse_program(src: &str) -> Result<Vec<Decl>, ParseError>
  ```

### 2. Variable Scoping

Variables in the surface language are resolved to **de Bruijn indices** during parsing (not in a separate pass).

- Maintain a `term_env: Vec<Name>` stack (innermost-first). When a binder introduces `x`, push `x` onto the front; looking up `x` returns its index.
- Maintain a separate `ivar_env: Vec<Name>` stack for **interval variables** introduced by `⟨i⟩` / `<i>` path-lambda binders and `forall i.` style binders in interval expressions.
- **Global names** (from previously parsed definitions/datatypes) are resolved against a `global_env: Vec<Name>` (names of top-level definitions in declaration order, most-recent first). A global name at position `k` in `global_env` is resolved to `TVar(term_env.len() + k)` — i.e. it sits just above the local variable stack, consistent with how `global_ctx` in `env.rs` works.
- If a name is not found in either local or global environments, and it matches a known datatype name (from already-parsed datatype declarations), produce `TData(name)`.
- Constructor names: when parsing `con_name args...`, if `con_name` is not a variable, try to look it up in known datatypes' `cons` / `pcons` fields and produce `TCon` / `TPCon`. The datatype name must be resolved automatically (the surface syntax should not require the user to write `TCon("Nat", "zero", [])` explicitly).

### 3. Surface Syntax to Parse

#### Top-level declarations

```
-- Line comment

def name : Type = term

data Name =
    | con1 : A -> B -> Name
    | con2 : Name
    | pcon : A -> B -> Name [ face0 , face1 ]
-- The `[face0, face1]` syntax declares path-constructor endpoints.
-- In the absence of arguments, just: | loop : S1 [ base , base ]
```

#### Universe

```
U0   U1   U2   ...   -- TUniv(n)
Type                 -- sugar for U0
```

#### Functions / Pi types

```
(x : A) -> B         -- TPi("x", A, B)
A -> B               -- TPi("_", A, B)   (non-dependent)
\x. t                -- TAbs("x", t)
\x y z. t            -- nested TAbs
fun x => t           -- alternative lambda syntax
```

#### Sigma types / pairs

```
(x : A) * B          -- TSigma
(a , b)              -- TPair (note: tuple syntax; distinguish from grouped expression by the comma)
fst t                -- TFst
snd t                -- TSnd
```

#### Interval and path types

```
I                    -- TIntervalTy
i0   i1              -- TInterval(I::I0), TInterval(I::I1)
i /\ j               -- TInterval(I::Meet(...))
i \/ j               -- TInterval(I::Join(...))
~ i                  -- TInterval(I::Neg(...))
Path A u v           -- TPath
<i> t                -- PLam("i", t)   (angle brackets or Unicode ⟨⟩)
p @ r                -- PApp(p, r)
```

#### Composition and Glue

```
hcomp A phi u u0     -- THComp
Equiv A B            -- TEquiv
mkEquiv A B f g eta eps  -- TMkEquiv
equivFwd e x         -- TEquivFwd
ua e                 -- TUa
transport p x        -- TTransport
Glue A phi te        -- TGlue
glueElem phi t a     -- TGlueElem
unglue phi te g      -- TUnglue
```

#### Inductive types

```
-- Use constructor names directly (no datatype prefix needed in surface syntax):
zero                 -- TCon("Nat", "zero", [])     -- if Nat has a `zero` constructor
suc n                -- TCon("Nat", "suc", [n])
base                 -- TCon("S1", "base", [])
loop @ r             -- TPCon("S1", "loop", [], r)  -- path-constructor application

-- Eliminator:
elim motive { con1 x y => body1 | con2 => body2 } scrutinee
-- or with a pipe on the first arm too:
elim motive { | con1 x y => body1 | con2 => body2 } scrutinee
```

For path-constructor eliminator cases, the last binder is the interval variable:
```
| loop i => <i> base    -- binders = ["i"], body = PLam("i", ...)
```

#### Application

```
f a b c              -- left-associative TApp
```

#### Grouping and annotation

```
(t)                  -- grouping
(t : A)              -- type annotation — produce a term that checks t against A
                     -- (you may represent this as a special AST node or inline-check it)
```

### 4. Operator Precedence (high to low)

1. Atoms: variables, universe, `i0`/`i1`, `I`, parenthesized expressions
2. Application (left-associative, highest)
3. `@` (path application, left-associative)
4. `~` (interval negation, prefix)
5. `/\` (interval meet)
6. `\/` (interval join)
7. `fst`, `snd`, `ua`, `transport`, `equivFwd`, etc. (prefix, apply to one argument)
8. `->` and `*` (right-associative, Pi / Sigma formation)
9. `,` (pair)
10. `\x.`, `<i>`, `fun x =>` (lowest — binders eat everything to the right)

### 5. Error Reporting

```rust
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}
```

Report line+column of the unexpected token or EOF. Good error messages are required (e.g. `"expected ':' after binder name 'x'"`, `"unknown constructor 'foo'"`, `"unmatched '('"`, etc.).

### 6. Lexer

Write a simple hand-written **lexer** (or token iterator) as a sub-module or inline helper. Token types should include at least:

```
Ident(String)     -- any name or keyword
Int(i32)          -- universe level, e.g. the `0` in `U0`
LParen, RParen    -- ( )
LBrace, RBrace    -- { }
LAngle, RAngle    -- < >  (for path lambdas)
Colon             -- :
Comma             -- ,
Dot               -- .
Arrow             -- ->
FatArrow          -- =>
Pipe              -- |
At                -- @
Backslash         -- \
Star              -- *
Slash             -- /  (part of /\)
AndSym            -- /\
OrSym             -- \/
Tilde             -- ~
LBracket          -- [
RBracket          -- ]
Equals            -- =
EOF
```

### 7. Testing

Include a `#[cfg(test)]` module with at least the following tests:

```rust
// Parses `\x. x` as TAbs("x", TVar(0))
// Parses `(x : U0) -> x` as TPi("x", TUniv(0), TVar(0))
// Parses `<i> i0` as PLam("i", TInterval(I::I0))
// Parses `p @ i0` as PApp(p, TInterval(I::I0))
// Parses a `data Nat = | zero : Nat | suc : Nat -> Nat` declaration
// Parses `elim motive { | zero => body0 | suc n => body1 } scrutinee`
// Parses `data S1 = | base : S1 | loop : S1 [ base , base ]`
// A round-trip test: parse a term, print with show_term, re-parse, check equality
```

---

## Important Implementation Notes

1. **De Bruijn**: All variable names in the output `Term` must be resolved to `TVar(i32)` — no named variables in the output. The parser is the only place names are tracked.

2. **Interval variables are separate**: `I::IVar(k)` uses a separate de Bruijn stack from `TVar(k)`. A path lambda `<i> t` pushes `i` onto `ivar_env`, not `term_env`.

3. **Constructor resolution**: The parser must track which datatypes have been declared so far. When it encounters a name that isn't in the term or global environment, check whether it matches any constructor of any known datatype. Produce `TCon(datatype_name, con_name, args)` accordingly. For path constructors used without `@`, parse them as `TPCon` only when applied with `@`.

4. **`TData` vs constructor**: A datatype name used as a *type* (not a constructor) must parse to `TData(name)`. The name resolution order is: local variable → global variable → constructor → datatype name.

5. **ElimCase binders**: In `elim motive { | con x y => body }`, binders `[x, y]` are listed outermost-first, exactly matching `ConSig::arg_tys` order. For path constructors, the last binder is the interval variable. These binders are pushed onto `term_env` (NOT `ivar_env`, even for the interval binder in an elim case — this is an ordinary term variable in the eliminator's scope).

6. **No external crates**: Use only the Rust standard library. The parser must compile with whatever dependencies are already in `Cargo.toml` — do not add new ones.

---

## Output

Produce a single file `parser.rs` ready to drop into `src/cubical/`. It should compile without warnings with `#![warn(unused)]`. Include a `pub use` re-export in `mod.rs`:

```rust
// in mod.rs, add:
pub mod parser;
```

The file should be well-commented, following the style of the existing codebase (module-level doc comment, section separators like `// ----...`).