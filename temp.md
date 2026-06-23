i tried test the cubical with [test.uwuc](file;file:///home/jihoo/uwulisp/test.uwuc) using "cargo run -- --cubical test.uwuc" command and i got this error
Cubical error: type error:
  Expected a Π-type, but found:
    Σ(A:U0). Π(B:U0). Σ(B:U0). A
and it's parser and other backends are self contained in [cubical](file;file:///home/jihoo/uwulisp/src/cubical) thus you don't need see other directory 
can you fix the error?

--- 
ai answer log for referrence
---

Let me examine the cubical directory and the test file to understand the error.

Listed directory cubical
Viewed test.uwuc:1-55
Viewed typechecker.rs:1-800
Viewed syntax.rs:1-502

Now let me look at the parser to understand how `(A : U0) * (B : U0)` is being parsed:

Listed directory parser
Viewed typechecker.rs:800-1100
Viewed grammar.rs:1-800

Now I understand the bug. Let me trace through the issue carefully.

The error is:
```
Expected a Π-type, but found: Σ(A:U0). Π(B:U0). Σ(B:U0). A
```

This is coming from the `swap` definition:
```
def swap : (A : U0) * (B : U0) -> (B : U0) * (A : U0) =
  \p. (snd p , fst p)
```

The issue is in `parse_paren` in the grammar. When parsing `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`, let me trace:

1. `parse_paren` sees `(A : U0)`, detects binder header `A : U0`  
2. Sees `*` → parses as `TSigma("A", U0, ...body...)` where body = `parse_arrow_star()` of `(B : U0) -> (B : U0) * (A : U0)`
3. In `parse_arrow_star`, it inserts `"_"` into `term_env` for `*`, but in step 2 above, when parsing the binder `(A : U0) *`, it inserts `"A"` into `term_env`.

Wait, let me re-read `parse_paren` more carefully:

```rust
if self.consume(&TokenKind::Star) {
    self.term_env.insert(0, name.0.clone());  // pushes "A"
    let body = self.parse_arrow_star()?;
    self.term_env.remove(0);
    return Ok(Term::TSigma(name.0, Box::new(name.1), Box::new(body)));
}
```

So body is `parse_arrow_star()` with `"A"` in scope. That parses `(B : U0) -> (B : U0) * (A : U0)`.

When `parse_arrow_star` gets `(B : U0)` via `parse_join()` then `parse_paren`, it hits `try_parse_binder_header` → gets `B : U0`, then sees `->` so: pushes `"B"` into `term_env` and parses `(B : U0) * (A : U0)`.

Now with env `["B", "A"]` we're parsing `(B : U0) * (A : U0)`. `parse_arrow_star` → `parse_join` → ... → `parse_paren`. Sees `(B : U0)`, parses binder header `B : U0`. Sees `*`. Pushes `"B"` (now env: `["B", "B", "A"]`). Parses `(A : U0)` as body.

For `(A : U0)`, `parse_paren` sees `A : U0` as a binder header with no `->` or `*`, so falls through to `resolve_ident("A")` – but `A` is at index 2 in env `["B", "B", "A"]` → `TVar(2)`.

Hmm, but the error shows `Σ(A:U0). Π(B:U0). Σ(B:U0). A` — this looks like the *type* `(A:U0) * (B:U0) -> (B:U0) * (A:U0)` is being parsed wrongly.

The issue is in `parse_arrow_star` — it pushes `"_"` for `->`, but for the `*` case in `parse_paren`, it pushes the actual binder name. But **`parse_arrow_star`** in the non-paren case also pushes `"_"` for `*`:

```rust
} else if self.consume(&TokenKind::Star) {
    self.term_env.insert(0, "_".to_string());  // <-- pushes "_" not the binder!
    let right = self.parse_arrow_star()?;
    self.term_env.remove(0);
    Ok(Term::TSigma("_".to_string(), Box::new(left), Box::new(right)))
}
```

The issue is the parse of `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`:

Starting from `parse_pair` → `parse_arrow_star` → `parse_join` → ... → `parse_paren`:
- `parse_paren` sees `(A : U0)`, tries binder header → gets `A : U0`
- Sees `*` → pushes `"A"` in term_env, parses rest with `parse_arrow_star()`

Now parsing `(B : U0) -> (B : U0) * (A : U0)` with env `["A"]`:
- `parse_arrow_star` → `parse_join` → ... → `parse_paren` → binder header `B : U0`
- Sees `->` → pushes `"B"` in term_env (env: `["B", "A"]`), parses `(B : U0) * (A : U0)` with `parse_arrow_star`
- Returns `TPi("B", U0, body_of_right_sigma)`

Now parsing `(B : U0) * (A : U0)` with env `["B", "A"]`:
- `parse_arrow_star` → `parse_join` → ... → `parse_paren` → binder header `B : U0`
- Sees `*` → pushes `"B"` in term_env (env: `["B", "B", "A"]`), parses `(A : U0)` with `parse_arrow_star`
- `parse_paren` sees `(A : U0)`, binder header → `A : U0`, no arrow/star → `resolve_ident("A")`
- `A` is at index 2 in env `["B", "B", "A"]` → `TVar(2)`
- Returns `TSigma("B", U0, TVar(2))`

So the full type parsed is:
`TSigma("A", U0, TPi("B", U0, TSigma("B", U0, TVar(2))))`

In the scope when checking the body of the outer sigma, `TVar(2)` refers to... let me think about de Bruijn indices. At the innermost sigma's body, env is `["B", "B", "A"]` from outer to inner. De Bruijn 0 = inner `B`, 1 = outer `B` (but wait, the inner sigma pushes its binder name `"B"` too). Actually `TVar(2)` = `A`.

So the type is `Σ(A:U0). Π(B:U0). Σ(B:U0). A` which matches the error message exactly!

The correct type should be `Σ(A:U0). Π(B:U0). Σ(B:U0). TVar(3)` (pointing to `A`), which is index 3 (after pushing: inner `B`, outer `B` from arrow, `A` from sigma). Wait actually `A` is being referenced but `TVar(2)` in the context `["B"(ivar=3), "B"(from->), "A"]` would be index 2 = `A`. That's correct semantically...

Wait, but `A` is the **type** variable bound by the outer sigma, not a value. When we say `(A : U0) * ... -> ... * (A : U0)`, `A` appears in the righthand side as a type. But what's the **depth** here?

Let me re-think. The problem is the `parse_paren` function when it encounters a binder-like pattern `(A : U0)` that's **not** followed by `->` or `*`. Instead of failing gracefully, it tries `resolve_ident(name.0)` — but at that point, the binder name `A` **isn't in scope yet** (the `*` handler pushed it into the term_env to the left). 

Actually wait — in the case `(A : U0) * (A : U0)` at the rightmost `(A : U0)`:
- We're in scope with `["B", "B", "A"]` (B pushed by inner star's parse_paren, B pushed by arrow's parse_paren, A pushed by outer star's parse_paren)
- The rightmost `(A : U0)` — parse_paren tries binder header, gets `A : U0`, no arrow/star follows → `resolve_ident("A")` → finds it at position 2 → `TVar(2)`

But in context `["B"_inner, "B"_arrow, "A"_outer]`, `TVar(2)` = `"A"`. This is actually correct! The rightmost `(A : U0)` should resolve `A` to the outer sigma binder, which is at depth 2.

So the inferred type IS `Σ(A:U0). Π(B:U0). Σ(B:U0). A`, and `A` here is `TVar(2)`.

Now the **real question** is why does `\p. (snd p, fst p)` fail with "Expected a Π-type, but found Σ..."?

Looking at the check function for `TAbs`, let me find it:

Viewed typechecker.rs:1100-1300

Now I see the check for `TAbs` at line 1269-1283. When checking `\p. (snd p, fst p)` against `Σ(A:U0). Π(B:U0). Σ(B:U0). A`, it tries to match the type against `TPi`, but the type is `TSigma` — hence "Expected a Π-type, but found: Σ(A:U0). Π(B:U0). Σ(B:U0). A".

So the **real root cause** is: the type `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` is being parsed incorrectly. The `*` should be non-dependent (anonymous) sigma, so the `->` should bind tighter than `*`, but instead the parser is treating the whole right side as a Sigma type.

The precedence issue: `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` should parse as:
```
((A : U0) * (B : U0)) -> ((B : U0) * (A : U0))
```

But the parser uses `parse_paren` which, upon seeing `(A : U0) *`, pushes `A` into scope and calls `parse_arrow_star()` for the ENTIRE remaining `(B : U0) -> (B : U0) * (A : U0)`. This makes `->` bind inside `*`, so it parses as:
```
Σ(A:U0). ((B : U0) -> ((B : U0) * (A : U0)))
```
= `Σ(A:U0). Π(B:U0). Σ(B:U0). A`

Which is exactly the wrong type shown in the error!

The problem is that the parser's `parse_paren` binder detection greedily captures the body with `parse_arrow_star()`. When `*` follows a `(x : T)` paren-binder, it should only be a **dependent pair type** former (like `Σ`), and the resulting `TSigma` should bind at the same level as the non-binder `*`. But the issue is that the right-hand side of the resulting sigma is parsed by `parse_arrow_star()`, which has higher precedence than what the outer context expects.

The real fix: in `parse_paren`, when we have `(x : T) *`, we should parse the right side with `parse_arrow_star()` which is correct. But the outer context should also see `*` as right-associative at the same level as `->`. The trouble is that **with binder**, `*` binds too loosely (the right side is all of `parse_arrow_star()` which includes `->`) but **without binder**, `->` binds less loosely than `*` since `parse_arrow_star` first tries `parse_join`, and then handles both `->` and `*` right-associatively at the same level.

Wait, actually both `->` and `*` are at the same level in `parse_arrow_star()`. Let me re-read:

```rust
fn parse_arrow_star(&mut self) -> Result<Term, ParseError> {
    let left = self.parse_join()?;
    if self.consume(&TokenKind::Arrow) {
        self.term_env.insert(0, "_".to_string());
        let right = self.parse_arrow_star()?;
        self.term_env.remove(0);
        Ok(Term::TPi("_".to_string(), Box::new(left), Box::new(right)))
    } else if self.consume(&TokenKind::Star) {
        self.term_env.insert(0, "_".to_string());
        let right = self.parse_arrow_star()?;
        self.term_env.remove(0);
        Ok(Term::TSigma("_".to_string(), Box::new(left), Box::new(right)))
    } else {
        Ok(left)
    }
}
```

So `->` and `*` are right-associative at the same level.

When parsing non-binder style, `A * B -> C * D` would parse as `A * (B -> (C * D))` — that's also wrong! But wait, the left-hand side is parsed first with `parse_join`, so `A * B -> C * D`:
- `parse_join` → `A`
- sees `*` → parse right with `parse_arrow_star` → this gives `B -> (C * D)` 
- Result: `A * (B -> (C * D))`

That's also wrong associativity. So the real issue is that `*` and `->` are at the same right-associative level.

But the type in the test uses `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` where `->` is meant to be **less tightly binding** than `*`.

The expected parse should be: `((A : U0) * (B : U0)) -> ((B : U0) * (A : U0))`.

The problem is the precedence: `->` should have **lower** precedence than `*`. We need to restructure `parse_arrow_star` to give `*` higher precedence:

```
parse_arrow:
    left = parse_star
    if sees `->`: right = parse_arrow; return Pi(left, right)
    else: return left

parse_star:
    left = parse_join
    if sees `*`: right = parse_star; return Sigma(left, right)  
    else: return left
```

But we also need to handle the binder cases correctly.

Actually the cleanest fix is to separate `->` and `*` into two levels: `parse_arrow` uses `parse_sigma` as its left operand, and `parse_sigma` uses `parse_join`.

Let me also look at how the binder parsing works in `parse_paren`:

In `parse_paren`, when we see `(A : U0)` binder followed by `*`, it calls `parse_arrow_star()` for the body. This is correct for the *Sigma* body — but the issue is that the overall `(A : U0) * body` expression should only extend as far as the `*` groups, and `->` should be at a lower level.

The fix: split `parse_arrow_star` into two functions: `parse_arrow` (which calls `parse_sigma` for the left) and `parse_sigma` (which calls `parse_join`). And in `parse_paren`, when seeing `(x : T) *`, call `parse_sigma` (not `parse_arrow`) for the right body; and when seeing `(x : T) ->`, call `parse_arrow` for the right body (which is the current behavior via `parse_arrow_star`).

Viewed grammar.rs:800-861

Now I have a full picture. The fix is to split `parse_arrow_star` into two levels: `parse_arrow` (lower precedence, for `->`) and `parse_sigma` (higher precedence, for `*`). 

Here's the fix:
- `parse_arrow_star` → renamed/restructured into `parse_arrow` which calls `parse_sigma` for its operands
- New `parse_sigma` function handles `*` and calls `parse_join` for operands
- `parse_paren`: when `(x:T) ->`, the body uses `parse_arrow` (full); when `(x:T) *`, the body uses `parse_sigma` only
- `parse_lambda`: when `Π/Pi (x:T). body`, body uses `parse_term` (fine since it's explicit binder); when `Σ/Sigma (x:T). body`, also fine
- `parse_pair` needs to call `parse_arrow` instead of `parse_arrow_star`
- `parse_face_with_extra_datatype` called `parse_arrow_star` directly — should call `parse_arrow`

Also need to handle the non-binder `*` case in `parse_arrow_star` (now `parse_sigma`) correctly so that `A * B -> C` parses as `(A * B) -> C` not `A * (B -> C)`.

Edited grammar.rs

Now let me verify the fix compiles and the test runs correctly:

Ran command: `cargo run -- --cubical test.uwuc 2>&1`

Still failing with the same error. The issue is in the parsing, but `cargo run` seems to show it compiled correctly — let me verify the parsing logic more carefully. Let me add some debug output to understand what's being parsed:

Viewed grammar.rs:270-325

The parse functions look correct. Now let me trace through the parsing of `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` with the new code:

1. `parse_pair` calls `parse_arrow()`
2. `parse_arrow` calls `parse_sigma()`
3. `parse_sigma` calls `parse_join()` → `parse_paren()` → sees `(A : U0)`, binder header, followed by `*`
4. `parse_paren` now calls `parse_sigma()` for the body (with "A" in env)
5. `parse_sigma` calls `parse_join()` → `parse_paren()` → sees `(B : U0)` binder header...

Wait, there's still an issue! In step 3, when `parse_paren` sees `(A : U0) *`, it pushes `"A"` and calls `parse_sigma()`. But `parse_sigma` will try to parse `(B : U0) -> ...`, and since `parse_sigma()` calls `parse_join()` which calls down to `parse_paren()`, it will see `(B : U0)`, detect binder header, and then check what follows — it will see `->`. Since in `parse_paren`, `->` is still handled and calls `parse_arrow()` for the body, it will create `TPi("B", U0, body_of_(B:U0)->...)`. Then `parse_sigma` receives this `TPi("B", ...)` as `left` and checks for `*` — there's no `*` after, so returns it.

So the result is `TSigma("A", U0, TPi("B", U0, ...))`. But the intent was for `(A:U0) * (B:U0) -> ...` to parse as `(A:U0) * ((B:U0) -> ...)` at the outer `parse_arrow` level, meaning we need the `*` at the top level to be the non-dependent one.

Wait, let me re-think. The type is:
```
(A : U0) * (B : U0) -> (B : U0) * (A : U0)
```

There are two possible intended parses:
1. `((A : U0) * (B : U0)) -> ((B : U0) * (A : U0))` — this is a function type
2. `(A : U0) * ((B : U0) -> ((B : U0) * (A : U0)))` — this is a sigma type

The test has `def swap : (A : U0) * (B : U0) -> (B : U0) * (A : U0)` with the body `\p. (snd p, fst p)`, which is clearly a *function*. So parse 1 is the intended one.

The issue: the **outer** `*` is not the `parse_paren` binder-`*` but rather the non-dependent `*` from `parse_sigma`. But `parse_paren` is consuming the `(A : U0)` and the `*` together, treating it as a dependent Σ binder, when actually the user meant non-dependent `*`.

This is actually an **ambiguity** in the syntax! `(A : U0) *` could mean:
- A dependent sigma binder `Σ(A:U0). ...`
- The non-dependent `*` applied to the expression `(A : U0)` (which is just `U0` syntactically since `A` would not be in scope)

Wait, but `(A : U0)` in parentheses looks exactly like a binder. The parser `try_parse_binder_header` looks ahead for `name : type` and if it sees that pattern, it treats it as a binder.

Hmm, but what if `A` is not in scope? Then `(A : U0)` as an **expression** makes no sense (A is unbound). However, as a type binder in `(A : U0) -> ...` or `(A : U0) * ...`, `A` becomes bound.

For `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`, the user probably intends `A` and `B` to be just type variables (bound on the left of `*`/`->`). The question is whether the first `*` creates a dependent pair or a function argument.

Looking at `swap`:
```
def swap : (A : U0) * (B : U0) -> (B : U0) * (A : U0) =
  \p. (snd p , fst p)
```

The value `\p. (snd p, fst p)` takes ONE argument `p` and returns a pair. So the type must be a function type! The whole `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` is the **type of `swap`** and should be `((A:U0) * (B:U0)) -> ((B:U0) * (A:U0))`, i.e., a function from a pair to a pair.

But wait... even in that case, `A` and `B` in `(B : U0) * (A : U0)` on the right side would need to be in scope. If the type is `((A:U0) * (B:U0)) -> ((B:U0) * (A:U0))`, then `A` and `B` are not bound anywhere — they're free variables in the type! That doesn't typecheck either.

Actually I think the intended parse IS the dependent sigma type:
```
Σ(A:U0). Π(B:U0). Σ(B:U0). A
```

Wait no — the correct meaning is: `swap` takes a pair `(A, B)` where `A : U0` and `B : U0` (a pair of types), and returns a pair `(B', A')` where the types are swapped. But this still doesn't work because `A` and `B` are universe-polymorphic type variables that need to be universally quantified.

Actually, looking at this more carefully — `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` in HoTT-style syntax should likely mean:

`Π(A:U0). Π(B:U0). (A * B) -> (B * A)` but written differently.

Hmm, but the test's `swap` simply takes a pair `p` and returns `(snd p, fst p)`. The type of that would need `p : A * B` for some `A, B`, and the result is `B * A`. So maybe the type annotation needs to be:

```
def swap : (A : U0) -> (B : U0) -> A * B -> B * A
```

But that's three lambda arguments. The test has `\p. ...` with one argument.

Actually, looking at the error type `Σ(A:U0). Π(B:U0). Σ(B:U0). A` and trying to match `\p. (snd p, fst p)` against that: the typechecker tries to check `TAbs("p", ...)` against `TSigma("A", U0, ...)`, which fails with "Expected a Π-type but found Σ".

So either:
1. The type annotation for `swap` is incorrect in `test.uwuc`, OR
2. The parser is mis-parsing the type

Let me re-read: `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`.

In Agda/Lean-style, `*` means product (sigma) and has HIGHER precedence than `->`. So this would parse as:
```
((A : U0) * (B : U0)) -> ((B : U0) * (A : U0))
```
= `Π(_: Σ(A:U0). B:U0). Σ(B:U0). A:U0`

But that requires `A` and `B` on the right side to be in scope, which they're not in the non-dependent version.

Actually, looking at this again — in the non-dependent `*`, `(A : U0) * (B : U0)` would be parsed as `U0 * U0` (since `(A : U0)` resolves to `U0` when `A` is unbound). But then `A` in `(A : U0)` on the right wouldn't be in scope.

I think there's a genuine ambiguity/bug in the **test file** as well. But first let me check what the **original** parser intended for this. Looking at the `parse_paren` original code, when `(A : U0) *` is seen, it pushes `A` in scope and calls `parse_arrow_star()` for the body, giving `Σ(A:U0). Π(B:U0). Σ(B:U0). A`.

So the original parser ALWAYS treats `(x : T) *` as a dependent Σ binder. That's the intended behavior. The type `Σ(A:U0). Π(B:U0). Σ(B:U0). A` means "a pair where the first component is a type `A:U0` and the second component is a function that takes a type `B:U0` and returns a pair of type `B * A`." That doesn't really make sense for `swap`.

The REAL issue is that the **test file's `swap` definition** is using a type that should be:
```
(A : U0) -> (B : U0) -> A * B -> B * A
```

OR if the `*` is intended as a non-dependent product of types:
```
def swap : (A : U0) -> (B : U0) -> (A * B) -> (B * A)
```

But that requires 3 lambdas, not 1.

Wait — but the test has `\p. (snd p, fst p)` with ONE argument. Let me think about what type would make that work... `p : A * B`, so the domain is `A * B` for specific `A` and `B`. The function takes `A * B` to `B * A`. So with concrete types:

If the type of `swap` is `U0 * U0 -> U0 * U0`, that would work in principle (U0 * U0 is a pair of types). But that's probably not the intent.

Actually, I think the intent is that `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` in this language means:

"A function from `(A * B)` (a pair of types) to `(B * A)` (a pair of types)", where `A` and `B` are just names for the components of the input pair, shown in the type annotation non-dependently.

With my fix, `(A : U0) * (B : U0)` would be treated as `U0 * U0` (since `A` is not bound in a non-dependent *), and then `->` makes the whole thing `(U0 * U0) -> (U0 * U0)`. Then checking `\p. (snd p, fst p)` against that would work if the products typecheck.

But wait — with the **old** parser (before my change), `(A : U0) *` already uses `A` as a binder in parse_paren. My change to `parse_sigma` doesn't affect the `parse_paren` path at all! The `parse_paren` still detects `(A : U0)` as a binder header and when followed by `*`, creates a `TSigma`.

The issue is: in `parse_paren`, when we see `(A : U0) *`, we always create a dependent Σ. This happens both before AND after my fix, because I only changed:
1. The body of `(x:T) *` now calls `parse_sigma()` instead of `parse_arrow_star()` 
2. The non-paren `*` in `parse_sigma` now has higher precedence than `->`

So for the type `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`:

**With my fix:**
- `parse_arrow` → `parse_sigma` → `parse_join` → ... → `parse_paren`
- `parse_paren` sees `(A : U0)`, binder header, sees `*`
- Pushes `"A"`, calls `parse_sigma()` for body
- `parse_sigma` → `parse_join` → `parse_paren` → sees `(B : U0)`, binder header, sees `->`
- Since in `parse_paren` `->` still calls `parse_arrow()`, this creates `TPi("B", U0, body)`
- body = `parse_arrow()` of `(B : U0) * (A : U0)` with env `["B", "A"]`
  - `parse_arrow` → `parse_sigma` → `parse_join` → `parse_paren` sees `(B : U0)` binder header, sees `*`
  - pushes `"B"`, calls `parse_sigma()` of `(A : U0)` with env `["B", "B", "A"]`
  - `parse_paren` sees `(A : U0)` binder header, no `->` or `*`, falls to `resolve_ident("A")` → TVar(2)
  - Returns `TSigma("B", U0, TVar(2))`
- Returns `TSigma("A", U0, TPi("B", U0, TSigma("B", U0, TVar(2))))`

Which is still `Σ(A:U0). Π(B:U0). Σ(B:U0). A`! 

The fundamental problem is that `parse_paren` eagerly treats `(x : T) *` as a binder. The fix of splitting `parse_arrow_star` didn't help here because the top-level `(A : U0)` is consumed by `parse_paren` before `parse_sigma` even sees it.

OK so now I need to understand the actual intent of the test. Let me look at what type `\p. (snd p, fst p)` would need to have:

- `p` must have type `Σ(x:A). B` for some `A, B`
- `snd p : B` (shifted)
- `fst p : A`
- Result: `(snd p, fst p) : Σ(x:B). A`

So `swap : Σ(x:A). B → Σ(x:B). A`.

The **correct type** for `swap` in this language (with `A, B : U0`) would be something like:
```
def swap : (A : U0) -> (B : U0) -> (A * B) -> B * A
```

But that needs 3 arguments. Or perhaps:
```
def swap : (A : U0) * (B : U0) -> (B : U0) * (A : U0)
```
where `(A : U0) * (B : U0)` means `Σ(A:U0). U0` (non-dependently) — but then `A` wouldn't be in scope on the right side.

Actually I think the real intended interpretation is that the test file has `swap` with type annotation **incorrectly** written, or the language is supposed to support a syntax like `(A : U0) * (B : U0) -> ...` where `A` and `B` from the left side are in scope on the right. That would only make sense if `->` creates the overall type and `A`, `B` are implicit type parameters.

Let me check: what does the typechecker produce when checking `\p. (snd p, fst p)` against `Σ(A:U0). Π(B:U0). Σ(B:U0). A`?

- Check `TAbs("p", body)` against `TSigma(...)` → fails with ExpectedPi — that's the error.

So the type annotation IS wrong for what the body is. The type `Σ(A:U0). Π(B:U0). Σ(B:U0). A` is a **value** (a dependent pair), not a function type. But `\p. (snd p, fst p)` is a lambda (function).

The fix must be in the test file OR in how `*` is parsed when not preceded by an explicit binder. Let me look at the issue from a different angle: what if the `(A : U0)` is supposed to NOT be treated as a binder here, but just as a parenthesized expression `U0`?

That would make `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` parse as:
- `parse_arrow` sees `U0 * U0 -> U0 * U0`
- which = `(U0 * U0) -> (U0 * U0)` with my precedence fix

Then `\p. (snd p, fst p)` would be checked against `Π(_: Σ(_, U0). U0). Σ(_, U0). U0`. That would work — `p` would be a pair of types, and `snd p` and `fst p` would swap them.

So the solution is: when `parse_paren` sees `(A : U0)` in a non-binder context (i.e. it's just a parenthesized type annotation that resolves `A` to its already-in-scope name), it should NOT treat it as a binder.

The current `try_parse_binder_header` only succeeds if the ident is followed by `:`. The issue is that it ALWAYS creates a binder header for `(name : type)`, even if `name` is already in scope.

One approach: in `try_parse_binder_header`, if the name is already in scope (in `term_env`), don't treat it as a new binder. But `A` is NOT in scope in the top-level def — `A` is genuinely unbound. So treating it as a binder (as currently done) introduces a new `A` variable.

Actually, I think the core issue is: in `(A : U0) * (B : U0) -> (B : U0) * (A : U0)`, the user intends:
- `A` and `B` are just locally-bound type variables
- The `*` on the left means "both A and B (both : U0) are inputs"
- The `->` makes it a function
- On the right, `A` and `B` refer to the left-side bound variables

This means the ORIGINAL parsing `Σ(A:U0). Π(B:U0). Σ(B:U0). A` IS actually correct, and the bug must be elsewhere — the typechecker needs to handle this type when checking a lambda.

Actually wait — `Σ(A:U0). Π(B:U0). Σ(B:U0). A` means "a pair where first component is `A:U0` and second component is a function `B:U0 → Σ(B:U0). A`". That's NOT what `swap` should be.

The intended type of `swap` is: given a pair `(A, B)` where `A:U0` and `B:U0` (two types), return the swapped pair `(B, A)`. That's `Σ(A:U0). U0 → Σ(B:U0). U0`. But this is just `U0 * U0 → U0 * U0` basically.

OR — maybe the user intends this more Agda-style where `*` for types means "product type", and so `(A : U0) * (B : U0) → ...` means "assuming types A and B, the function type...". This is really like:

`∀ A B : U0, A * B → B * A`

which in the Π notation would be `(A : U0) → (B : U0) → A * B → B * A`.

I think the test file needs to be fixed. The swap definition should be:

```
def swap : (A : U0) -> (B : U0) -> A * B -> B * A =
  \A B p. (snd p , fst p)
```

But let me check whether the test.uwuc is supposed to be a test that PASSES or is it showing a broken test that we need to fix to make pass. Let me look at the other definitions and see if they work:

Actually, the user says "I tried to test the cubical with test.uwuc" and got the error. The intent is for ALL the definitions in test.uwuc to typecheck. So either the test file is wrong or the typechecker/parser is wrong.

Let me look at the `compose` definition:
```
def compose : (A : U0) -> (B : U0) -> (C : U0)
            -> (B -> C) -> (A -> B) -> A -> C =
  \A B C g f x. g (f x)
```

This uses explicit Π-binders `(A : U0) -> ...` style, which works fine.

The `swap` definition uses `*`:
```
def swap : (A : U0) * (B : U0) -> (B : U0) * (A : U0) =
  \p. (snd p , fst p)
```

The body `\p. (snd p, fst p)` takes ONE argument and returns a pair. So the type should be a function type. But the parsed type is `Σ(A:U0). Π(B:U0). Σ(B:U0). A` which is NOT a function type.

The right fix is probably to change the test file's `swap` definition to:
```
def swap : (A : U0) -> (B : U0) -> A * B -> B * A =
  \A B p. (snd p , fst p)
```

But wait, can we fix this without modifying the test file? The user said the error comes from test.uwuc and we should fix it. Maybe the PARSER should be fixed so that `(A : U0) * (B : U0) -> ...` is parsed as `(A * B) -> ...` where `A = U0` and `B = U0`.

Actually the fundamental question is: when the parser sees `(A : U0) * ...`, should it treat `(A : U0)` as:
1. A binder expression creating a new variable `A : U0` (dependent Σ), OR  
2. A parenthesized expression `U0` where `A` is just an irrelevant annotation (non-dependent, with `A` discarded)

The current parser always does (1). For `(A : U0) *` to create a non-dependent product, we'd need to detect that `A` is never used in the right side. Or alternatively, not treat `(A : T)` as a binder in the `*` case at all.

Actually I think the correct interpretation here (matching standard HoTT/Cubical type theory syntax) is:

- `(x : A) -> B` = dependent Pi (Π(x:A). B, x may appear in B)
- `A -> B` = non-dependent Pi (shorthand, x doesn't appear in B)  
- `(x : A) * B` = dependent Sigma (Σ(x:A). B, x may appear in B)
- `A * B` = non-dependent Sigma (shorthand)

So `(A : U0) * (B : U0) -> (B : U0) * (A : U0)` should parse as:
- outer: `Σ(A:U0). body`
- body: `Π(B:U0). ...` → wait, `->` not `*` next
- Hmm, `(B : U0) -> (B : U0) * (A : U0)` should be `Π(B:U0). Σ(B:U0). A`

So the full type is `Σ(A:U0). Π(B:U0). Σ(B:U0). A`.

And the body of `swap` should not be `\p. (snd p, fst p)` but something that produces a value of type `Σ(A:U0). Π(B:U0). Σ(B:U0). A`.

That would be: `(U0, (\B. (U0, ???)))`. That doesn't make sense for a "swap" function.

I think the **test file** has an error in the `swap` definition — both the type and the implementation. The correct `swap` for swapping a pair `(a, b) : A * B` to get `(b, a) : B * A` should be:

```
def swap : (A : U0) -> (B : U0) -> A * B -> B * A =
  \A B p. (snd p , fst p)
```

Let me update the test file to fix this:

Edited test.uwuc
Ran command: `cargo run -- --cubical test.uwuc 2>&1 | tail -20`

Different error now — progress! Let me look at this new error. The error is about a type mismatch during `snd p` type inference. With type `A * B -> B * A`, let me trace:

- `\A B p. (snd p, fst p)` checked against `Π(A:U0). Π(B:U0). Π(_:A*B). B*A`
- In ctx `[p:A*B, B:U0, A:U0]`, check `(snd p, fst p)` against `B*A`
- `B*A` = `TSigma("_", B, shift(1,0,A))` where B=TVar(1), A=TVar(2)
- The pair `(snd p, fst p)`: `fst p : A` (index 2 in ctx) ✓
- `snd p : B[fst p / x]` but since non-dependent, B = TVar(1) = B

Actually the issue might be in how `A * B` is parsed. With the new parser, `A * B` where `A` and `B` are in scope goes through `parse_sigma`:
- `parse_sigma` → `parse_join` → `parse_papp` → `parse_app` → resolves `A` to `TVar(...)`, then sees `*`
- Wait, `A` is an atom, parsed by `parse_app`, then `parse_sigma` sees `*`

But wait! The `*` in `parse_arrow_star` was consumed by `parse_arrow_star → parse_sigma`. But the **old** code had `parse_arrow_star` handle `*`. With my split, `parse_arrow` → `parse_sigma` → `parse_join` → ... → resolves `A`, then `parse_sigma` sees `*` and processes `B`.

So `A * B` parses as `TSigma("_", TVar(idx_A), TVar(idx_B+1))` where `+1` is because a dummy `"_"` is pushed.

Wait, I push `"_"` into `term_env` when handling `*` in `parse_sigma`:
```rust
self.term_env.insert(0, "_".to_string());
let right = self.parse_sigma()?;
self.term_env.remove(0);
```

So when parsing the right side of `A * B`, `"_"` is pushed into scope. This means any variables on the right get their index shifted by 1. `B` was at some index `k`, but now with `"_"` pushed, it's at `k+1`. And after the parse, `"_"` is removed. The resulting `TSigma("_", TVar(k_A), TVar(k_B+1))` is then stored.

This is correct behavior for dependent Sigma — the right side is in a scope with `"_"` bound. Since `B` is non-dependent on the pair's first component, we use `TVar(k_B+1)` (shifting `B` past the anonymous binder). This is exactly what the old `parse_arrow_star` did too.

Let me check the old code:
```rust
} else if self.consume(&TokenKind::Star) {
    self.term_env.insert(0, "_".to_string());
    let right = self.parse_arrow_star()?;
    self.term_env.remove(0);
    Ok(Term::TSigma("_".to_string(), ...))
}
```

Yes, same behavior. So the `A * B` parsing is unchanged.

The error is `Type mismatch: expected #1, got fst #0`. Let me think — `#1` means `TVar(1)` displayed with the name at index 1. This comes from the pair checking.

The new error is occurring during a different check. Let me look at the full error output:

Ran command: `cargo run -- --cubical test.uwuc 2>&1 | grep -A5 "Cubical error"`

Let me look at the `mod.rs` to see how definitions are typechecked to understand which definition is failing:

Viewed mod.rs:1-268

The error `Type mismatch: expected #1, got fst #0` is about the `snd` projection. Let me trace through the type of `swap` more carefully.

With type `(A : U0) -> (B : U0) -> A * B -> B * A`, let me trace parsing:

1. `(A : U0) ->` → `TPi("A", U0, body_A)` with A pushed
2. `(B : U0) ->` → `TPi("B", U0, body_B)` with B pushed (env: `["B", "A"]`)
3. `A * B ->` → `parse_arrow`:
   - left = `parse_sigma()` of `A * B`
     - `parse_sigma` → `parse_join` → ... → resolves `A` as TVar(1), then sees `*`
     - pushes `"_"`, right = `parse_sigma` of `B` = TVar(1+1)=TVar(2) (B was at 1, with `_` pushed it's 2... wait, `B` was at index 0 before pushing `_`, so after pushing `_`, `B` is at index 1)
     
Actually wait. With env `["B", "A"]`, `B` is at index 0 and `A` is at index 1.

When parsing `A * B`:
- `A` → `TVar(1)` (index 1 in `["B","A"]`)
- `*` → push `"_"`, env becomes `["_","B","A"]`
- `B` → `TVar(1)` in new env `["_","B","A"]` (B is at index 1)
- pop `"_"`, env back to `["B","A"]`
- Result: `TSigma("_", TVar(1), TVar(1))` = `Σ(_:A). B`

Hmm wait, `B` in `["_","B","A"]` is at index 1, so it's `TVar(1)`. But in a proper dependent Sigma body, `TVar(0)` would refer to the bound variable `_`. So `TVar(1)` = `B` from the outer context (shifted by 1 past `_`). That's correct — `B` as the type of the second component.

4. Then `->` → push `"_"`, env `["_","B","A"]`
5. Right side `B * A`:
   - `B` → TVar(2) in `["_","B","A"]` (B shifted by 1 due to the `->` binder and another for the `*` push)... wait.

Actually let me re-read. The `->` case in `parse_arrow`:
```rust
if self.consume(&TokenKind::Arrow) {
    self.term_env.insert(0, "_".to_string());
    let right = self.parse_arrow()?;
    self.term_env.remove(0);
    Ok(Term::TPi("_".to_string(), Box::new(left), Box::new(right)))
}
```

After pushing `"_"` for `->`, env is `["_", "B", "A"]`. Now parsing `B * A`:

`parse_arrow` → `parse_sigma`:
- `parse_sigma` → `parse_join` → ... → resolves `B` as TVar(2)? No: in `["_","B","A"]`, `B` is at index 1. Wait, indices in the env are positional: env[0]="_", env[1]="B", env[2]="A". So `B` = TVar(1), `A` = TVar(2).

`B * A`:
- B → TVar(1)
- `*` → push `"_"`, env `["_","_","B","A"]`
- A → TVar(3)? No: in `["_","_","B","A"]`, A is at index 3.
- pop `"_"`
- Result: `TSigma("_", TVar(1), TVar(3))` = `Σ(_:B). A`

Hmm, but after the `->` push, the final Pi is `TPi("_", TSigma("_",TVar(1),TVar(1)), TSigma("_", TVar(1), TVar(3)))`.

Wait, I messed up. Let me redo this. After the outer `(A:U0)->` and `(B:U0)->`, env = `["B", "A"]` (innermost first). B=TVar(0), A=TVar(1).

Parsing `A * B -> B * A`:

**`parse_arrow` is called** with env `["B","A"]`:
- `parse_sigma` of `A * B`:
  - `parse_join` → `A` = TVar(1) (A is at index 1)
  - sees `*`, pushes `"_"` → env `["_","B","A"]`
  - right = `parse_sigma` of `B`:
    - B = TVar(1+1)? No, B is at env position 1 (in `["_","B","A"]`), so B = TVar(1)
  - pop `"_"` → env back to `["B","A"]`
  - Result: `TSigma("_", TVar(1), TVar(1))`

- sees `->`, pushes `"_"` → env `["_","B","A"]`
- right = `parse_arrow` of `B * A`:
  - `parse_sigma` of `B * A`:
    - B = TVar(1) (in `["_","B","A"]`)
    - sees `*`, pushes `"_"` → env `["_","_","B","A"]`
    - right of `*` = `A` = TVar(3) (in `["_","_","B","A"]`, A is at index 3)... Wait: env `["_","_","B","A"]`—indices: 0="_", 1="_", 2="B", 3="A". Yes, A=TVar(3).
    - pop `"_"` → env back to `["_","B","A"]`
    - Result: `TSigma("_", TVar(1), TVar(3))`
- pop `"_"` → env back to `["B","A"]`
- Final arrow result: `TPi("_", TSigma("_",TVar(1),TVar(1)), TSigma("_",TVar(1),TVar(3)))`

Now wrapping with the outer `(B:U0)->` and `(A:U0)->`:

Full type term:
```
TPi("A", U0, TPi("B", U0, TPi("_", TSigma("_",TVar(1),TVar(1)), TSigma("_",TVar(1),TVar(3)))))
```

Let me verify this:
- `Π(A:U0). Π(B:U0). Π(_: Σ(_:A). B). Σ(_:B). A`

In context `[p: Σ(_:A).B, B:U0, A:U0]`:
- p = TVar(0): type `Σ(_:A).B` where in scope A=TVar(2), B=TVar(1)

Wait, the `TSigma("_", TVar(1), TVar(1))` in the type of `_` means `Σ(_:TVar(1)). TVar(1)`. But which TVar(1)?

In the scope of the innermost `TPi("_", ...)`, TVar(1) refers to `B` (from outer Pi). So the pair type `p` is `Σ(_:B). B` — that's wrong! It should be `Σ(_:A). B`.

The problem is in my earlier trace. Let me redo more carefully. After `(B:U0)->` is processed:
- env = `["B", "A"]` — B=TVar(0), A=TVar(1)

Parsing `A * B` in `parse_sigma`:
- Left: parse `A` → TVar(1) ✓ (A is at index 1)
- sees `*`, push `"_"` → env `["_","B","A"]`
- Right: parse `B` → TVar(?) — in env `["_","B","A"]`, B is at index 1 → TVar(1) ✓
- pop `"_"` → env `["B","A"]`
- Result: `TSigma("_", TVar(1), TVar(1))`

In the term, this is `Σ(_:A). #1-in-body`. The body of the sigma has `"_"` bound (at index 0 in body). `TVar(1)` in the body refers to... in the body scope `["_","B","A"]`, `TVar(1)` = B. So `Σ(_:A). B` ✓ 

Now parsing `B * A` in the `->` right side. Env is `["_","B","A"]` (the `_` from `->` is pushed).

In `parse_sigma`:
- Left: parse `B` → in `["_","B","A"]`, B=TVar(1). ✓
- sees `*`, push `"_"` → env `["_","_","B","A"]`
- Right: parse `A` → in `["_","_","B","A"]`, A=TVar(3) ✓
- pop `"_"` → env `["_","B","A"]`
- Result: `TSigma("_", TVar(1), TVar(3))`

In the `->` right body (scope for `p`), env is `["_","B","A"]`. `TVar(1)` = B ✓. The body of this sigma has `"_"` at index 0 in scope `["_","_","B","A"]`, and `TVar(3)` = A ✓. So this is `Σ(_:B). A`. ✓

Final arrow (with `->` binder `"_"` in scope):
`TPi("_", TSigma("_",TVar(1),TVar(1)), TSigma("_",TVar(1),TVar(3)))`

Reading this in scope `["B","A"]`:
- Domain: `TSigma("_", TVar(1), TVar(1))` = `Σ(_:A). B` ✓ (TVar(1) in outer scope = A; TVar(1) in body scope `["_","B","A"]` = B ✓)
- Codomain (with `_:Σ(_:A).B` at TVar(0), B at TVar(1), A at TVar(2)):
  - `TSigma("_", TVar(1), TVar(3))` in scope `["_","B","A"]`: `Σ(_:B). A` where TVar(1)=B, TVar(3) in `["_","_","B","A"]`=A ✓

So the parsed type IS `Π(A:U0). Π(B:U0). Π(_:Σ(_:A).B). Σ(_:B).A` which is correct for swap! ✓

Now why is there a type mismatch `expected #1, got fst #0`?

Let me check the typechecking of `\A B p. (snd p, fst p)` against this type. 

The check process:
1. Check `TAbs("A", TAbs("B", TAbs("p", TPair(TSnd(TVar(0)), TFst(TVar(0))))))` against `TPi("A",U0,TPi("B",U0,TPi("_",TSigma("_",TVar(1),TVar(1)),TSigma("_",TVar(1),TVar(3)))))`

Wait — after `apply_globals`, the type has globals substituted. A, B, etc. are de Bruijn indices. Let me check `apply_globals`:

Viewed env.rs:1-125

Now let me understand the type mismatch. The error says `expected #1, got fst #0`. In the context, `#1` means `TVar(1)`. This is during the pair check of `(snd p, fst p)`.

When checking `TPair(TSnd(TVar(0)), TFst(TVar(0)))` against `TSigma("_", TVar(1), TVar(3))`:

Looking at `check_dt` for `TPair`:

Viewed typechecker.rs:1300-1476