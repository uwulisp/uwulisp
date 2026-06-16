// Cubical Env — Rust port of Env.hs
//
// Depends on:
//   crate::syntax::{Name, Term, shift, subst}
//   crate::typechecker::{Ctx, TypeError, infer, check}

use crate::cubical::syntax::{Name, Term, shift, subst};
use crate::cubical::typechecker::{Ctx, TypeError, infer, check};

// ---------------------------------------------------------------------------
// Global Named Environment
// ---------------------------------------------------------------------------

/// A global definition: `(name, type, value)`.
/// Stored most-recent first.
pub type GlobalEnv = Vec<(Name, Term, Term)>;

/// Build a `Ctx` from a `GlobalEnv`.
/// Variables are ordered innermost-first, so we reverse the env.
pub fn global_ctx(genv: &GlobalEnv) -> Ctx {
    genv.iter()
        .rev()
        .map(|(name, ty, _)| (name.clone(), ty.clone()))
        .collect()
}

/// Substitute all global definitions into a term directly via de Bruijn
/// substitution, rather than wrapping in `TApp`/`TAbs` chains.
///
/// The parser assigns globals indices starting at `length localEnv`.
/// At the top level `localEnv` is empty, so globals occupy indices `0..n-1`
/// with the most-recent global at index 0.
///
/// We substitute one global at a time, outermost (highest index) first,
/// so that earlier substitutions don't disturb the indices of later ones.
/// After substituting index `k`, we shift the term down by 1 to close the gap.
pub fn apply_globals(genv: &GlobalEnv, t: &Term) -> Term {
    let n = genv.len();

    // `genv` is most-recent first; reversing gives oldest first.
    // Oldest global has the highest index (n-1), newest has index 0.
    let vals: Vec<&Term> = genv.iter().rev().map(|(_, _, v)| v).collect();

    // Pair each value with its de Bruijn index: (n-1, vals[0]), (n-2, vals[1]), ...
    // Then fold right (outermost / highest index first).
    let indexed_vals: Vec<(i32, &Term)> = (0..n as i32)
        .rev()
        .zip(vals.iter().copied())
        .collect();

    // `foldr substGlobal t indexedVals` in Haskell processes the list
    // left-to-right but applies the function from the right.  Because
    // `indexedVals` is already ordered highest-index first, a left fold
    // (`iter().fold`) gives the same traversal order.
    indexed_vals.iter().fold(t.clone(), |body, (k, v)| {
        subst_global(*k, v, &body)
    })
}

/// Substitute the global at de Bruijn index `k` with its value `v`,
/// then shift the whole term down by 1 to account for the removed binding.
fn subst_global(k: i32, v: &Term, body: &Term) -> Term {
    shift(-1, k, &subst(k, &shift(k, 0, v), body))
}

/// Infer the type of a term in the context of a `GlobalEnv`.
pub fn infer_with_env(genv: &GlobalEnv, t: &Term) -> Result<Term, TypeError> {
    infer(&global_ctx(genv), t)
}

/// Check a term against a type in the context of a `GlobalEnv`.
pub fn check_with_env(genv: &GlobalEnv, t: &Term, ty: &Term) -> Result<(), TypeError> {
    check(&global_ctx(genv), t, ty)
}