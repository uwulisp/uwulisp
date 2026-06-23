// Cubical TypeChecker — Rust port of typechecker.hs
//
// Depends on:
//   crate::interval::{I, DNF, Literal}
//   crate::syntax::{Term, Name, Level, shift, subst, beta, show_term}
//   crate::eval::{is_top_dnf, is_bot_dnf}
//   crate::equality::{definitionally_equal_ctx, definitionally_equal_ctx_r, EtaResult}

use std::collections::BTreeSet;
use std::fmt;

use crate::cubical::equality::{EtaResult, definitionally_equal_ctx_r};
use crate::cubical::eval::{is_bot_dnf, is_top_dnf};
use crate::cubical::interval::{DNF, I, Literal};
use crate::cubical::nbe::nbe_eval;
use crate::cubical::syntax::{Datatype, ElimCase, Level, Name, Term, beta, shift, show_term};

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
        Ok(nbe_eval(&shift(i + 1, 0, &ctx[i as usize].1)))
    }
}

/// Fallback used by `infer` on neutral-looking forms (application, fst,
/// snd, ...) whose immediate subterm isn't itself inferable — typically
/// because it's a bare, un-annotated introduction form (a `TAbs`/`PLam`
/// beta-redex or an un-annotated `TPair`). In that case `infer` on the
/// subterm alone can never succeed, but the *whole* term may still reduce
/// to something with an inferable type (e.g. `(\x. x) U0` reduces to `U0`,
/// and `fst (a, b)` reduces to `a`). We retry inference on the fully
/// reduced term, and only give up if reduction made no progress.
fn infer_via_reduction(ctx: &Ctx, t: &Term, original_err: TypeError) -> Result<Term, TypeError> {
    let reduced = nbe_eval(t);
    if reduced == *t {
        Err(original_err)
    } else {
        infer(ctx, &reduced)
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
    // Inductive types / HITs
    UnknownDatatype(Name),
    UnknownConstructor(Name, Name),
    WrongNumberOfArgs {
        con: Name,
        expected: usize,
        got: usize,
    },
    BadElimCase {
        con: Name,
        msg: String,
    },
    MissingCase(Name),
    ExpectedData(Term),
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::UnboundVariable(x) => write!(f, "  Unbound variable: '{}'", x),
            TypeError::TypeMismatch(ex, got) => write!(
                f,
                "  Type mismatch\n    expected : {}\n    got      : {}",
                ex, got
            ),
            TypeError::ExpectedPi(ty) => write!(f, "  Expected a Π-type, but found:\n    {}", ty),
            TypeError::ExpectedPath(ty) => {
                write!(f, "  Expected a Path type, but found:\n    {}", ty)
            }
            TypeError::ExpectedUniverse(ty) => {
                write!(f, "  Expected a universe U_n, but found:\n    {}", ty)
            }
            TypeError::ExpectedEquiv(ty) => {
                write!(f, "  Expected an Equiv type, but found:\n    {}", ty)
            }
            TypeError::ExpectedSigma(ty) => {
                write!(f, "  Expected a Σ-type, but found:\n    {}", ty)
            }
            TypeError::NotAnInterval(t) => write!(
                f,
                "  Expected an interval expression (𝕀), but got:\n    {}",
                t
            ),
            TypeError::CannotInfer(t) => write!(
                f,
                "  Cannot infer type of term without annotation:\n    {}\n  \
                     (Tip: use 'check' instead of 'infer', or add a type annotation)",
                t
            ),
            TypeError::EtaFuelExhausted(t1, t2) => write!(
                f,
                "  Eta-equality check ran out of fuel (terms may be equal but are too\n  \
                     deeply nested to decide automatically).\n    lhs : {}\n    rhs : {}",
                t1, t2
            ),
            TypeError::Other(msg) => write!(f, "  {}", msg),
            TypeError::UnknownDatatype(d) => write!(f, "  Unknown datatype: '{}'", d),
            TypeError::UnknownConstructor(d, c) => {
                write!(f, "  Unknown constructor '{}' for datatype '{}'", c, d)
            }
            TypeError::WrongNumberOfArgs { con, expected, got } => write!(
                f,
                "  Constructor '{}' expects {} argument(s), got {}",
                con, expected, got
            ),
            TypeError::BadElimCase { con, msg } => {
                write!(f, "  Bad eliminator case for '{}': {}", con, msg)
            }
            TypeError::MissingCase(con) => write!(
                f,
                "  Eliminator is missing a case for constructor '{}'",
                con
            ),
            TypeError::ExpectedData(ty) => {
                write!(f, "  Expected a datatype (TData), but found:\n    {}", ty)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Require helpers
// ---------------------------------------------------------------------------

pub fn require_equal(ctx: &Ctx, expected: &Term, got: &Term) -> Result<(), TypeError> {
    match definitionally_equal_ctx_r(ctx, expected, got) {
        EtaResult::Equal => Ok(()),
        EtaResult::NotEqual => Err(TypeError::TypeMismatch(nbe_eval(expected), nbe_eval(got))),
        EtaResult::Exhausted => Err(TypeError::EtaFuelExhausted(
            nbe_eval(expected),
            nbe_eval(got),
        )),
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
                show_term(&names, &nbe_eval(expected)),
                nbe_eval(expected),
                show_term(&names, &nbe_eval(got)),
                nbe_eval(got),
            )))
        }
        EtaResult::Exhausted => Err(TypeError::EtaFuelExhausted(
            nbe_eval(expected),
            nbe_eval(got),
        )),
    }
}

pub fn require_universe(ctx: &Ctx, t: &Term) -> Result<Level, TypeError> {
    let ty = infer(ctx, t)?;
    match nbe_eval(&ty) {
        Term::TUniv(n) => Ok(n),
        other => Err(TypeError::ExpectedUniverse(other)),
    }
}

fn type_level_dt(dts: &[Datatype], ctx: &Ctx, t: &Term) -> Result<Level, TypeError> {
    // Match type formers structurally first. `nbe_eval` on a Π-type that still
    // mentions outer binders can collapse free de Bruijn indices and break
    // universe-level checking for dependent arrows like `(A : U0) -> A -> A`.
    match t {
        Term::TPi(x, a, b) => {
            let i = type_level_dt(dts, ctx, a)?;
            let ctx2 = extend_ctx(x.clone(), nbe_eval(a), ctx);
            let j = type_level_dt(dts, &ctx2, b)?;
            Ok(i.max(j))
        }
        Term::TPath(a, u, v) => {
            let n = type_level_dt(dts, ctx, a)?;
            let a_ = nbe_eval(a);
            let u_ty = match &a_ {
                Term::PLam(_, body) => nbe_eval(&beta(body, &Term::TInterval(I::I0))),
                p => p.clone(),
            };
            let v_ty = match &a_ {
                Term::PLam(_, body) => nbe_eval(&beta(body, &Term::TInterval(I::I1))),
                p => p.clone(),
            };
            check_dt(dts, ctx, u, &u_ty)?;
            check_dt(dts, ctx, v, &v_ty)?;
            Ok(n)
        }
        Term::TEquiv(a, b) => {
            let n = type_level_dt(dts, ctx, a)?;
            let m = type_level_dt(dts, ctx, b)?;
            Ok(n.max(m))
        }
        Term::TSigma(x, a, b) => {
            let i = type_level_dt(dts, ctx, a)?;
            let ctx2 = extend_ctx(x.clone(), nbe_eval(a), ctx);
            let j = type_level_dt(dts, &ctx2, b)?;
            Ok(i.max(j))
        }
        _ => match nbe_eval(t) {
            Term::TUniv(n) => Ok(n),
            Term::TData(_) => Ok(0),
            Term::TIntervalTy => Ok(0),
            _ => {
                let ty = infer_dt(dts, ctx, t)?;
                match nbe_eval(&ty) {
                    Term::TUniv(n) => Ok(n),
                    other => Err(TypeError::ExpectedUniverse(other)),
                }
            }
        },
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
    match nbe_eval(&ty) {
        Term::TEquiv(a, b) => Ok((nbe_eval(&a), nbe_eval(&b))),
        other => Err(TypeError::ExpectedEquiv(other)),
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
        Literal::Pos(k) => (*k, I::I1),
        Literal::NegVar(k) => (*k, I::I0),
    };

    fn go_i(i: &I, n: i32, val: &I) -> I {
        match i {
            I::IVar(k) if *k == n => val.clone(),
            I::Meet(a, b) => I::Meet(Box::new(go_i(a, n, val)), Box::new(go_i(b, n, val))),
            I::Join(a, b) => I::Join(Box::new(go_i(a, n, val)), Box::new(go_i(b, n, val))),
            I::Neg(a) => I::Neg(Box::new(go_i(a, n, val))),
            other => other.clone(),
        }
    }

    fn go(t: &Term, n: i32, val: &I) -> Term {
        match t {
            Term::TInterval(i) => nbe_eval(&Term::TInterval(go_i(i, n, val))),

            Term::TCube(DNF { cubes }) => {
                // Substitute the literal into each cube then re-normalise.
                let subst_lit = |l: &Literal| -> I {
                    match l {
                        Literal::Pos(k) => go_i(&I::IVar(*k), n, val),
                        Literal::NegVar(k) => I::Neg(Box::new(go_i(&I::IVar(*k), n, val))),
                    }
                };
                let subst_cube = |c: &BTreeSet<Literal>| -> I {
                    c.iter().fold(I::I1, |acc, l| {
                        I::Meet(Box::new(subst_lit(l)), Box::new(acc))
                    })
                };
                let combined = cubes.iter().fold(I::I0, |acc, c| {
                    I::Join(Box::new(subst_cube(c)), Box::new(acc))
                });
                nbe_eval(&Term::TInterval(combined))
            }

            Term::TApp(f, a) => nbe_eval(&Term::TApp(
                Box::new(go(f, n, val)),
                Box::new(go(a, n, val)),
            )),
            Term::TAbs(x, b) => Term::TAbs(x.clone(), Box::new(go(b, n, val))),
            Term::TPi(x, a, b) => {
                Term::TPi(x.clone(), Box::new(go(a, n, val)), Box::new(go(b, n, val)))
            }
            Term::TPath(a, u, v) => Term::TPath(
                Box::new(go(a, n, val)),
                Box::new(go(u, n, val)),
                Box::new(go(v, n, val)),
            ),
            Term::PLam(i, b) => Term::PLam(i.clone(), Box::new(go(b, n, val))),
            Term::PApp(p, r) => nbe_eval(&Term::PApp(
                Box::new(go(p, n, val)),
                Box::new(go(r, n, val)),
            )),
            Term::THComp(a, ph, u, u0) => nbe_eval(&Term::THComp(
                Box::new(go(a, n, val)),
                Box::new(go(ph, n, val)),
                Box::new(go(u, n, val)),
                Box::new(go(u0, n, val)),
            )),
            Term::TEquiv(a, b) => Term::TEquiv(Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TMkEquiv(a, b, f, g, eta, eps) => Term::TMkEquiv(
                Box::new(go(a, n, val)),
                Box::new(go(b, n, val)),
                Box::new(go(f, n, val)),
                Box::new(go(g, n, val)),
                Box::new(go(eta, n, val)),
                Box::new(go(eps, n, val)),
            ),
            Term::TEquivFwd(e, x) => nbe_eval(&Term::TEquivFwd(
                Box::new(go(e, n, val)),
                Box::new(go(x, n, val)),
            )),
            Term::TUa(e) => Term::TUa(Box::new(go(e, n, val))),
            Term::TTransport(p, x) => nbe_eval(&Term::TTransport(
                Box::new(go(p, n, val)),
                Box::new(go(x, n, val)),
            )),
            Term::TGlue(a, ph, te) => nbe_eval(&Term::TGlue(
                Box::new(go(a, n, val)),
                Box::new(go(ph, n, val)),
                Box::new(go(te, n, val)),
            )),
            Term::TGlueElem(ph, x, a) => nbe_eval(&Term::TGlueElem(
                Box::new(go(ph, n, val)),
                Box::new(go(x, n, val)),
                Box::new(go(a, n, val)),
            )),
            Term::TUnglue(ph, te, g) => nbe_eval(&Term::TUnglue(
                Box::new(go(ph, n, val)),
                Box::new(go(te, n, val)),
                Box::new(go(g, n, val)),
            )),
            Term::TSigma(x, a, b) => {
                Term::TSigma(x.clone(), Box::new(go(a, n, val)), Box::new(go(b, n, val)))
            }
            Term::TPair(a, b) => Term::TPair(Box::new(go(a, n, val)), Box::new(go(b, n, val))),
            Term::TFst(p) => nbe_eval(&Term::TFst(Box::new(go(p, n, val)))),
            Term::TSnd(p) => nbe_eval(&Term::TSnd(Box::new(go(p, n, val)))),
            // Inductive types / HITs: recurse into all sub-terms.
            // TData has no interval variables.
            Term::TData(_) => t.clone(),
            Term::TCon(data, con, args) => nbe_eval(&Term::TCon(
                data.clone(),
                con.clone(),
                args.iter().map(|a| go(a, n, val)).collect(),
            )),
            Term::TPCon(data, con, args, r) => nbe_eval(&Term::TPCon(
                data.clone(),
                con.clone(),
                args.iter().map(|a| go(a, n, val)).collect(),
                Box::new(go(r, n, val)),
            )),
            Term::TElim(motive, cases, scrut) => nbe_eval(&Term::TElim(
                Box::new(go(motive, n, val)),
                cases
                    .iter()
                    .map(|c| ElimCase {
                        con: c.con.clone(),
                        binders: c.binders.clone(),
                        body: Box::new(go(&c.body, n, val)),
                    })
                    .collect(),
                Box::new(go(scrut, n, val)),
            )),
            // TVar, TUniv, TIntervalTy: no interval vars
            other => other.clone(),
        }
    }

    go(t, n, &val)
}

/// Check that `tube_at0 ≡ base` on every face of `phi`'s DNF.
fn check_faces(ctx: &Ctx, phi: &Term, tube_at0: &Term, base: &Term) -> Result<(), TypeError> {
    match phi {
        Term::TCube(DNF { cubes }) => {
            for cube in cubes {
                // Apply all literals in the cube as substitutions.
                let apply_all = |t: &Term| -> Term {
                    cube.iter()
                        .fold(t.clone(), |acc, lit| apply_literal(lit, &acc))
                };
                let lhs = nbe_eval(&apply_all(tube_at0));
                let rhs = nbe_eval(&apply_all(base));
                require_equal_endpt(ctx, &lhs, &rhs)?;
            }
            Ok(())
        }
        // Non-DNF phi: fall back to a direct equality check.
        _ => require_equal_endpt(ctx, tube_at0, base),
    }
}

fn instantiate_telescope(args: &[Term], body: &Term) -> Term {
    args.iter()
        .rev()
        .fold(body.clone(), |acc, arg| beta(&acc, arg))
}

fn shift_cases(cases: &[ElimCase], d: i32) -> Vec<ElimCase> {
    cases
        .iter()
        .map(|case| ElimCase {
            con: case.con.clone(),
            binders: case.binders.clone(),
            body: Box::new(shift(d, case.binders.len() as i32, &case.body)),
        })
        .collect()
}

fn eval_elim_face(
    motive: &Term,
    cases: &[ElimCase],
    face: &Term,
    ord_vars: &[Term],
    ambient_depth: i32,
) -> Term {
    let face_scrut = instantiate_telescope(ord_vars, face);
    nbe_eval(&Term::TElim(
        Box::new(shift(ambient_depth, 0, motive)),
        shift_cases(cases, ambient_depth),
        Box::new(nbe_eval(&face_scrut)),
    ))
}

// ---------------------------------------------------------------------------
// Type Inference
// ---------------------------------------------------------------------------

pub fn infer(ctx: &Ctx, t: &Term) -> Result<Term, TypeError> {
    infer_dt(&[], ctx, t)
}

/// Like `infer` but with access to declared datatypes for checking
/// `TData`/`TCon`/`TPCon`/`TElim`.  Pass `&[]` when no datatypes are in scope.
pub fn infer_dt(dts: &[Datatype], ctx: &Ctx, t: &Term) -> Result<Term, TypeError> {
    match t {
        // Variable
        Term::TVar(i) => lookup_ctx(*i, ctx),

        // Universe: U_n : U_{n+1}
        Term::TUniv(n) => Ok(Term::TUniv(n + 1)),

        // Application: f a  where  f : Π(x:A).B
        Term::TApp(f, a) => match infer_dt(dts, ctx, f) {
            Ok(f_ty) => {
                let (a_ty, b_ty) = match &f_ty {
                    Term::TPi(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                    _ => match nbe_eval(&f_ty) {
                        Term::TPi(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                        other => return Err(TypeError::ExpectedPi(other)),
                    },
                };
                check_dt(dts, ctx, a, &a_ty)?;
                Ok(nbe_eval(&beta(&b_ty, a)))
            }
            Err(e) => infer_via_reduction(ctx, t, e),
        },

        // Pi formation: Π(x:A).B : U(max i j)
        Term::TPi(x, a_ty, b_ty) => {
            let i = type_level_dt(dts, ctx, a_ty)?;
            let ctx2 = extend_ctx(x.clone(), nbe_eval(a_ty), ctx);
            let j = type_level_dt(dts, &ctx2, b_ty)?;
            Ok(Term::TUniv(i.max(j)))
        }

        // Path type: Path A u v : U n
        Term::TPath(a_ty, u, v) => {
            let n = type_level_dt(dts, ctx, a_ty)?;
            let a_ty_ = nbe_eval(a_ty);
            let u_ty = match &a_ty_ {
                Term::PLam(_, body) => nbe_eval(&beta(body, &Term::TInterval(I::I0))),
                p => p.clone(),
            };
            let v_ty = match &a_ty_ {
                Term::PLam(_, body) => nbe_eval(&beta(body, &Term::TInterval(I::I1))),
                p => p.clone(),
            };
            check(ctx, u, &u_ty)?;
            check(ctx, v, &v_ty)?;
            Ok(Term::TUniv(n))
        }

        // Path application: p @ r
        Term::PApp(p, r) => match infer(ctx, p) {
            Ok(p_ty) => match nbe_eval(&p_ty) {
                Term::TPath(a_ty, _, _) => {
                    check_interval(ctx, r)?;
                    let r_ = nbe_eval(r);
                    Ok(match nbe_eval(&a_ty) {
                        Term::PLam(_, body) => nbe_eval(&beta(&body, &r_)),
                        plain => plain,
                    })
                }
                other => Err(TypeError::ExpectedPath(other)),
            },
            Err(e) => infer_via_reduction(ctx, t, e),
        },

        // Interval atoms
        Term::TInterval(_) | Term::TCube(_) => Ok(interval_ty()),
        Term::TIntervalTy => Ok(Term::TUniv(0)),

        // Lambdas cannot be inferred
        t @ Term::TAbs(_, _) | t @ Term::PLam(_, _) => Err(TypeError::CannotInfer(t.clone())),

        // Equiv type
        Term::TEquiv(a, b) => {
            let n = type_level_dt(dts, ctx, a)?;
            let m = type_level_dt(dts, ctx, b)?;
            Ok(Term::TUniv(n.max(m)))
        }

        // mkEquiv: build an equivalence record
        Term::TMkEquiv(a, b, f, g, eta, eps) => {
            type_level_dt(dts, ctx, a)?;
            type_level_dt(dts, ctx, b)?;
            let a_ = nbe_eval(a);
            let b_ = nbe_eval(b);
            // f : A → B
            check(
                ctx,
                f,
                &Term::TPi("_".into(), Box::new(a_.clone()), Box::new(shift(1, 0, &b_))),
            )?;
            // g : B → A
            check(
                ctx,
                g,
                &Term::TPi("_".into(), Box::new(b_.clone()), Box::new(shift(1, 0, &a_))),
            )?;
            // eta : (a : A) → Path A a (g (f a))
            check(
                ctx,
                eta,
                &Term::TPi(
                    "a".into(),
                    Box::new(a_.clone()),
                    Box::new(Term::TPath(
                        Box::new(shift(1, 0, &a_)),
                        Box::new(Term::TVar(0)),
                        Box::new(Term::TApp(
                            Box::new(shift(1, 0, g)),
                            Box::new(Term::TApp(
                                Box::new(shift(1, 0, f)),
                                Box::new(Term::TVar(0)),
                            )),
                        )),
                    )),
                ),
            )?;
            // eps : (b : B) → Path B (f (g b)) b
            check(
                ctx,
                eps,
                &Term::TPi(
                    "b".into(),
                    Box::new(b_.clone()),
                    Box::new(Term::TPath(
                        Box::new(shift(1, 0, &b_)),
                        Box::new(Term::TApp(
                            Box::new(shift(1, 0, f)),
                            Box::new(Term::TApp(
                                Box::new(shift(1, 0, g)),
                                Box::new(Term::TVar(0)),
                            )),
                        )),
                        Box::new(Term::TVar(0)),
                    )),
                ),
            )?;
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
            let n = type_level_dt(dts, ctx, &a)?;
            Ok(Term::TPath(
                Box::new(Term::TUniv(n)),
                Box::new(a),
                Box::new(b),
            ))
        }

        // transport p x : B   where  p : Path U A B,  x : A
        Term::TTransport(p, x) => {
            let p_ty = match p.as_ref() {
                // `p` is a literal path-lambda (an introduction form, not a
                // path-typed neutral) — `infer(p)` can never succeed on a
                // bare PLam, so derive its TPath type directly from the
                // body instead, the same way `infer` already does for
                // TAbs-applied-to-argument in TApp.
                Term::PLam(i, body) => {
                    let ctx2 = extend_ctx(i.clone(), interval_ty(), ctx);
                    let a_ty = infer(&ctx2, body)?;
                    let u = nbe_eval(&beta(body, &Term::TInterval(I::I0)));
                    let v = nbe_eval(&beta(body, &Term::TInterval(I::I1)));
                    Term::TPath(Box::new(a_ty), Box::new(u), Box::new(v))
                }
                _ => infer(ctx, p)?,
            };
            match nbe_eval(&p_ty) {
                Term::TPath(a_ty, _, _) => {
                    let (x_ty, ret_ty) = match nbe_eval(&a_ty) {
                        Term::PLam(_, body) => (
                            nbe_eval(&beta(&body, &Term::TInterval(I::I0))),
                            nbe_eval(&beta(&body, &Term::TInterval(I::I1))),
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
            let n = type_level_dt(dts, ctx, a_ty)?;
            let a_ty_ = nbe_eval(a_ty);
            check_interval(ctx, phi)?;
            let te_ty = infer(ctx, te)?;
            let m = match nbe_eval(&te_ty) {
                Term::TUniv(k) => k,
                Term::TEquiv(a, b) => {
                    let a_ = nbe_eval(&a);
                    let b_ = nbe_eval(&b);
                    require_equal(ctx, &b_, &a_ty_)?;
                    let p = type_level_dt(dts, ctx, &a_)?;
                    let q = type_level_dt(dts, ctx, &b_)?;
                    p.max(q)
                }
                Term::TMkEquiv(a, b, _, _, _, _) => {
                    let a_ = nbe_eval(&a);
                    let b_ = nbe_eval(&b);
                    require_equal(ctx, &b_, &a_ty_)?;
                    let p = type_level_dt(dts, ctx, &a_)?;
                    let q = type_level_dt(dts, ctx, &b_)?;
                    p.max(q)
                }
                other => {
                    return Err(TypeError::Other(format!(
                        "Glue: equivalence argument has unexpected type: {}",
                        other
                    )));
                }
            };
            Ok(Term::TUniv(n.max(m)))
        }

        // unglue phi te g
        Term::TUnglue(phi, te, g) => {
            check_interval(ctx, phi)?;
            let phi_ = nbe_eval(phi);
            if is_top_dnf(&phi_) {
                infer(ctx, &Term::TEquivFwd(te.clone(), g.clone()))
            } else if is_bot_dnf(&phi_) {
                infer(ctx, g)
            } else {
                let g_ty = infer(ctx, g)?;
                match nbe_eval(&g_ty) {
                    Term::TGlue(a_ty, _, _) => Ok(nbe_eval(&a_ty)),
                    other => Err(TypeError::Other(format!(
                        "unglue: expected argument of Glue type, got: {}",
                        other
                    ))),
                }
            }
        }

        // glue elem — can only infer in degenerate phi cases
        t @ Term::TGlueElem(phi, elm, a) => {
            let phi_ = nbe_eval(phi);
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
            let i = type_level_dt(dts, ctx, a_ty)?;
            let ctx2 = extend_ctx(x.clone(), nbe_eval(a_ty), ctx);
            let j = type_level_dt(dts, &ctx2, b_ty)?;
            Ok(Term::TUniv(i.max(j)))
        }

        // fst p : A   where  p : Σ(x:A).B
        Term::TFst(p) => match infer(ctx, p) {
            Ok(p_ty) => match nbe_eval(&p_ty) {
                Term::TSigma(_, a_ty, _) => Ok(nbe_eval(&a_ty)),
                other => Err(TypeError::ExpectedSigma(other)),
            },
            Err(e) => infer_via_reduction(ctx, t, e),
        },

        // snd p : B[fst p / x]   where  p : Σ(x:A).B
        Term::TSnd(p) => match infer(ctx, p) {
            Ok(p_ty) => match nbe_eval(&p_ty) {
                Term::TSigma(_, _, b_ty) => Ok(nbe_eval(&beta(&b_ty, &Term::TFst(p.clone())))),
                other => Err(TypeError::ExpectedSigma(other)),
            },
            Err(e) => infer_via_reduction(ctx, t, e),
        },

        // Pairs cannot be inferred without annotation
        t @ Term::TPair(_, _) => Err(TypeError::CannotInfer(t.clone())),

        // hcomp A phi tube base
        Term::THComp(a_ty, phi, tube, base) => {
            type_level_dt(dts, ctx, a_ty)?;
            let a_ty_ = nbe_eval(a_ty);
            check_interval(ctx, phi)?;
            check(ctx, base, &a_ty_)?;

            let phi_ = nbe_eval(phi);
            match nbe_eval(tube) {
                Term::PLam(i, body) => {
                    // (a) body : A in extended context
                    let ctx2 = extend_ctx(i.clone(), interval_ty(), ctx);
                    let a_ty_s = shift(1, 0, &a_ty_);
                    check(&ctx2, &body, &a_ty_s)?;
                    // (b) tube@0 ≡ base on each face of phi
                    let tube_at0 = nbe_eval(&beta(&body, &Term::TInterval(I::I0)));
                    check_faces(ctx, &phi_, &tube_at0, &nbe_eval(base))?;
                }
                tube_ => {
                    // Non-lambda tube: treat as Path A u v
                    let tube_ty = infer(ctx, &tube_)?;
                    match nbe_eval(&tube_ty) {
                        Term::TPath(a, u, v) => {
                            if !definitionally_equal_ctx_r(ctx, &nbe_eval(&a), &a_ty_).is_equal() {
                                return Err(TypeError::TypeMismatch(
                                    nbe_eval(&a_ty_),
                                    nbe_eval(&a),
                                ));
                            }
                            check(ctx, &nbe_eval(&u), &a_ty_)?;
                            check(ctx, &nbe_eval(&v), &a_ty_)?;
                            check_faces(ctx, &phi_, &nbe_eval(&u), &nbe_eval(base))?;
                        }
                        other => return Err(TypeError::ExpectedPath(other)),
                    }
                }
            }
            Ok(a_ty_)
        }

        // ------------------------------------------------------------------
        // Inductive types / HITs
        // ------------------------------------------------------------------

        // TData(d) : U_k  where k is the maximum universe level required by
        // any constructor argument type. We compute this by checking each
        // arg type in a scope containing all prior args of that telescope.
        // Datatypes with no constructors and no args default to U_0.
        Term::TData(d) => {
            let dt = dts
                .iter()
                .find(|dt| &dt.name == d)
                .ok_or_else(|| TypeError::UnknownDatatype(d.clone()))?;

            let mut max_level: Level = 0;

            // Ordinary constructors
            for con_sig in &dt.cons {
                let mut tel_ctx = ctx.clone();
                let mut prev_args: Vec<Term> = Vec::new();
                for (k, arg_ty) in con_sig.arg_tys.iter().enumerate() {
                    let arg_ty_inst = prev_args
                        .iter()
                        .rev()
                        .fold(arg_ty.clone(), |ty, a| beta(&ty, a));
                    let lvl = type_level_dt(dts, &tel_ctx, &arg_ty_inst)?;
                    max_level = max_level.max(lvl);
                    // Push a fresh variable for this arg into the context.
                    let var_name = format!("_con_arg_{}", k);
                    let depth = k as i32;
                    prev_args.push(shift(depth + 1, 0, &Term::TVar(0)));
                    tel_ctx = extend_ctx(var_name, nbe_eval(&arg_ty_inst), &tel_ctx);
                }
            }

            // Path constructors (ordinary args only; interval arg is in 𝕀 ⊂ U_0)
            for pcon_sig in &dt.pcons {
                let mut tel_ctx = ctx.clone();
                let mut prev_args: Vec<Term> = Vec::new();
                for (k, arg_ty) in pcon_sig.arg_tys.iter().enumerate() {
                    let arg_ty_inst = prev_args
                        .iter()
                        .rev()
                        .fold(arg_ty.clone(), |ty, a| beta(&ty, a));
                    let lvl = type_level_dt(dts, &tel_ctx, &arg_ty_inst)?;
                    max_level = max_level.max(lvl);
                    let var_name = format!("_pcon_arg_{}", k);
                    let depth = k as i32;
                    prev_args.push(shift(depth + 1, 0, &Term::TVar(0)));
                    tel_ctx = extend_ctx(var_name, nbe_eval(&arg_ty_inst), &tel_ctx);
                }
            }

            Ok(Term::TUniv(max_level))
        }

        // TCon(d, c, args) : TData(d)
        // Check each arg against the constructor's declared argument types,
        // substituting earlier args into later (dependent) argument types.
        Term::TCon(d, c, args) => {
            let dt = dts
                .iter()
                .find(|dt| &dt.name == d)
                .ok_or_else(|| TypeError::UnknownDatatype(d.clone()))?;
            let sig = dt
                .find_con(c)
                .ok_or_else(|| TypeError::UnknownConstructor(d.clone(), c.clone()))?;
            if args.len() != sig.arity() {
                return Err(TypeError::WrongNumberOfArgs {
                    con: c.clone(),
                    expected: sig.arity(),
                    got: args.len(),
                });
            }
            // Check each argument against its declared type.
            // arg_tys is a telescope: arg_tys[k] lives in a scope with
            // args[0..k-1] bound (outermost-first, indices 0..k-1 from
            // outermost). We substitute earlier checked args in as we go.
            let mut checked_args: Vec<Term> = Vec::with_capacity(args.len());
            for (k, arg) in args.iter().enumerate() {
                // arg_ty[k] has free indices 0..k-1 (de Bruijn, innermost =
                // latest previous arg). Substitute them from innermost out.
                let arg_ty = checked_args
                    .iter()
                    .rev()
                    .fold(sig.arg_tys[k].clone(), |ty, prev| beta(&ty, prev));
                check_dt(dts, ctx, arg, &nbe_eval(&arg_ty))?;
                checked_args.push(nbe_eval(arg));
            }
            Ok(Term::TData(d.clone()))
        }

        // TPCon(d, pc, args, r) : Path (TData(d)) face0[args] face1[args]
        Term::TPCon(d, pc, args, r) => {
            let dt = dts
                .iter()
                .find(|dt| &dt.name == d)
                .ok_or_else(|| TypeError::UnknownDatatype(d.clone()))?;
            let sig = dt
                .find_pcon(pc)
                .ok_or_else(|| TypeError::UnknownConstructor(d.clone(), pc.clone()))?;
            if args.len() != sig.arity() {
                return Err(TypeError::WrongNumberOfArgs {
                    con: pc.clone(),
                    expected: sig.arity(),
                    got: args.len(),
                });
            }
            // Check ordinary args against telescope, same as TCon.
            let mut checked_args: Vec<Term> = Vec::with_capacity(args.len());
            for (k, arg) in args.iter().enumerate() {
                let arg_ty = checked_args
                    .iter()
                    .rev()
                    .fold(sig.arg_tys[k].clone(), |ty, prev| beta(&ty, prev));
                check_dt(dts, ctx, arg, &nbe_eval(&arg_ty))?;
                checked_args.push(nbe_eval(arg));
            }
            // Check interval argument.
            check_interval(ctx, r)?;
            // Compute boundary terms by substituting checked_args into
            // face0/face1 (which live in a scope of arity ordinary args).
            let face0 = checked_args
                .iter()
                .rev()
                .fold(sig.face0.clone(), |ty, a| beta(&ty, a));
            let face1 = checked_args
                .iter()
                .rev()
                .fold(sig.face1.clone(), |ty, a| beta(&ty, a));
            // The path family must be a PLam over the interval variable so that
            // the type is well-formed as Path (λ_. TData(d)) face0 face1.
            // For non-parameterized HITs TData(d) is constant, but using PLam
            // keeps the form correct and would generalise to parameterized types.
            Ok(Term::TPath(
                Box::new(Term::PLam(
                    "_".into(),
                    Box::new(Term::TData(d.clone())),
                )),
                Box::new(nbe_eval(&face0)),
                Box::new(nbe_eval(&face1)),
            ))
        }

        // TElim(motive, cases, scrut)
        //
        // motive : TData(d) → U_n
        // scrut  : TData(d)
        // For each constructor  c  with args A₀…Aₖ:
        //   case body : motive (TCon(d, c, args))
        //   (under binders for the constructor args in context)
        // For each path constructor  pc  with args A₀…Aₖ  and boundary  f0/f1:
        //   case body : Path (motive ∘ pcon) (case_for_f0) (case_for_f1)
        //   body is PLam-shaped (see ElimCase docs in syntax.rs)
        // Returns: motive scrut
        Term::TElim(motive, cases, scrut) => {
            // Infer scrutinee — must be TData(d).
            let scrut_ty = infer_dt(dts, ctx, scrut)?;
            let d = match nbe_eval(&scrut_ty) {
                Term::TData(d) => d,
                other => return Err(TypeError::ExpectedData(other)),
            };
            let dt = dts
                .iter()
                .find(|dt| dt.name == d)
                .ok_or_else(|| TypeError::UnknownDatatype(d.clone()))?;

            // Verify motive has type Π(_:TData(d)).C where C is a well-formed type.
            match motive.as_ref() {
                Term::TAbs(x, body) => {
                    let motive_ctx =
                        extend_ctx(x.clone(), Term::TData(d.clone()), ctx);
                    type_level_dt(dts, &motive_ctx, body)?;
                }
                _ => {
                    let motive_inferred = infer_dt(dts, ctx, motive)?;
                    match nbe_eval(&motive_inferred) {
                        Term::TPi(x, dom, cod) => {
                            require_equal(ctx, &nbe_eval(&dom), &Term::TData(d.clone()))?;
                            let cod_ctx = extend_ctx(x, nbe_eval(&dom), ctx);
                            type_level_dt(dts, &cod_ctx, &cod)?;
                        }
                        other => return Err(TypeError::ExpectedPi(other)),
                    }
                }
            }

            // Check all ordinary constructor cases.
            for con_sig in &dt.cons {
                let case = cases
                    .iter()
                    .find(|c| c.con == con_sig.name)
                    .ok_or_else(|| TypeError::MissingCase(con_sig.name.clone()))?;

                if case.binders.len() != con_sig.arity() {
                    return Err(TypeError::BadElimCase {
                        con: con_sig.name.clone(),
                        msg: format!(
                            "expected {} binders, got {}",
                            con_sig.arity(),
                            case.binders.len()
                        ),
                    });
                }

                // Build extended context: push binders outermost-first,
                // last binder ends up at index 0.
                // arg_tys[k] is in a scope with k prior args (indices 0..k-1),
                // but as we push onto ctx those already-bound args shift.
                // We build the ctx incrementally: each new binder's type is
                // evaluated in the ctx so far (with previous binders live).
                let mut case_ctx = ctx.clone();
                let mut con_args_in_ctx: Vec<Term> = Vec::new();
                for (k, binder_name) in case.binders.iter().enumerate() {
                    // arg_tys[k] mentions indices 0..k-1 in declaration scope.
                    // In case_ctx those are already bound at depth 0..k-1 from
                    // the bottom of the stack.  Substitute them: fold innermost first.
                    let arg_ty = con_args_in_ctx
                        .iter()
                        .rev()
                        .fold(con_sig.arg_tys[k].clone(), |ty, a| beta(&ty, a));
                    let arg_ty_ev = nbe_eval(&arg_ty);
                    // This arg, once in context, is TVar(0) in case_ctx after push.
                    // For the next iteration we record it as TVar(0) shifted up by
                    // the depth we've pushed so far.
                    let depth = k as i32;
                    con_args_in_ctx.push(shift(depth + 1, 0, &Term::TVar(0)));
                    case_ctx = extend_ctx(binder_name.clone(), arg_ty_ev, &case_ctx);
                }

                // Expected type: motive applied to TCon(d, c, all binders as vars).
                // The binders in case_ctx are at indices 0..arity-1 (innermost=0).
                // TCon's args are positional outermost-first, so arg[0] = TVar(arity-1), etc.
                let arity = con_sig.arity();
                let con_term_args: Vec<Term> = (0..arity)
                    .map(|k| Term::TVar((arity - 1 - k) as i32))
                    .collect();
                let scrut_as_con = Term::TCon(d.clone(), con_sig.name.clone(), con_term_args);
                let expected_ty = nbe_eval(&Term::TApp(
                    Box::new(shift(arity as i32, 0, motive)),
                    Box::new(scrut_as_con),
                ));
                check_dt(dts, &case_ctx, &case.body, &expected_ty)?;
            }

            // Check all path constructor cases.
            for pcon_sig in &dt.pcons {
                let case = cases
                    .iter()
                    .find(|c| c.con == pcon_sig.name)
                    .ok_or_else(|| TypeError::MissingCase(pcon_sig.name.clone()))?;

                // binders = arity ordinary args + 1 interval var (last).
                let expected_binders = pcon_sig.arity() + 1;
                if case.binders.len() != expected_binders {
                    return Err(TypeError::BadElimCase {
                        con: pcon_sig.name.clone(),
                        msg: format!(
                            "expected {} binders ({} ordinary + 1 interval), got {}",
                            expected_binders,
                            pcon_sig.arity(),
                            case.binders.len()
                        ),
                    });
                }

                let ord_binders = &case.binders[..pcon_sig.arity()];
                let i_name = &case.binders[pcon_sig.arity()];

                // Build context for the ordinary args (same as ordinary constructor).
                let mut case_ctx = ctx.clone();
                let mut pcon_args_in_ctx: Vec<Term> = Vec::new();
                for (k, binder_name) in ord_binders.iter().enumerate() {
                    let arg_ty = pcon_args_in_ctx
                        .iter()
                        .rev()
                        .fold(pcon_sig.arg_tys[k].clone(), |ty, a| beta(&ty, a));
                    let depth = k as i32;
                    pcon_args_in_ctx.push(shift(depth + 1, 0, &Term::TVar(0)));
                    case_ctx = extend_ctx(binder_name.clone(), nbe_eval(&arg_ty), &case_ctx);
                }

                // Extend with the interval variable (now at index 0).
                let arity = pcon_sig.arity();
                let ord_case_ctx = case_ctx.clone();
                case_ctx = extend_ctx(i_name.clone(), interval_ty(), &case_ctx);

                // The case body must have type:
                //   Path (motive (pcon args i)) face0_case face1_case
                // where:
                //   - pcon args i = TPCon(d, pc, [arg vars], TVar(0))  [i at 0]
                //   - face0_case  = case for the pcon's face0 constructor applied to elim
                //   - face1_case  = case for the pcon's face1 constructor applied to elim
                //
                // The path type A is (motive ∘ TPCon(d,pc,args,i)), so it's a PLam.
                // The endpoints are motive applied to the boundary TCon terms,
                // but more precisely: by coherence the boundaries must match what
                // the ordinary cases return when applied to the boundary args.
                // We check the body as a PLam over the interval variable and
                // verify endpoints via boundary substitution into the case body.

                // Ordinary arg vars in case_ctx (interval at 0, ord args at 1..arity).
                let ord_var: Vec<Term> = (0..arity)
                    .map(|k| Term::TVar((arity - k) as i32)) // arg[0]=TVar(arity), arg[k]=TVar(arity-k)
                    .collect();
                let ord_var_no_i: Vec<Term> = (0..arity)
                    .map(|k| Term::TVar((arity - 1 - k) as i32))
                    .collect();
                let i_var = Term::TVar(0);

                // TPCon with i as the interval arg.
                let pcon_term = Term::TPCon(
                    d.clone(),
                    pcon_sig.name.clone(),
                    ord_var.clone(),
                    Box::new(i_var.clone()),
                );

                // Motive applied to pcon — this is a PLam over i.
                // motive lives in ctx (no case binders), so shift by (arity+1).
                let motive_shifted = shift((arity + 1) as i32, 0, motive);
                let motive_at_pcon = nbe_eval(&Term::TApp(
                    Box::new(motive_shifted.clone()),
                    Box::new(pcon_term),
                ));

                // Endpoints: substitute i=I0/I1 into pcon, apply motive.
                let face0_term = nbe_eval(&Term::TPCon(
                    d.clone(),
                    pcon_sig.name.clone(),
                    ord_var.clone(),
                    Box::new(Term::TInterval(I::I0)),
                ));
                let face1_term = nbe_eval(&Term::TPCon(
                    d.clone(),
                    pcon_sig.name.clone(),
                    ord_var.clone(),
                    Box::new(Term::TInterval(I::I1)),
                ));
                let u = nbe_eval(&Term::TApp(
                    Box::new(motive_shifted.clone()),
                    Box::new(face0_term),
                ));
                let v = nbe_eval(&Term::TApp(Box::new(motive_shifted), Box::new(face1_term)));

                let expected_body_ty = Term::TPath(
                    Box::new(Term::PLam(i_name.clone(), Box::new(motive_at_pcon))),
                    Box::new(u),
                    Box::new(v),
                );
                check_dt(dts, &case_ctx, &case.body, &expected_body_ty)?;

                let body_at0 = nbe_eval(&Term::PApp(
                    case.body.clone(),
                    Box::new(Term::TInterval(I::I0)),
                ));
                let body_at1 = nbe_eval(&Term::PApp(
                    case.body.clone(),
                    Box::new(Term::TInterval(I::I1)),
                ));
                let face0_case =
                    eval_elim_face(motive, cases, &pcon_sig.face0, &ord_var_no_i, arity as i32);
                let face1_case =
                    eval_elim_face(motive, cases, &pcon_sig.face1, &ord_var_no_i, arity as i32);
                require_equal_endpt(&ord_case_ctx, &face0_case, &body_at0)?;
                require_equal_endpt(&ord_case_ctx, &face1_case, &body_at1)?;
            }

            // Result type: motive scrut
            Ok(nbe_eval(&Term::TApp(
                Box::new(motive.as_ref().clone()),
                Box::new(nbe_eval(scrut)),
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Type Checking
// ---------------------------------------------------------------------------

pub fn check(ctx: &Ctx, t: &Term, ty: &Term) -> Result<(), TypeError> {
    check_dt(&[], ctx, t, ty)
}

/// Like `check` but with access to declared datatypes.
/// Pass `&[]` when no datatypes are in scope.
pub fn check_dt(dts: &[Datatype], ctx: &Ctx, t: &Term, ty: &Term) -> Result<(), TypeError> {
    match t {
        // Lambda introduction
        Term::TAbs(x, body) => {
            let (a_ty, b_ty) = match ty {
                Term::TPi(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                _ => match nbe_eval(ty) {
                    Term::TPi(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                    other => return Err(TypeError::ExpectedPi(other)),
                },
            };
            check_dt(
                dts,
                &extend_ctx(x.clone(), nbe_eval(&a_ty), ctx),
                body,
                &b_ty,
            )
        }

        // Path-lambda introduction
        Term::PLam(i, body) => {
            let (a_ty, u, v) = match ty {
                Term::TPath(a, u, v) => (a.as_ref().clone(), u.as_ref().clone(), v.as_ref().clone()),
                _ => match nbe_eval(ty) {
                    Term::TPath(a, u, v) => {
                        (a.as_ref().clone(), u.as_ref().clone(), v.as_ref().clone())
                    }
                    other => return Err(TypeError::ExpectedPath(other)),
                },
            };
            let ctx2 = extend_ctx(i.clone(), interval_ty(), ctx);
            let body_ty = match nbe_eval(&a_ty) {
                p @ Term::PLam { .. } => p,
                plain => shift(1, 0, &plain),
            };
            let body_at0 = nbe_eval(&beta(body, &Term::TInterval(I::I0)));
            let body_at1 = nbe_eval(&beta(body, &Term::TInterval(I::I1)));
            require_equal_endpt(ctx, &nbe_eval(&u), &body_at0)?;
            require_equal_endpt(ctx, &nbe_eval(&v), &body_at1)?;
            check_dt(dts, &ctx2, body, &body_ty)
        }

        // GlueElem checking
        Term::TGlueElem(phi, t_inner, a) => match nbe_eval(ty) {
            Term::TGlue(a_ty, phi_, te) => {
                check_interval(ctx, phi)?;
                require_equal(ctx, &nbe_eval(&phi_), &nbe_eval(phi))?;
                let t_ty = match nbe_eval(&te) {
                    Term::TMkEquiv(dom_a, _, _, _, _, _) => nbe_eval(&dom_a),
                    Term::TEquiv(dom_a, _) => nbe_eval(&dom_a),
                    other => other,
                };
                check_dt(dts, ctx, t_inner, &t_ty)?;
                check_dt(dts, ctx, a, &nbe_eval(&a_ty))
            }
            other => Err(TypeError::Other(format!(
                "glue: expected Glue type, got: {}",
                other
            ))),
        },

        // Pair introduction
        Term::TPair(a, b) => {
            let (a_ty, b_ty) = match ty {
                Term::TSigma(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                _ => match nbe_eval(ty) {
                    Term::TSigma(_, a, b) => (a.as_ref().clone(), b.as_ref().clone()),
                    other => return Err(TypeError::ExpectedSigma(other)),
                },
            };
            check_dt(dts, ctx, a, &nbe_eval(&a_ty))?;
            check_dt(dts, ctx, b, &nbe_eval(&beta(&b_ty, a)))
        }

        // Constructor introduction — checked bidirectionally.
        //
        // For TCon: the expected type must be TData(d). We use it to resolve
        // the datatype so argument checking can propagate the expected type
        // into dependent telescope positions, rather than inferring and
        // comparing afterward.
        //
        // For TPCon: similarly, the expected type should be
        // Path (λ_. TData(d)) face0 face1; we extract d from it and then
        // delegate to infer_dt (which checks args and verifies the path
        // endpoints). We still call require_equal at the end to catch any
        // endpoint mismatch the caller's annotation encodes.
        Term::TCon(d, c, args) => {
            // Resolve expected datatype name from the annotation when possible,
            // falling back to the constructor's own declared datatype.
            let expected_ty_nf = nbe_eval(ty);
            let expected_d = match &expected_ty_nf {
                Term::TData(ed) => {
                    // Consistency: the annotation's datatype must match the
                    // constructor's declared datatype.
                    if ed != d {
                        return Err(TypeError::TypeMismatch(
                            expected_ty_nf.clone(),
                            Term::TData(d.clone()),
                        ));
                    }
                    ed.clone()
                }
                // If the annotation isn't a TData, let infer catch it below.
                _ => d.clone(),
            };
            let dt = dts
                .iter()
                .find(|dt| dt.name == expected_d)
                .ok_or_else(|| TypeError::UnknownDatatype(expected_d.clone()))?;
            let sig = dt
                .find_con(c)
                .ok_or_else(|| TypeError::UnknownConstructor(expected_d.clone(), c.clone()))?;
            if args.len() != sig.arity() {
                return Err(TypeError::WrongNumberOfArgs {
                    con: c.clone(),
                    expected: sig.arity(),
                    got: args.len(),
                });
            }
            // Check args against the telescope, propagating checked args into
            // later dependent positions (bidirectional mode for each arg).
            let mut checked_args: Vec<Term> = Vec::with_capacity(args.len());
            for (k, arg) in args.iter().enumerate() {
                let arg_ty = checked_args
                    .iter()
                    .rev()
                    .fold(sig.arg_tys[k].clone(), |ty, prev| beta(&ty, prev));
                check_dt(dts, ctx, arg, &nbe_eval(&arg_ty))?;
                checked_args.push(nbe_eval(arg));
            }
            // Verify the overall expected type equals TData(d).
            require_equal(ctx, &expected_ty_nf, &Term::TData(d.clone()))
        }

        Term::TPCon(d, pc, args, r) => {
            // Infer the full path type from the constructor signature, then
            // unify with the expected type so endpoint annotations are checked.
            let inferred = infer_dt(dts, ctx, &Term::TPCon(d.clone(), pc.clone(), args.clone(), r.clone()))?;
            require_equal(ctx, &nbe_eval(ty), &nbe_eval(&inferred))
        }

        // Fall through to inference.
        t => match infer_dt(dts, ctx, t) {
            Ok(ty_) => require_equal(ctx, &nbe_eval(ty), &nbe_eval(&ty_)),
            Err(e) => {
                let reduced = nbe_eval(t);
                if reduced == *t {
                    Err(e)
                } else {
                    check_dt(dts, ctx, &reduced, ty)
                }
            }
        },
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

pub fn infer_closed_dt(dts: &[Datatype], t: &Term) -> Result<Term, TypeError> {
    infer_dt(dts, &Vec::new(), t)
}

pub fn check_closed_dt(dts: &[Datatype], t: &Term, ty: &Term) -> Result<(), TypeError> {
    check_dt(dts, &Vec::new(), t, ty)
}

pub fn report_infer(label: &str, t: &Term) {
    match infer_closed(t) {
        Ok(ty) => println!("  ✓  {}\n       : {}", label, ty),
        Err(e) => println!("  ✗  {}\n{}", label, e),
    }
}

pub fn report_check(label: &str, t: &Term, ty: &Term) {
    match check_closed(t, ty) {
        Ok(()) => println!("  ✓  {}\n       ⊢ {}\n       : {}", label, t, ty),
        Err(e) => println!("  ✗  {}\n{}", label, e),
    }
}