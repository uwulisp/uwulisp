// Cubical TypeChecker — Rust port of typechecker.hs
//
// Depends on:
//   crate::interval::{I, DNF, Literal}
//   crate::syntax::{Term, Name, Level, shift, subst, beta, show_term}
//   crate::eval::{eval, is_top_dnf, is_bot_dnf}
//   crate::equality::{definitionally_equal_ctx, definitionally_equal_ctx_r, EtaResult}

use std::collections::BTreeSet;
use std::fmt;

use crate::cubical::interval::{I, DNF, Literal};
use crate::cubical::syntax::{Term, Name, Level, shift, beta, show_term};
use crate::cubical::eval::{eval, is_top_dnf, is_bot_dnf};
use crate::cubical::equality::{definitionally_equal_ctx_r, EtaResult};

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

pub type Ctx = Vec<(Name, Term)>;

fn interval_ty() -> Term {
    Term::TIntervalTy
}

pub fn extend_ctx(x: Name, ty: Term, ctx: &Ctx) -> Ctx {
    let mut ctx2 = vec![(x, ty)];
    ctx2.extend_from_slice(ctx);
    ctx2
}

pub fn lookup_ctx(i: i32, ctx: &Ctx) -> Result<Term, TypeError> {
    if i < 0 || i as usize >= ctx.len() {
        Err(TypeError::UnboundVariable(format!("#{}", i)))
    } else {
        Ok(eval(&shift(i + 1, 0, &ctx[i as usize].1)))
    }
}

// ---------------------------------------------------------------------------
// TypeError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnboundVariable(Name),
    TypeMismatch(Term, Term),
    ExpectedPi(Term),
    ExpectedPath(Term),
    ExpectedUniverse(Term),
    ExpectedEquiv(Term),
    ExpectedSigma(Term),
    NotAnInterval(Term),
    CannotInfer(Term),
    EtaFuelExhausted(Term, Term),
    Other(String),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnboundVariable(x) =>
                write!(f, "  Unbound variable: '{}'", x),
            TypeError::TypeMismatch(ex, got) =>
                write!(f, "  Type mismatch\n    expected : {}\n    got      : {}", ex, got),
            TypeError::ExpectedPi(ty) =>
                write!(f, "  Expected a Π-type, but found:\n    {}", ty),
            TypeError::ExpectedPath(ty) =>
                write!(f, "  Expected a Path type, but found:\n    {}", ty),
            TypeError::ExpectedUniverse(ty) =>
                write!(f, "  Expected a universe U_n, but found:\n    {}", ty),
            TypeError::ExpectedEquiv(ty) =>
                write!(f, "  Expected an Equiv type, but found:\n    {}", ty),
            TypeError::ExpectedSigma(ty) =>
                write!(f, "  Expected a Σ-type, but found:\n    {}", ty),
            TypeError::NotAnInterval(t) =>
                write!(f, "  Expected an interval expression (𝕀), but got:\n    {}", t),
            TypeError::CannotInfer(t) =>
                write!(
                    f,
                    "  Cannot infer type of term without annotation:\n    {}\n  \
                     (Tip: use 'check' instead of 'infer', or add a type annotation)",
                    t
                ),
            TypeError::EtaFuelExhausted(t1, t2) =>
                write!(
                    f,
                    "  Eta-equality check ran out of fuel (terms may be equal but are too\n  \
                     deeply nested to decide automatically).\n    lhs : {}\n    rhs : {}",
                    t1, t2
                ),
            TypeError::Other(msg) =>
                write!(f, "  {}", msg),
        }
    }
}

// ---------------------------------------------------------------------------
// Require helpers
// ---------------------------------------------------------------------------

pub fn require_equal(ctx: &Ctx, expected: &Term, got: &Term) -> Result<(), TypeError> {
    match definitionally_equal_ctx_r(ctx, expected, got) {
        EtaResult::Equal     => Ok(()),
        EtaResult::NotEqual  => Err(TypeError::TypeMismatch(eval(expected), eval(got))),
        EtaResult::Exhausted => Err(TypeError::EtaFuelExhausted(eval(expected), eval(got))),
    }
}

pub fn require_equal_endpt(ctx: &Ctx, expected: &Term, got: &Term) -> Result<(), TypeError> {
    match definitionally_equal_ctx_r(ctx, expected, got) {
        EtaResult::Equal => Ok(()),
        EtaResult::NotEqual => {
            let names: Vec<Name> = ctx.iter().map(|(n, _)| n.clone()).collect();
            Err(TypeError::Other(format!(
                "endpoint mismatch (ctx_depth={}, ctx={:?})\
                 \n  expected={}  [raw={}]\
                 \n  got={}  [raw={}]",
                ctx.len(),
                ctx.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
                show_term(&names, &eval(expected)),
                eval(expected),
                show_term(&names, &eval(got)),
                eval(got),
            )))
        }
        EtaResult::Exhausted =>
            Err(TypeError::EtaFuelExhausted(eval(expected), eval(got))),
    }
}

pub fn require_universe(ctx: &Ctx, t: &Term) -> Result<Level, TypeError> {
    let ty = infer(ctx, t)?;
    match eval(&ty) {
        Term::TUniv(n) => Ok(n),
        other          => Err(TypeError::ExpectedUniverse(other)),
    }
}

pub fn check_interval(ctx: &Ctx, t: &Term) -> Result<(), TypeError> {
    match t {
        Term::TInterval(_) | Term::TCube(_) => return Ok(()),
        _ => {}
    }
    let ty = infer(ctx, t)?;
    if ty == interval_ty() {
        Ok(())
    } else {
        Err(TypeError::NotAnInterval(t.clone()))
    }
}

pub fn require_equiv(ctx: &Ctx, t: &Term) -> Result<(Term, Term), TypeError> {
    let ty = infer(ctx, t)?;
    match eval(&ty) {
        Term::TEquiv(a, b) => Ok((eval(&a), eval(&b))),
        other              => Err(TypeError::ExpectedEquiv(other)),
    }
}

// ---------------------------------------------------------------------------
// Face-restriction helpers
// ---------------------------------------------------------------------------

/// Apply a single DNF literal as a substitution on a term.
/// `Pos n`    → iₙ = 1   (IVar n ↦ I1)
/// `NegVar n` → iₙ = 0   (IVar n ↦ I0)
pub fn apply_literal(lit: &Literal, t: &Term) -> Term {
    let (n, val) = match lit {
        Literal::Pos(k)    => (*k, I::I1),
        Literal::NegVar(k) => (*k, I::I0),
    };

    fn go_i(i: &I, n: i32, val: &I) -> I {
        match i {
            I::IVar(k) if *k == n => val.clone(),
            I::Meet(a, b)         => I::Meet(Box::new(go_i(a, n, val)), Box::new(go_i(b, n, val))),
            I::Join(a, b)         => I::Join(Box::new(go_i(a, n, val)), Box::new(go_i(b, n, val))),
            I::Neg(a)             => I::Neg(Box::new(go_i(a, n, val))),
            other                 => other.clone(),
        }
    }

    fn go(t: &Term, n: i32, val: &I) -> Term {
        match t {
            Term::TInterval(i) =>
                eval(&Term::TInterval(go_i(i, n, val))),

            Term::TCube(DNF { cubes }) => {
                // Substitute the literal into each cube then re-normalise.
                let subst_lit = |l: &Literal| -> I {
                    match l {
                        Literal::Pos(k)    => go_i(&I::IVar(*k), n, val),
                        Literal::NegVar(k) => I::Neg(Box::new(go_i(&I::IVar(*k), n, val))),
                    }
                };
                let subst_cube = |c: &BTreeSet<Literal>| -> I {
                    c.iter().fold(I::I1, |acc, l| I::Meet(Box::new(subst_lit(l)), Box::new(acc)))
                };
                let combined = cubes.iter().fold(I::I0, |acc, c| {
                    I::Join(Box::new(subst_cube(c)), Box::new(acc))
                });
                eval(&Term::TInterval(combined))
            }

            Term::TApp(f, a) =>
                eval(&Term::TApp(Box::new(go(f, n, val)), Box::new(go(a, n, val)))),
            Term::TAbs(x, b) =>
                Term::TAbs(x.clone(), Box::new(go(b, n, val))),
            Term::TPi(x, a, b) =>
                Term::TPi(x.clone(), Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TPath(a, u, v) =>
                Term::TPath(Box::new(go(a, n, val)), Box::new(go(u, n, val)), Box::new(go(v, n, val))),
            Term::PLam(i, b) =>
                Term::PLam(i.clone(), Box::new(go(b, n, val))),
            Term::PApp(p, r) =>
                eval(&Term::PApp(Box::new(go(p, n, val)), Box::new(go(r, n, val)))),
            Term::THComp(a, ph, u, u0) =>
                eval(&Term::THComp(
                    Box::new(go(a, n, val)), Box::new(go(ph, n, val)),
                    Box::new(go(u, n, val)), Box::new(go(u0, n, val)),
                )),
            Term::TEquiv(a, b) =>
                Term::TEquiv(Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TMkEquiv(a, b, f, g, eta, eps) =>
                Term::TMkEquiv(
                    Box::new(go(a, n, val)), Box::new(go(b, n, val)),
                    Box::new(go(f, n, val)), Box::new(go(g, n, val)),
                    Box::new(go(eta, n, val)), Box::new(go(eps, n, val)),
                ),
            Term::TEquivFwd(e, x) =>
                eval(&Term::TEquivFwd(Box::new(go(e, n, val)), Box::new(go(x, n, val)))),
            Term::TUa(e) =>
                Term::TUa(Box::new(go(e, n, val))),
            Term::TTransport(p, x) =>
                eval(&Term::TTransport(Box::new(go(p, n, val)), Box::new(go(x, n, val)))),
            Term::TGlue(a, ph, te) =>
                eval(&Term::TGlue(
                    Box::new(go(a, n, val)), Box::new(go(ph, n, val)), Box::new(go(te, n, val)),
                )),
            Term::TGlueElem(ph, x, a) =>
                eval(&Term::TGlueElem(
                    Box::new(go(ph, n, val)), Box::new(go(x, n, val)), Box::new(go(a, n, val)),
                )),
            Term::TUnglue(ph, te, g) =>
                eval(&Term::TUnglue(
                    Box::new(go(ph, n, val)), Box::new(go(te, n, val)), Box::new(go(g, n, val)),
                )),
            Term::TSigma(x, a, b) =>
                Term::TSigma(x.clone(), Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TPair(a, b) =>
                Term::TPair(Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TFst(p) =>
                eval(&Term::TFst(Box::new(go(p, n, val)))),
            Term::TSnd(p) =>
                eval(&Term::TSnd(Box::new(go(p, n, val)))),
            // TVar, TUniv, TIntervalTy: no interval vars
            other => other.clone(),
        }
    }

    go(t, n, &val)
}

/// Check that `tube_at0 ≡ base` on every face of `phi`'s DNF.
fn check_faces(
    ctx: &Ctx,
    phi: &Term,
    tube_at0: &Term,
    base: &Term,
) -> Result<(), TypeError> {
    match phi {
        Term::TCube(DNF { cubes }) => {
            for cube in cubes {
                // Apply all literals in the cube as substitutions.
                let apply_all = |t: &Term| -> Term {
                    cube.iter().fold(t.clone(), |acc, lit| apply_literal(lit, &acc))
                };
                let lhs = eval(&apply_all(tube_at0));
                let rhs = eval(&apply_all(base));
                require_equal_endpt(ctx, &lhs, &rhs)?;
            }
            Ok(())
        }
        // Non-DNF phi: fall back to a direct equality check.
        _ => require_equal_endpt(ctx, tube_at0, base),
    }
}

// ---------------------------------------------------------------------------
// Type Inference
// ---------------------------------------------------------------------------

pub fn infer(ctx: &Ctx, t: &Term) -> Result<Term, TypeError> {
    match t {
        // Variable
        Term::TVar(i) => lookup_ctx(*i, ctx),

        // Universe: U_n : U_{n+1}
        Term::TUniv(n) => Ok(Term::TUniv(n + 1)),

        // Application: f a  where  f : Π(x:A).B
        Term::TApp(f, a) => {
            let f_ty = infer(ctx, f)?;
            match eval(&f_ty) {
                Term::TPi(_, a_ty, b_ty) => {
                    check(ctx, a, &a_ty)?;
                    Ok(eval(&beta(&b_ty, a)))
                }
                other => Err(TypeError::ExpectedPi(other)),
            }
        }

        // Pi formation: Π(x:A).B : U(max i j)
        Term::TPi(x, a_ty, b_ty) => {
            let i = require_universe(ctx, a_ty)?;
            let ctx2 = extend_ctx(x.clone(), eval(a_ty), ctx);
            let j = require_universe(&ctx2, b_ty)?;
            Ok(Term::TUniv(i.max(j)))
        }

        // Path type: Path A u v : U n
        Term::TPath(a_ty, u, v) => {
            let n = require_universe(ctx, a_ty)?;
            let a_ty_ = eval(a_ty);
            let u_ty = match &a_ty_ {
                Term::PLam(_, body) => eval(&beta(body, &Term::TInterval(I::I0))),
                p                   => p.clone(),
            };
            let v_ty = match &a_ty_ {
                Term::PLam(_, body) => eval(&beta(body, &Term::TInterval(I::I1))),
                p                   => p.clone(),
            };
            check(ctx, u, &u_ty)?;
            check(ctx, v, &v_ty)?;
            Ok(Term::TUniv(n))
        }

        // Path application: p @ r
        Term::PApp(p, r) => {
            let p_ty = infer(ctx, p)?;
            match eval(&p_ty) {
                Term::TPath(a_ty, _, _) => {
                    check_interval(ctx, r)?;
                    let r_ = eval(r);
                    Ok(match eval(&a_ty) {
                        Term::PLam(_, body) => eval(&beta(&body, &r_)),
                        plain               => plain,
                    })
                }
                other => Err(TypeError::ExpectedPath(other)),
            }
        }

        // Interval atoms
        Term::TInterval(_) | Term::TCube(_) => Ok(interval_ty()),
        Term::TIntervalTy                   => Ok(Term::TUniv(0)),

        // Lambdas cannot be inferred
        t @ Term::TAbs(_, _) | t @ Term::PLam(_, _) => Err(TypeError::CannotInfer(t.clone())),

        // Equiv type
        Term::TEquiv(a, b) => {
            let n = require_universe(ctx, a)?;
            let m = require_universe(ctx, b)?;
            Ok(Term::TUniv(n.max(m)))
        }

        // mkEquiv: build an equivalence record
        Term::TMkEquiv(a, b, f, g, eta, eps) => {
            require_universe(ctx, a)?;
            require_universe(ctx, b)?;
            let a_ = eval(a);
            let b_ = eval(b);
            // f : A → B
            check(ctx, f, &Term::TPi("_".into(), Box::new(a_.clone()), Box::new(shift(1, 0, &b_))))?;
            // g : B → A
            check(ctx, g, &Term::TPi("_".into(), Box::new(b_.clone()), Box::new(shift(1, 0, &a_))))?;
            // eta : (a : A) → Path A a (g (f a))
            check(ctx, eta, &Term::TPi(
                "a".into(),
                Box::new(a_.clone()),
                Box::new(Term::TPath(
                    Box::new(shift(1, 0, &a_)),
                    Box::new(Term::TVar(0)),
                    Box::new(Term::TApp(
                        Box::new(shift(1, 0, g)),
                        Box::new(Term::TApp(Box::new(shift(1, 0, f)), Box::new(Term::TVar(0)))),
                    )),
                )),
            ))?;
            // eps : (b : B) → Path B (f (g b)) b
            check(ctx, eps, &Term::TPi(
                "b".into(),
                Box::new(b_.clone()),
                Box::new(Term::TPath(
                    Box::new(shift(1, 0, &b_)),
                    Box::new(Term::TApp(
                        Box::new(shift(1, 0, f)),
                        Box::new(Term::TApp(Box::new(shift(1, 0, g)), Box::new(Term::TVar(0)))),
                    )),
                    Box::new(Term::TVar(0)),
                )),
            ))?;
            Ok(Term::TEquiv(Box::new(a_), Box::new(b_)))
        }

        // equivFwd e x : B   where  e : Equiv A B,  x : A
        Term::TEquivFwd(e, x) => {
            let (a, b) = require_equiv(ctx, e)?;
            check(ctx, x, &a)?;
            Ok(b)
        }

        // ua e : Path U A B   where  e : Equiv A B
        Term::TUa(e) => {
            let (a, b) = require_equiv(ctx, e)?;
            let n = require_universe(ctx, &a)?;
            Ok(Term::TPath(Box::new(Term::TUniv(n)), Box::new(a), Box::new(b)))
        }

        // transport p x : B   where  p : Path U A B,  x : A
        Term::TTransport(p, x) => {
            let p_ty = infer(ctx, p)?;
            match eval(&p_ty) {
                Term::TPath(a_ty, _, _) => {
                    let (x_ty, ret_ty) = match eval(&a_ty) {
                        Term::PLam(_, body) => (
                            eval(&beta(&body, &Term::TInterval(I::I0))),
                            eval(&beta(&body, &Term::TInterval(I::I1))),
                        ),
                        plain => (plain.clone(), plain),
                    };
                    check(ctx, x, &x_ty)?;
                    Ok(ret_ty)
                }
                other => Err(TypeError::ExpectedPath(other)),
            }
        }

        // Glue type formation
        Term::TGlue(a_ty, phi, te) => {
            let n    = require_universe(ctx, a_ty)?;
            let a_ty_ = eval(a_ty);
            check_interval(ctx, phi)?;
            let te_ty = infer(ctx, te)?;
            let m = match eval(&te_ty) {
                Term::TUniv(k) => k,
                Term::TEquiv(a, b) => {
                    let a_ = eval(&a);
                    let b_ = eval(&b);
                    require_equal(ctx, &b_, &a_ty_)?;
                    let p = require_universe(ctx, &a_)?;
                    let q = require_universe(ctx, &b_)?;
                    p.max(q)
                }
                Term::TMkEquiv(a, b, _, _, _, _) => {
                    let a_ = eval(&a);
                    let b_ = eval(&b);
                    require_equal(ctx, &b_, &a_ty_)?;
                    let p = require_universe(ctx, &a_)?;
                    let q = require_universe(ctx, &b_)?;
                    p.max(q)
                }
                other => return Err(TypeError::Other(format!(
                    "Glue: equivalence argument has unexpected type: {}", other
                ))),
            };
            Ok(Term::TUniv(n.max(m)))
        }

        // unglue phi te g
        Term::TUnglue(phi, te, g) => {
            check_interval(ctx, phi)?;
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                infer(ctx, &Term::TEquivFwd(te.clone(), g.clone()))
            } else if is_bot_dnf(&phi_) {
                infer(ctx, g)
            } else {
                let g_ty = infer(ctx, g)?;
                match eval(&g_ty) {
                    Term::TGlue(a_ty, _, _) => Ok(eval(&a_ty)),
                    other => Err(TypeError::Other(format!(
                        "unglue: expected argument of Glue type, got: {}", other
                    ))),
                }
            }
        }

        // glue elem — can only infer in degenerate phi cases
        t @ Term::TGlueElem(phi, elm, a) => {
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                infer(ctx, elm)
            } else if is_bot_dnf(&phi_) {
                infer(ctx, a)
            } else {
                Err(TypeError::CannotInfer(t.clone()))
            }
        }

        // Sigma formation: Σ(x:A).B : U(max i j)
        Term::TSigma(x, a_ty, b_ty) => {
            let i = require_universe(ctx, a_ty)?;
            let ctx2 = extend_ctx(x.clone(), eval(a_ty), ctx);
            let j = require_universe(&ctx2, b_ty)?;
            Ok(Term::TUniv(i.max(j)))
        }

        // fst p : A   where  p : Σ(x:A).B
        Term::TFst(p) => {
            let p_ty = infer(ctx, p)?;
            match eval(&p_ty) {
                Term::TSigma(_, a_ty, _) => Ok(eval(&a_ty)),
                other                    => Err(TypeError::ExpectedSigma(other)),
            }
        }

        // snd p : B[fst p / x]   where  p : Σ(x:A).B
        Term::TSnd(p) => {
            let p_ty = infer(ctx, p)?;
            match eval(&p_ty) {
                Term::TSigma(_, _, b_ty) =>
                    Ok(eval(&beta(&b_ty, &Term::TFst(p.clone())))),
                other => Err(TypeError::ExpectedSigma(other)),
            }
        }

        // Pairs cannot be inferred without annotation
        t @ Term::TPair(_, _) => Err(TypeError::CannotInfer(t.clone())),

        // hcomp A phi tube base
        Term::THComp(a_ty, phi, tube, base) => {
            require_universe(ctx, a_ty)?;
            let a_ty_ = eval(a_ty);
            check_interval(ctx, phi)?;
            check(ctx, base, &a_ty_)?;

            let phi_ = eval(phi);
            match eval(tube) {
                Term::PLam(i, body) => {
                    // (a) body : A in extended context
                    let ctx2   = extend_ctx(i.clone(), interval_ty(), ctx);
                    let a_ty_s = shift(1, 0, &a_ty_);
                    check(&ctx2, &body, &a_ty_s)?;
                    // (b) tube@0 ≡ base on each face of phi
                    let tube_at0 = eval(&beta(&body, &Term::TInterval(I::I0)));
                    check_faces(ctx, &phi_, &tube_at0, &eval(base))?;
                }
                tube_ => {
                    // Non-lambda tube: treat as Path A u v
                    let tube_ty = infer(ctx, &tube_)?;
                    match eval(&tube_ty) {
                        Term::TPath(a, u, v) => {
                            if !definitionally_equal_ctx_r(ctx, &eval(&a), &a_ty_)
                                .is_equal()
                            {
                                return Err(TypeError::TypeMismatch(eval(&a_ty_), eval(&a)));
                            }
                            check(ctx, &eval(&u), &a_ty_)?;
                            check(ctx, &eval(&v), &a_ty_)?;
                            check_faces(ctx, &phi_, &eval(&u), &eval(base))?;
                        }
                        other => return Err(TypeError::ExpectedPath(other)),
                    }
                }
            }
            Ok(a_ty_)
        }
    }
}

// ---------------------------------------------------------------------------
// Type Checking
// ---------------------------------------------------------------------------

pub fn check(ctx: &Ctx, t: &Term, ty: &Term) -> Result<(), TypeError> {
    match t {
        // Lambda introduction
        Term::TAbs(x, body) => match eval(ty) {
            Term::TPi(_, a_ty, b_ty) =>
                check(&extend_ctx(x.clone(), eval(&a_ty), ctx), body, &b_ty),
            other => Err(TypeError::ExpectedPi(other)),
        },

        // Path-lambda introduction
        Term::PLam(i, body) => match eval(ty) {
            Term::TPath(a_ty, u, v) => {
                let ctx2 = extend_ctx(i.clone(), interval_ty(), ctx);
                let body_ty = match eval(&a_ty) {
                    p @ Term::PLam { .. } => p,
                    plain                 => shift(1, 0, &plain),
                };
                let body_at0 = eval(&beta(body, &Term::TInterval(I::I0)));
                let body_at1 = eval(&beta(body, &Term::TInterval(I::I1)));
                require_equal_endpt(ctx, &eval(&u), &body_at0)?;
                require_equal_endpt(ctx, &eval(&v), &body_at1)?;
                check(&ctx2, body, &body_ty)
            }
            other => Err(TypeError::ExpectedPath(other)),
        },

        // GlueElem checking
        Term::TGlueElem(phi, t_inner, a) => match eval(ty) {
            Term::TGlue(a_ty, phi_, te) => {
                check_interval(ctx, phi)?;
                require_equal(ctx, &eval(&phi_), &eval(phi))?;
                let t_ty = match eval(&te) {
                    Term::TMkEquiv(dom_a, _, _, _, _, _) => eval(&dom_a),
                    Term::TEquiv(dom_a, _)               => eval(&dom_a),
                    other                                => other,
                };
                check(ctx, t_inner, &t_ty)?;
                check(ctx, a, &eval(&a_ty))
            }
            other => Err(TypeError::Other(format!(
                "glue: expected Glue type, got: {}", other
            ))),
        },

        // Pair introduction
        Term::TPair(a, b) => match eval(ty) {
            Term::TSigma(_, a_ty, b_ty) => {
                check(ctx, a, &eval(&a_ty))?;
                check(ctx, b, &eval(&beta(&b_ty, a)))
            }
            other => Err(TypeError::ExpectedSigma(other)),
        },

        // Fall through to inference
        t => {
            let ty_ = infer(ctx, t)?;
            require_equal(ctx, &eval(ty), &eval(&ty_))
        }
    }
}

// ---------------------------------------------------------------------------
// EtaResult convenience
// ---------------------------------------------------------------------------

impl EtaResult {
    fn is_equal(&self) -> bool {
        *self == EtaResult::Equal
    }
}

// ---------------------------------------------------------------------------
// Top-level helpers
// ---------------------------------------------------------------------------

pub fn infer_closed(t: &Term) -> Result<Term, TypeError> {
    infer(&Vec::new(), t)
}

pub fn check_closed(t: &Term, ty: &Term) -> Result<(), TypeError> {
    check(&Vec::new(), t, ty)
}

pub fn report_infer(label: &str, t: &Term) {
    match infer_closed(t) {
        Ok(ty)  => println!("  ✓  {}\n       : {}", label, ty),
        Err(e)  => println!("  ✗  {}\n{}", label, e),
    }
}

pub fn report_check(label: &str, t: &Term, ty: &Term) {
    match check_closed(t, ty) {
        Ok(()) => println!(
            "  ✓  {}\n       ⊢ {}\n       : {}",
            label, t, ty
        ),
        Err(e) => println!("  ✗  {}\n{}", label, e),
    }
}