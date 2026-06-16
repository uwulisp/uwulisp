// Cubical Syntax — Rust port of syntax.hs
//
// Depends on types from interval.rs:
//   use crate::interval::{I, DNF};

use crate::cubical::interval::{I, DNF};
use std::fmt;

pub type Name  = String;
pub type Level = i32;

// ---------------------------------------------------------------------------
// Term Syntax
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    TVar(i32),
    TApp(Box<Term>, Box<Term>),
    TAbs(Name, Box<Term>),
    TUniv(Level),
    TIntervalTy,
    TPi(Name, Box<Term>, Box<Term>),
    TInterval(I),
    TCube(DNF),
    TPath(Box<Term>, Box<Term>, Box<Term>),
    PLam(Name, Box<Term>),
    PApp(Box<Term>, Box<Term>),
    THComp(Box<Term>, Box<Term>, Box<Term>, Box<Term>),
    TEquiv(Box<Term>, Box<Term>),
    TMkEquiv(Box<Term>, Box<Term>, Box<Term>, Box<Term>, Box<Term>, Box<Term>),
    TEquivFwd(Box<Term>, Box<Term>),
    TUa(Box<Term>),
    TTransport(Box<Term>, Box<Term>),
    TGlue(Box<Term>, Box<Term>, Box<Term>),
    TGlueElem(Box<Term>, Box<Term>, Box<Term>),
    TUnglue(Box<Term>, Box<Term>, Box<Term>),
    TSigma(Name, Box<Term>, Box<Term>),
    TPair(Box<Term>, Box<Term>),
    TFst(Box<Term>),
    TSnd(Box<Term>),
}

// ---------------------------------------------------------------------------
// Pretty-printing
// ---------------------------------------------------------------------------

pub fn show_term(env: &[Name], t: &Term) -> String {
    match t {
        Term::TVar(i) => {
            let i = *i as usize;
            if i < env.len() {
                env[i].clone()
            } else {
                format!("#{}", i)
            }
        }
        Term::TApp(f, a) =>
            format!("({} {})", show_term(env, f), show_term(env, a)),
        Term::TAbs(x, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!("λ{}. {}", x, show_term(&env2, b))
        }
        Term::TUniv(n) => format!("U{}", n),
        Term::TIntervalTy => "𝕀".to_string(),
        Term::TPi(x, a, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!("Π({}:{}). {}", x, show_term(env, a), show_term(&env2, b))
        }
        Term::TInterval(i) => format!("{}", i),
        Term::TCube(c) => format!("{}", c),
        Term::TPath(a, u, v) =>
            format!("Path {} {} {}", show_term(env, a), show_term(env, u), show_term(env, v)),
        Term::PLam(i, b) => {
            let mut env2 = vec![i.clone()];
            env2.extend_from_slice(env);
            format!("⟨{}⟩ {}", i, show_term(&env2, b))
        }
        Term::PApp(p, r) =>
            format!("{} @ {}", show_term(env, p), show_term(env, r)),
        Term::THComp(a, phi, u, u0) =>
            format!(
                "hcomp {} [{}] ({}) {}",
                show_term(env, a), show_term(env, phi),
                show_term(env, u), show_term(env, u0)
            ),
        Term::TEquiv(a, b) =>
            format!("Equiv {} {}", show_term(env, a), show_term(env, b)),
        Term::TMkEquiv(a, b, f, g, eta, eps) =>
            format!(
                "mkEquiv {} {} {} {} {} {}",
                show_term(env, a), show_term(env, b),
                show_term(env, f), show_term(env, g),
                show_term(env, eta), show_term(env, eps)
            ),
        Term::TEquivFwd(e, x) =>
            format!("equivFwd ({}) {}", show_term(env, e), show_term(env, x)),
        Term::TUa(e) =>
            format!("ua ({})", show_term(env, e)),
        Term::TTransport(p, x) =>
            format!("transport ({}) {}", show_term(env, p), show_term(env, x)),
        Term::TGlue(a, phi, te) =>
            format!(
                "Glue {} [{}] ({})",
                show_term(env, a), show_term(env, phi), show_term(env, te)
            ),
        Term::TGlueElem(phi, t, a) =>
            format!(
                "glue [{}] ({}) {}",
                show_term(env, phi), show_term(env, t), show_term(env, a)
            ),
        Term::TUnglue(phi, te, g) =>
            format!(
                "unglue [{}] ({}) {}",
                show_term(env, phi), show_term(env, te), show_term(env, g)
            ),
        Term::TSigma(x, a, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!("Σ({}:{}). {}", x, show_term(env, a), show_term(&env2, b))
        }
        Term::TPair(a, b) =>
            format!("({} , {})", show_term(env, a), show_term(env, b)),
        Term::TFst(p) =>
            format!("fst {}", show_term(env, p)),
        Term::TSnd(p) =>
            format!("snd {}", show_term(env, p)),
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", show_term(&[], self))
    }
}

// ---------------------------------------------------------------------------
// Shift
// ---------------------------------------------------------------------------

/// Increment all free de Bruijn indices >= `c` by `d`.
pub fn shift(d: i32, c: i32, term: &Term) -> Term {
    match term {
        Term::TVar(i) =>
            Term::TVar(if *i >= c { i + d } else { *i }),
        Term::TApp(f, a) =>
            Term::TApp(b(shift(d, c, f)), b(shift(d, c, a))),
        Term::TAbs(x, body) =>
            Term::TAbs(x.clone(), b(shift(d, c + 1, body))),
        Term::TPi(x, a, body) =>
            Term::TPi(x.clone(), b(shift(d, c, a)), b(shift(d, c + 1, body))),
        Term::TUniv(n) => Term::TUniv(*n),
        Term::TIntervalTy => Term::TIntervalTy,
        Term::TInterval(i) => Term::TInterval(i.clone()),
        Term::TCube(cu) => Term::TCube(cu.clone()),
        Term::TPath(a, u, v) =>
            Term::TPath(b(shift(d, c, a)), b(shift(d, c, u)), b(shift(d, c, v))),
        Term::PLam(x, body) =>
            Term::PLam(x.clone(), b(shift(d, c + 1, body))),
        Term::PApp(p, r) =>
            Term::PApp(b(shift(d, c, p)), b(shift(d, c, r))),
        Term::THComp(a, phi, u, u0) =>
            Term::THComp(
                b(shift(d, c, a)), b(shift(d, c, phi)),
                b(shift(d, c, u)), b(shift(d, c, u0)),
            ),
        Term::TEquiv(a, bx) =>
            Term::TEquiv(b(shift(d, c, a)), b(shift(d, c, bx))),
        Term::TMkEquiv(a, bx, f, g, eta, eps) =>
            Term::TMkEquiv(
                b(shift(d, c, a)), b(shift(d, c, bx)),
                b(shift(d, c, f)), b(shift(d, c, g)),
                b(shift(d, c, eta)), b(shift(d, c, eps)),
            ),
        Term::TEquivFwd(e, x) =>
            Term::TEquivFwd(b(shift(d, c, e)), b(shift(d, c, x))),
        Term::TUa(e) =>
            Term::TUa(b(shift(d, c, e))),
        Term::TTransport(p, x) =>
            Term::TTransport(b(shift(d, c, p)), b(shift(d, c, x))),
        Term::TGlue(a, phi, te) =>
            Term::TGlue(b(shift(d, c, a)), b(shift(d, c, phi)), b(shift(d, c, te))),
        Term::TGlueElem(phi, t, a) =>
            Term::TGlueElem(b(shift(d, c, phi)), b(shift(d, c, t)), b(shift(d, c, a))),
        Term::TUnglue(phi, te, g) =>
            Term::TUnglue(b(shift(d, c, phi)), b(shift(d, c, te)), b(shift(d, c, g))),
        Term::TSigma(x, a, body) =>
            Term::TSigma(x.clone(), b(shift(d, c, a)), b(shift(d, c + 1, body))),
        Term::TPair(a, bx) =>
            Term::TPair(b(shift(d, c, a)), b(shift(d, c, bx))),
        Term::TFst(p) => Term::TFst(b(shift(d, c, p))),
        Term::TSnd(p) => Term::TSnd(b(shift(d, c, p))),
    }
}

// ---------------------------------------------------------------------------
// Substitution
// ---------------------------------------------------------------------------

/// Substitute de Bruijn index `j` with `s` inside `term`.
pub fn subst(j: i32, s: &Term, term: &Term) -> Term {
    match term {
        Term::TVar(i) =>
            if *i == j { s.clone() } else { Term::TVar(*i) },
        Term::TApp(f, a) =>
            Term::TApp(b(subst(j, s, f)), b(subst(j, s, a))),
        Term::TAbs(x, body) => {
            let s1 = shift(1, 0, s);
            Term::TAbs(x.clone(), b(subst(j + 1, &s1, body)))
        }
        Term::TPi(x, a, body) => {
            let s1 = shift(1, 0, s);
            Term::TPi(x.clone(), b(subst(j, s, a)), b(subst(j + 1, &s1, body)))
        }
        Term::TUniv(n) => Term::TUniv(*n),
        Term::TIntervalTy => Term::TIntervalTy,
        Term::TInterval(i) => Term::TInterval(i.clone()),
        Term::TCube(cu) => Term::TCube(cu.clone()),
        Term::TPath(a, u, v) =>
            Term::TPath(b(subst(j, s, a)), b(subst(j, s, u)), b(subst(j, s, v))),
        Term::PLam(x, body) => {
            let s1 = shift(1, 0, s);
            Term::PLam(x.clone(), b(subst(j + 1, &s1, body)))
        }
        Term::PApp(p, r) =>
            Term::PApp(b(subst(j, s, p)), b(subst(j, s, r))),
        Term::THComp(a, phi, u, u0) =>
            Term::THComp(
                b(subst(j, s, a)), b(subst(j, s, phi)),
                b(subst(j, s, u)), b(subst(j, s, u0)),
            ),
        Term::TEquiv(a, bx) =>
            Term::TEquiv(b(subst(j, s, a)), b(subst(j, s, bx))),
        Term::TMkEquiv(a, bx, f, g, eta, eps) =>
            Term::TMkEquiv(
                b(subst(j, s, a)), b(subst(j, s, bx)),
                b(subst(j, s, f)), b(subst(j, s, g)),
                b(subst(j, s, eta)), b(subst(j, s, eps)),
            ),
        Term::TEquivFwd(e, x) =>
            Term::TEquivFwd(b(subst(j, s, e)), b(subst(j, s, x))),
        Term::TUa(e) =>
            Term::TUa(b(subst(j, s, e))),
        Term::TTransport(p, x) =>
            Term::TTransport(b(subst(j, s, p)), b(subst(j, s, x))),
        Term::TGlue(a, phi, te) =>
            Term::TGlue(b(subst(j, s, a)), b(subst(j, s, phi)), b(subst(j, s, te))),
        Term::TGlueElem(phi, t, a) =>
            Term::TGlueElem(b(subst(j, s, phi)), b(subst(j, s, t)), b(subst(j, s, a))),
        Term::TUnglue(phi, te, g) =>
            Term::TUnglue(b(subst(j, s, phi)), b(subst(j, s, te)), b(subst(j, s, g))),
        Term::TSigma(x, a, body) => {
            let s1 = shift(1, 0, s);
            Term::TSigma(x.clone(), b(subst(j, s, a)), b(subst(j + 1, &s1, body)))
        }
        Term::TPair(a, bx) =>
            Term::TPair(b(subst(j, s, a)), b(subst(j, s, bx))),
        Term::TFst(p) => Term::TFst(b(subst(j, s, p))),
        Term::TSnd(p) => Term::TSnd(b(subst(j, s, p))),
    }
}

// ---------------------------------------------------------------------------
// Beta reduction
// ---------------------------------------------------------------------------

/// Apply `body` (with de Bruijn index 0 free) to `arg`.
pub fn beta(body: &Term, arg: &Term) -> Term {
    shift(-1, 0, &subst(0, &shift(1, 0, arg), body))
}

// ---------------------------------------------------------------------------
// Helper: box a value
// ---------------------------------------------------------------------------

#[inline]
fn b<T>(v: T) -> Box<T> {
    Box::new(v)
}