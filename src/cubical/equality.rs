// Cubical Equality — Rust port of Equality.hs
//
// Depends on:
//   crate::interval::I
//   crate::syntax::{Term, Name, shift, beta}
//   crate::eval::{eval, is_top_dnf, is_bot_dnf}

use crate::cubical::interval::I;
use crate::cubical::syntax::{Term, Name, shift, beta};
use crate::cubical::eval::{eval, is_top_dnf, is_bot_dnf};

/// A context maps de Bruijn names to their types.
pub type Ctx = Vec<(Name, Term)>;

// ---------------------------------------------------------------------------
// Term size (fuel derivation)
// ---------------------------------------------------------------------------

/// Structural node count of a term. Used to derive the initial fuel for
/// `eta_eq`; see `initial_fuel` for the termination argument.
pub fn term_size(t: &Term) -> usize {
    match t {
        Term::TVar(_)
        | Term::TUniv(_)
        | Term::TIntervalTy
        | Term::TInterval(_)
        | Term::TCube(_)           => 1,

        Term::TAbs(_, b)
        | Term::PLam(_, b)
        | Term::TUa(b)
        | Term::TFst(b)
        | Term::TSnd(b)            => 1 + term_size(b),

        Term::TApp(f, a)
        | Term::PApp(f, a)
        | Term::TEquiv(f, a)
        | Term::TEquivFwd(f, a)
        | Term::TTransport(f, a)
        | Term::TPair(f, a)        => 1 + term_size(f) + term_size(a),

        Term::TPi(_, a, b)
        | Term::TSigma(_, a, b)    => 1 + term_size(a) + term_size(b),

        Term::TPath(a, u, v)
        | Term::TGlue(a, u, v)
        | Term::TGlueElem(a, u, v)
        | Term::TUnglue(a, u, v)   => 1 + term_size(a) + term_size(u) + term_size(v),

        Term::THComp(a, ph, u, u0) =>
            1 + term_size(a) + term_size(ph) + term_size(u) + term_size(u0),

        Term::TMkEquiv(a, b, f, g, e, s) =>
            1 + term_size(a) + term_size(b) + term_size(f)
              + term_size(g) + term_size(e) + term_size(s),
    }
}

/// Starting fuel for an eta-equality check.
/// Floor of 16 ensures small terms get reasonable headroom.
pub fn initial_fuel(t1: &Term, t2: &Term) -> usize {
    (term_size(t1) + term_size(t2)).max(16)
}

// ---------------------------------------------------------------------------
// Eta-equality result
// ---------------------------------------------------------------------------

/// Three-valued result of an eta-equality check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtaResult {
    /// The two terms are definitionally equal.
    Equal,
    /// The two terms are definitionally distinct.
    NotEqual,
    /// Fuel ran out before a verdict was reached (inconclusive).
    Exhausted,
}

/// Conjunctive combination: both sides must be `Equal`.
/// `Exhausted` is infectious; `NotEqual` beats `Equal` but loses to `Exhausted`.
pub fn and_result(a: EtaResult, b: EtaResult) -> EtaResult {
    use EtaResult::*;
    match (a, b) {
        (Equal,     r)          => r,
        (r,         Equal)      => r,
        (Exhausted, _)          => Exhausted,
        (_,         Exhausted)  => Exhausted,
        (NotEqual,  NotEqual)   => NotEqual,
    }
}

// ---------------------------------------------------------------------------
// Context-free definitional equality
// ---------------------------------------------------------------------------

pub fn definitionally_equal(t1: &Term, t2: &Term) -> bool {
    let v1 = eval(t1);
    let v2 = eval(t2);
    v1 == v2 || eta_eq(initial_fuel(&v1, &v2), &Vec::new(), &v1, &v2) == EtaResult::Equal
}

pub fn definitionally_equal_ctx(ctx: &Ctx, t1: &Term, t2: &Term) -> bool {
    let v1 = eval(t1);
    let v2 = eval(t2);
    v1 == v2 || eta_eq(initial_fuel(&v1, &v2), ctx, &v1, &v2) == EtaResult::Equal
}

/// Like `definitionally_equal_ctx` but surfaces fuel exhaustion as a distinct
/// `EtaResult` so callers can emit a proper error.
pub fn definitionally_equal_ctx_r(ctx: &Ctx, t1: &Term, t2: &Term) -> EtaResult {
    let v1 = eval(t1);
    let v2 = eval(t2);
    if v1 == v2 {
        EtaResult::Equal
    } else {
        eta_eq(initial_fuel(&v1, &v2), ctx, &v1, &v2)
    }
}

// ---------------------------------------------------------------------------
// Path boundary reduction
// ---------------------------------------------------------------------------

/// If `p : Path A u v` and `r` is `I0` / `I1`, return the endpoint.
pub fn reduce_papp_by_type(ctx: &Ctx, p: &Term, r: &Term) -> Option<Term> {
    match infer_ty(ctx, p) {
        Some(Term::TPath(_, u, v)) => {
            let r_ = eval(r);
            if is_bot_dnf(&r_) || r_ == Term::TInterval(I::I0) {
                Some(eval(&u))
            } else if is_top_dnf(&r_) || r_ == Term::TInterval(I::I1) {
                Some(eval(&v))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn infer_ty(ctx: &Ctx, t: &Term) -> Option<Term> {
    match t {
        Term::TVar(i) => {
            let i = *i as usize;
            if i < ctx.len() {
                Some(eval(&shift((i + 1) as i32, 0, &ctx[i].1)))
            } else {
                None
            }
        }
        Term::TApp(f, a) => match infer_ty(ctx, f) {
            Some(Term::TPi(_, _, b_ty)) => Some(eval(&beta(&b_ty, a))),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Lightweight neutral type inference
// ---------------------------------------------------------------------------

fn infer_neutral_ty(ctx: &Ctx, t: &Term) -> Option<Term> {
    match t {
        Term::TVar(i) => {
            let i = *i as usize;
            if i < ctx.len() {
                Some(eval(&shift((i + 1) as i32, 0, &ctx[i].1)))
            } else {
                None
            }
        }
        Term::TApp(f, a) => match infer_neutral_ty(ctx, f) {
            Some(Term::TPi(_, _, b_ty)) => Some(eval(&beta(&b_ty, a))),
            _ => None,
        },
        _ => None,
    }
}

/// Try to infer the Pi domain of `neutral` from the context, to use as the
/// type of the fresh variable introduced when eta-expanding `neutral` against
/// a lambda. Returns `None` when the type cannot be determined.
pub fn infer_lam_dom(ctx: &Ctx, neutral: &Term) -> Option<Term> {
    match infer_neutral_ty(ctx, neutral) {
        Some(Term::TPi(_, dom_ty, _)) => Some(eval(&dom_ty)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Core eta-equality
// ---------------------------------------------------------------------------

/// `eta_eq(fuel, ctx, t1, t2)` checks whether `t1` and `t2` are
/// definitionally equal under `ctx`, consuming `fuel` for eta-expansion steps.
pub fn eta_eq(fuel: usize, ctx: &Ctx, t1: &Term, t2: &Term) -> EtaResult {
    use EtaResult::*;

    if fuel == 0 {
        return Exhausted;
    }

    if t1 == t2 {
        return Equal;
    }

    // ------------------------------------------------------------------
    // Path boundary reduction (consumes fuel)
    // ------------------------------------------------------------------
    if let Term::PApp(p, r) = t1 {
        if let Some(u) = reduce_papp_by_type(ctx, p, r) {
            return eta_eq(fuel - 1, ctx, &u, t2);
        }
    }
    if let Term::PApp(p, r) = t2 {
        if let Some(u) = reduce_papp_by_type(ctx, p, r) {
            return eta_eq(fuel - 1, ctx, t1, &u);
        }
    }

    // ------------------------------------------------------------------
    // Lambda eta (consumes fuel)
    // ------------------------------------------------------------------

    // Both sides are lambdas.
    if let (Term::TAbs(x, b1), Term::TAbs(_, b2)) = (t1, t2) {
        let dom = infer_lam_dom(ctx, t1)
            .or_else(|| infer_lam_dom(ctx, t2))
            .unwrap_or(Term::TUniv(0));
        let mut ctx2 = vec![(x.clone(), dom)];
        ctx2.extend_from_slice(ctx);
        return eta_eq(fuel - 1, &ctx2, &eval(b1), &eval(b2));
    }

    // Only RHS is a lambda — eta-expand neutral LHS.
    if let Term::TAbs(x, b2) = t2 {
        return match infer_lam_dom(ctx, t1) {
            None => Exhausted,
            Some(dom) => {
                let mut ctx2 = vec![(x.clone(), dom)];
                ctx2.extend_from_slice(ctx);
                eta_eq(
                    fuel - 1, &ctx2,
                    &eval(&Term::TApp(Box::new(shift(1, 0, t1)), Box::new(Term::TVar(0)))),
                    &eval(b2),
                )
            }
        };
    }

    // Only LHS is a lambda — eta-expand neutral RHS.
    if let Term::TAbs(x, b1) = t1 {
        return match infer_lam_dom(ctx, t2) {
            None => Exhausted,
            Some(dom) => {
                let mut ctx2 = vec![(x.clone(), dom)];
                ctx2.extend_from_slice(ctx);
                eta_eq(
                    fuel - 1, &ctx2,
                    &eval(b1),
                    &eval(&Term::TApp(Box::new(shift(1, 0, t2)), Box::new(Term::TVar(0)))),
                )
            }
        };
    }

    // ------------------------------------------------------------------
    // Path-lambda eta (consumes fuel)
    // ------------------------------------------------------------------

    // Both sides are path-lambdas.
    if let (Term::PLam(i, b1), Term::PLam(_, b2)) = (t1, t2) {
        let mut ctx2 = vec![(i.clone(), Term::TIntervalTy)];
        ctx2.extend_from_slice(ctx);
        return eta_eq(fuel - 1, &ctx2, &eval(b1), &eval(b2));
    }

    // Only RHS is a path-lambda.
    if let Term::PLam(i, b2) = t2 {
        let mut ctx2 = vec![(i.clone(), Term::TIntervalTy)];
        ctx2.extend_from_slice(ctx);
        return eta_eq(
            fuel - 1, &ctx2,
            &eval(&Term::PApp(Box::new(shift(1, 0, t1)), Box::new(Term::TVar(0)))),
            &eval(b2),
        );
    }

    // Only LHS is a path-lambda.
    if let Term::PLam(i, b1) = t1 {
        let mut ctx2 = vec![(i.clone(), Term::TIntervalTy)];
        ctx2.extend_from_slice(ctx);
        return eta_eq(
            fuel - 1, &ctx2,
            &eval(b1),
            &eval(&Term::PApp(Box::new(shift(1, 0, t2)), Box::new(Term::TVar(0)))),
        );
    }

    // ------------------------------------------------------------------
    // Congruence on neutral spines (structural: no fuel consumed)
    // ------------------------------------------------------------------
    if let (Term::TApp(f1, a1), Term::TApp(f2, a2)) = (t1, t2) {
        return and_result(eta_eq(fuel, ctx, f1, f2), eta_eq(fuel, ctx, a1, a2));
    }
    if let (Term::PApp(p1, r1), Term::PApp(p2, r2)) = (t1, t2) {
        return and_result(eta_eq(fuel, ctx, p1, p2), eta_eq(fuel, ctx, r1, r2));
    }

    // ------------------------------------------------------------------
    // Type congruence (structural: no fuel consumed)
    // ------------------------------------------------------------------
    if let (Term::TPi(_, a1, b1), Term::TPi(_, a2, b2)) = (t1, t2) {
        return and_result(eta_eq(fuel, ctx, a1, a2), eta_eq(fuel, ctx, b1, b2));
    }
    if let (Term::TPath(ty1, u1, v1), Term::TPath(ty2, u2, v2)) = (t1, t2) {
        return and_result(
            and_result(eta_eq(fuel, ctx, ty1, ty2), eta_eq(fuel, ctx, u1, u2)),
            eta_eq(fuel, ctx, v1, v2),
        );
    }
    if let (Term::TSigma(_, a1, b1), Term::TSigma(_, a2, b2)) = (t1, t2) {
        return and_result(eta_eq(fuel, ctx, a1, a2), eta_eq(fuel, ctx, b1, b2));
    }

    // ------------------------------------------------------------------
    // Pair congruence (structural)
    // ------------------------------------------------------------------
    if let (Term::TPair(a1, b1), Term::TPair(a2, b2)) = (t1, t2) {
        return and_result(eta_eq(fuel, ctx, a1, a2), eta_eq(fuel, ctx, b1, b2));
    }

    // ------------------------------------------------------------------
    // Sigma eta: one side is a pair, the other is neutral (consumes fuel)
    // ------------------------------------------------------------------
    if let Term::TPair(a1, b1) = t1 {
        return and_result(
            eta_eq(fuel - 1, ctx, a1, &eval(&Term::TFst(Box::new(t2.clone())))),
            eta_eq(fuel - 1, ctx, b1, &eval(&Term::TSnd(Box::new(t2.clone())))),
        );
    }
    if let Term::TPair(a2, b2) = t2 {
        return and_result(
            eta_eq(fuel - 1, ctx, &eval(&Term::TFst(Box::new(t1.clone()))), a2),
            eta_eq(fuel - 1, ctx, &eval(&Term::TSnd(Box::new(t1.clone()))), b2),
        );
    }

    // ------------------------------------------------------------------
    // Projection congruence on neutral spines (structural)
    // ------------------------------------------------------------------
    if let (Term::TFst(p1), Term::TFst(p2)) = (t1, t2) {
        return eta_eq(fuel, ctx, p1, p2);
    }
    if let (Term::TSnd(p1), Term::TSnd(p2)) = (t1, t2) {
        return eta_eq(fuel, ctx, p1, p2);
    }

    NotEqual
}