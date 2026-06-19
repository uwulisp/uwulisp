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

    // -- Inductive types / Higher Inductive Types (HITs) --------------------
    /// Reference to a declared datatype, used as a type. `TData("S1")` ~ `S¹`.
    TData(Name),
    /// Ordinary constructor application: `TCon(datatype, constructor, args)`.
    /// `args` are positional, in declaration order.
    TCon(Name, Name, Vec<Term>),
    /// Path-constructor application: `TPCon(datatype, constructor, args, r)`.
    /// `r` is the interval argument. `args` are the constructor's ordinary
    /// arguments only (the interval argument is kept separate as `r`,
    /// matching how `PLam`/`PApp` separate interval abstraction from term
    /// abstraction).
    TPCon(Name, Name, Vec<Term>, Box<Term>),
    /// Eliminator (dependent recursor) for a datatype.
    /// `TElim(motive, cases, scrutinee)`.
    /// `motive : (x : TData(d)) -> U_n`, given as a `TAbs`-shaped term
    /// (i.e. `motive` itself binds the scrutinee, index 0 in its body).
    TElim(Box<Term>, Vec<ElimCase>, Box<Term>),
}

/// One arm of an eliminator. Binds `binders.len()` fresh variables over
/// `body`, declared outermost-first (matching `ConSig`/`PConSig` telescopes).
///
/// For an ordinary-constructor case (`con` names a `ConSig`):
///   `binders` has length `arity`, one name per constructor argument,
///   and `body` has type `motive (con binders...)`.
///
/// For a path-constructor case (`con` names a `PConSig`):
///   `binders` has length `arity + 1`: the constructor's ordinary
///   arguments (outermost-first), then the interval variable LAST.
///   `body` has type `Path (motive (pcon args... @ i)) face0case face1case`,
///   where `body` itself is a `PLam`-shaped term over the interval variable
///   (i.e. the interval binder in `binders` corresponds to a `PApp`/`PLam`
///   style abstraction, not an ordinary `TAbs`).
///   Substituting `i = 0` / `i = 1` into `body` must be `definitionally_equal`
///   to the case's own arguments substituted into the datatype's declared
///   `face0` / `face1` for that path constructor.
///
/// Binder scoping: `binders` is listed outermost-to-innermost (declaration
/// order), matching `ConSig::arg_tys` / `PConSig::arg_tys`. When pushed into
/// a context (which is innermost-first — see `Ctx` in typechecker.rs and
/// equality.rs), the LAST element of `binders` becomes index 0. For a path
/// constructor, this means the interval variable is index 0 and the last
/// ordinary argument is index 1, etc. — exactly mirroring how `PLam`/`TAbs`
/// chains nest in this codebase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElimCase {
    pub con: Name,
    pub binders: Vec<Name>,
    pub body: Box<Term>,
}

// ---------------------------------------------------------------------------
// Datatype schema (the "data" declaration mechanism)
// ---------------------------------------------------------------------------

/// Signature of an ordinary (point) constructor.
/// `arg_tys[k]` is the type of the k-th argument (0-indexed, outermost
/// first), in a scope where index 0 refers to argument 0, index 1 to
/// argument 1, etc. — i.e. `arg_tys` forms a telescope exactly like a
/// chain of `TPi` binders, read outermost-first, indices counting up.
///
/// Non-dependent / non-recursive constructors (the common case — `Bool`,
/// `Nat`, `List`) just use types that don't mention earlier arguments.
/// A self-referencing argument (recursion, e.g. `suc : Nat -> Nat`) uses
/// `TData(d)` directly as the argument type — no special-casing needed,
/// since `TData` is an ordinary term-former.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConSig {
    pub name: Name,
    pub arg_tys: Vec<Term>,
}

impl ConSig {
    pub fn arity(&self) -> usize {
        self.arg_tys.len()
    }
}

/// Signature of a path constructor (the HIT part).
/// E.g. for S¹: `PConSig { name: "loop", arg_tys: vec![], face0: TCon(S1,base,[]), face1: TCon(S1,base,[]) }`.
///
/// `arg_tys` follows the same telescope convention as `ConSig::arg_tys`
/// (outermost-first, counting up). `face0` / `face1` are terms in that
/// same scope of `arg_tys.len()` variables — the ordinary arguments only.
/// The interval argument is NOT in scope in `face0`/`face1`, since at each
/// face it is fixed to `I0`/`I1` and therefore is not a free variable of
/// the boundary term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PConSig {
    pub name: Name,
    pub arg_tys: Vec<Term>,
    pub face0: Term,
    pub face1: Term,
}

impl PConSig {
    pub fn arity(&self) -> usize {
        self.arg_tys.len()
    }
}

/// A full datatype declaration: `data Name = con1 ... | con2 ... | pcon1 ...`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datatype {
    pub name: Name,
    pub cons: Vec<ConSig>,
    pub pcons: Vec<PConSig>,
}

impl Datatype {
    pub fn find_con(&self, name: &str) -> Option<&ConSig> {
        self.cons.iter().find(|c| c.name == name)
    }
    pub fn find_pcon(&self, name: &str) -> Option<&PConSig> {
        self.pcons.iter().find(|c| c.name == name)
    }
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
        Term::TData(d) => d.clone(),
        Term::TCon(_, c, args) => {
            if args.is_empty() {
                c.clone()
            } else {
                let parts: Vec<String> = args.iter().map(|a| show_term(env, a)).collect();
                format!("({} {})", c, parts.join(" "))
            }
        }
        Term::TPCon(_, c, args, r) => {
            let mut parts: Vec<String> = args.iter().map(|a| show_term(env, a)).collect();
            parts.push(format!("@ {}", show_term(env, r)));
            format!("({} {})", c, parts.join(" "))
        }
        Term::TElim(motive, cases, scrut) => {
            let case_strs: Vec<String> = cases.iter().map(|case| {
                // binders are outermost-first in declaration; extend the
                // pretty-printing env the same way, outermost-first, so
                // nested show_term calls see innermost-first as usual.
                let mut env2 = case.binders.clone();
                env2.reverse();
                env2.extend_from_slice(env);
                format!(
                    "{} {} -> {}",
                    case.con,
                    case.binders.join(" "),
                    show_term(&env2, &case.body)
                )
            }).collect();
            format!(
                "elim[{}] {{ {} }} {}",
                show_term(env, motive),
                case_strs.join(" | "),
                show_term(env, scrut)
            )
        }
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
        Term::TData(name) => Term::TData(name.clone()),
        Term::TCon(data, con, args) =>
            Term::TCon(data.clone(), con.clone(), args.iter().map(|a| shift(d, c, a)).collect()),
        Term::TPCon(data, con, args, r) =>
            Term::TPCon(
                data.clone(), con.clone(),
                args.iter().map(|a| shift(d, c, a)).collect(),
                b(shift(d, c, r)),
            ),
        Term::TElim(motive, cases, scrut) =>
            Term::TElim(
                b(shift(d, c, motive)),
                cases.iter().map(|case| ElimCase {
                    con: case.con.clone(),
                    binders: case.binders.clone(),
                    body: b(shift(d, c + case.binders.len() as i32, &case.body)),
                }).collect(),
                b(shift(d, c, scrut)),
            ),
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
        Term::TData(name) => Term::TData(name.clone()),
        Term::TCon(data, con, args) =>
            Term::TCon(data.clone(), con.clone(), args.iter().map(|a| subst(j, s, a)).collect()),
        Term::TPCon(data, con, args, r) =>
            Term::TPCon(
                data.clone(), con.clone(),
                args.iter().map(|a| subst(j, s, a)).collect(),
                b(subst(j, s, r)),
            ),
        Term::TElim(motive, cases, scrut) =>
            Term::TElim(
                b(subst(j, s, motive)),
                cases.iter().map(|case| {
                    let n = case.binders.len() as i32;
                    let s1 = shift(n, 0, s);
                    ElimCase {
                        con: case.con.clone(),
                        binders: case.binders.clone(),
                        body: b(subst(j + n, &s1, &case.body)),
                    }
                }).collect(),
                b(subst(j, s, scrut)),
            ),
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