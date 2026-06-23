// Cubical Eval — Rust port of eval.hs
//
// Depends on:
//   crate::interval::{dnf_top, dnf_bot, eval_interval, I}
//   crate::syntax::{Term, Name, shift, subst, beta}

use crate::cubical::interval::{I, dnf_bot, dnf_top, eval_interval};
use crate::cubical::syntax::{Term, beta, shift, subst};

// ---------------------------------------------------------------------------
// DNF Helpers
// ---------------------------------------------------------------------------

pub fn is_top_dnf(t: &Term) -> bool {
    matches!(t, Term::TCube(d) if *d == dnf_top())
}

pub fn is_bot_dnf(t: &Term) -> bool {
    matches!(t, Term::TCube(d) if *d == dnf_bot())
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

/// Structural (syntactic) equality — used only for the trivial-path check
/// inside transport; no eta-expansion.
fn syntactic_eq(a: &Term, b: &Term) -> bool {
    a == b
}

pub fn eval(t: &Term) -> Term {
    match t {
        // ------------------------------------------------------------------
        // Application
        // ------------------------------------------------------------------
        Term::TApp(f, a) => {
            let f_ = eval(f);
            let a_ = eval(a);
            match f_ {
                Term::TAbs(_, body) => eval(&beta(&body, &a_)),
                f_ => Term::TApp(Box::new(f_), Box::new(a_)),
            }
        }

        // ------------------------------------------------------------------
        // Path application
        // ------------------------------------------------------------------
        Term::PApp(p, r) => {
            let r_ = eval(r);
            let p_ = eval(p);
            match p_ {
                Term::PLam(_, body) => eval(&beta(&body, &r_)),
                p_ => Term::PApp(Box::new(p_), Box::new(r_)),
            }
        }

        // ------------------------------------------------------------------
        // Congruence cases (evaluate under binders / sub-terms)
        // ------------------------------------------------------------------
        Term::TAbs(x, b) => Term::TAbs(x.clone(), Box::new(eval(b))),
        Term::TPi(x, a, b) => Term::TPi(x.clone(), Box::new(eval(a)), Box::new(eval(b))),
        Term::TPath(a, u, v) => {
            Term::TPath(Box::new(eval(a)), Box::new(eval(u)), Box::new(eval(v)))
        }
        Term::PLam(i, b) => Term::PLam(i.clone(), Box::new(eval(b))),
        Term::TInterval(i) => Term::TCube(eval_interval(i)),

        // ------------------------------------------------------------------
        // Homogeneous composition
        // ------------------------------------------------------------------
        Term::THComp(a_ty, phi, tube, base) => {
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                let tube_ = eval(tube);
                match tube_ {
                    Term::PLam(_, body) => eval(&beta(&body, &Term::TInterval(I::I1))),
                    tube_ => Term::PApp(Box::new(tube_), Box::new(Term::TInterval(I::I1))),
                }
            } else if is_bot_dnf(&phi_) {
                eval(base)
            } else {
                Term::THComp(
                    Box::new(eval(a_ty)),
                    Box::new(phi_),
                    Box::new(eval(tube)),
                    Box::new(eval(base)),
                )
            }
        }

        // ------------------------------------------------------------------
        // Equivalences
        // ------------------------------------------------------------------
        Term::TEquiv(a, b) => Term::TEquiv(Box::new(eval(a)), Box::new(eval(b))),

        Term::TMkEquiv(a, b, f, g, eta, eps) => Term::TMkEquiv(
            Box::new(eval(a)),
            Box::new(eval(b)),
            Box::new(eval(f)),
            Box::new(eval(g)),
            Box::new(eval(eta)),
            Box::new(eval(eps)),
        ),

        Term::TEquivFwd(e, x) => {
            let e_ = eval(e);
            let x_ = eval(x);
            match &e_ {
                Term::TMkEquiv(_, _, f, _, _, _) => eval(&Term::TApp(f.clone(), Box::new(x_))),
                _ => Term::TEquivFwd(Box::new(e_), Box::new(x_)),
            }
        }

        Term::TUa(e) => Term::TUa(Box::new(eval(e))),

        // ------------------------------------------------------------------
        // Transport
        // ------------------------------------------------------------------
        Term::TTransport(p, x) => {
            let p_ = eval(p);
            let x_ = eval(x);
            eval_transport(p_, x_)
        }

        // ------------------------------------------------------------------
        // Glue types
        // ------------------------------------------------------------------
        Term::TGlue(a_ty, phi, te) => {
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                let te_ = eval(te);
                match &te_ {
                    Term::TAbs(_, body) => {
                        let body_at_1 = beta(body, &Term::TInterval(I::I1));
                        equiv_dom(&body_at_1)
                    }
                    other => equiv_dom(other),
                }
            } else if is_bot_dnf(&phi_) {
                eval(a_ty)
            } else {
                Term::TGlue(Box::new(eval(a_ty)), Box::new(phi_), Box::new(eval(te)))
            }
        }

        Term::TGlueElem(phi, t, a) => {
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                eval(t)
            } else if is_bot_dnf(&phi_) {
                eval(a)
            } else {
                Term::TGlueElem(Box::new(phi_), Box::new(eval(t)), Box::new(eval(a)))
            }
        }

        Term::TUnglue(phi, te, g) => {
            let phi_ = eval(phi);
            if is_top_dnf(&phi_) {
                eval(&Term::TEquivFwd(Box::new(eval(te)), Box::new(eval(g))))
            } else if is_bot_dnf(&phi_) {
                eval(g)
            } else {
                Term::TUnglue(Box::new(phi_), Box::new(eval(te)), Box::new(eval(g)))
            }
        }

        // ------------------------------------------------------------------
        // Sigma types & pairs
        // ------------------------------------------------------------------
        Term::TSigma(x, a, b) => Term::TSigma(x.clone(), Box::new(eval(a)), Box::new(eval(b))),

        Term::TPair(a, b) => Term::TPair(Box::new(eval(a)), Box::new(eval(b))),

        // fst (a , b)  →  a
        Term::TFst(p) => match eval(p) {
            Term::TPair(a, _) => *a,
            p_ => Term::TFst(Box::new(p_)),
        },

        // snd (a , b)  →  b
        Term::TSnd(p) => match eval(p) {
            Term::TPair(_, b) => *b,
            p_ => Term::TSnd(Box::new(p_)),
        },

        // ------------------------------------------------------------------
        // Inductive types / HITs
        // ------------------------------------------------------------------

        // Constructors: congruence only — evaluate children, rebuild.
        Term::TCon(data, con, args) => {
            Term::TCon(data.clone(), con.clone(), args.iter().map(eval).collect())
        }

        Term::TPCon(data, con, args, r) => Term::TPCon(
            data.clone(),
            con.clone(),
            args.iter().map(eval).collect(),
            Box::new(eval(r)),
        ),

        // Eliminator: ι-reduction on constructors / path constructors,
        // else stuck (rebuilt with evaluated children).
        Term::TElim(motive, cases, scrut) => {
            let scrut_ = eval(scrut);
            match &scrut_ {
                // Ordinary constructor: substitute its args for the
                // matching case's binders, then evaluate the body.
                Term::TCon(_, con_name, args) => {
                    match cases.iter().find(|case| &case.con == con_name) {
                        Some(case) => eval(&subst_case_args(&case.binders, args, &case.body)),
                        // No matching case: stuck (ill-typed if the
                        // eliminator was well-checked, but eval doesn't
                        // assume well-typedness).
                        None => {
                            Term::TElim(Box::new(eval(motive)), eval_cases(cases), Box::new(scrut_))
                        }
                    }
                }

                // Path constructor: substitute ordinary args into the
                // case's (PLam-shaped) body, then PApp at the same
                // interval argument `r` — mirroring THComp's tube
                // reducing to its I1 face.
                Term::TPCon(_, con_name, args, r) => {
                    match cases.iter().find(|case| &case.con == con_name) {
                        Some(case) if !case.binders.is_empty() => {
                            // `case.body` is PLam-shaped: its OWN binder
                            // (the interval variable) is still open here.
                            // We substitute the ordinary args directly via
                            // `subst` at indices 0..arity-1 and deliberately
                            // do NOT use `subst_case_args`/`beta`: `beta`
                            // closes (removes) the binder it substitutes,
                            // but here we must leave the PLam binder open
                            // so the result is still PApp-able at `r`.
                            // `subst`'s own PLam case (in syntax.rs) already
                            // shifts the substituted value by 1 when it
                            // crosses that binder, so calling `subst` here
                            // at the *outer* indices 0..arity-1 is correct
                            // without any manual pre-shifting on our part.
                            let body_open = subst_ord_args_open(args, &case.body);
                            eval(&Term::PApp(Box::new(body_open), Box::new((**r).clone())))
                        }
                        _ => {
                            Term::TElim(Box::new(eval(motive)), eval_cases(cases), Box::new(scrut_))
                        }
                    }
                }

                // Neutral scrutinee: stuck.
                _ => Term::TElim(Box::new(eval(motive)), eval_cases(cases), Box::new(scrut_)),
            }
        }

        // ------------------------------------------------------------------
        // Atoms: already in normal form
        // ------------------------------------------------------------------
        _ => t.clone(),
    }
}

// ---------------------------------------------------------------------------
// Inductive types / HITs: helpers
// ---------------------------------------------------------------------------

/// Evaluate the bodies of a case list under no substitution (used only on
/// the "stuck" paths above, where children still need to be normalized).
fn eval_cases(cases: &[crate::cubical::syntax::ElimCase]) -> Vec<crate::cubical::syntax::ElimCase> {
    cases
        .iter()
        .map(|case| crate::cubical::syntax::ElimCase {
            con: case.con.clone(),
            binders: case.binders.clone(),
            body: Box::new(eval(&case.body)),
        })
        .collect()
}

/// Substitute `args` (outermost-first) for the ordinary-argument positions
/// of a path-constructor case body, WITHOUT closing/removing any binder.
///
/// Unlike `subst_case_args` (used for ordinary constructors, where the
/// case body has no further binder and we want a fully-closed result),
/// here `body` is `PLam`-shaped: its own binder (the interval variable)
/// must stay open, since the caller still needs to `PApp` the result at a
/// concrete interval term. So we substitute at the *outer* indices
/// `0..args.len()-1` directly via `subst`, and rely on `subst`'s own
/// per-binder shifting (see its `PLam`/`TAbs` cases in syntax.rs) to
/// correctly thread `args` underneath the still-open `PLam` — we must NOT
/// pre-shift `args` ourselves, since `subst` already does that exactly
/// once per binder it crosses.
fn subst_ord_args_open(args: &[Term], body: &Term) -> Term {
    args.iter()
        .rev()
        .enumerate()
        .fold(body.clone(), |acc, (offset, arg)| {
            subst(offset as i32, arg, &acc)
        })
}

/// Substitute `args` (innermost binder = last arg, matching declaration
/// order: `binders` and `args` are both outermost-first) for `binders` in
/// `body`, closing all of them. This generalizes `beta` (single-binder
/// substitution) to a telescope of `binders.len()` simultaneous binders.
///
/// We substitute from the innermost binder outward: the innermost binder
/// is index 0 in `body`, so we substitute index 0 first with the last arg,
/// shift-and-close, then the new index 0 (formerly index 1) with the
/// second-to-last arg, and so on. This is exactly `beta` applied
/// `binders.len()` times, matching how `apply_globals` in env.rs closes
/// a telescope of substitutions one at a time.
fn subst_case_args(binders: &[crate::cubical::syntax::Name], args: &[Term], body: &Term) -> Term {
    debug_assert_eq!(binders.len(), args.len());
    args.iter()
        .rev()
        .fold(body.clone(), |acc, arg| beta(&acc, arg))
}

// ---------------------------------------------------------------------------
// Transport (extracted for readability)
// ---------------------------------------------------------------------------

fn eval_transport(p_: Term, x_: Term) -> Term {
    match p_ {
        // ua e : Path U A B  →  transport (ua e) x  =  equivFwd e x
        Term::TUa(ref e) => eval(&Term::TEquivFwd(e.clone(), Box::new(x_))),

        Term::PLam(ref i_name, ref body) => {
            let b0 = eval(&beta(body, &Term::TInterval(I::I0)));
            let b1 = eval(&beta(body, &Term::TInterval(I::I1)));

            // Trivial (constant) path: transport is identity
            if syntactic_eq(&b0, &b1) {
                return x_;
            }

            match (&b0, &b1) {
                // ------------------------------------------------------
                // Pi transport (non-dependent codomain only)
                // ------------------------------------------------------
                (Term::TPi(arg_name, _, _), Term::TPi(_, _, _)) => {
                    let arg_name = arg_name.clone();
                    let i_name = i_name.clone();

                    // B-family: ⟨i⟩ B i
                    let b0_body = match &b0 {
                        Term::TPi(_, _, b) => (**b).clone(),
                        _ => b0.clone(),
                    };
                    let b_fam = Term::PLam(
                        i_name.clone(),
                        Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                            Term::TPi(_, _, b_i) => *b_i,
                            _ => shift(1, 0, &b0_body),
                        }),
                    );

                    // Is B non-dependent in a (TVar 0)?
                    let b_non_dep = match &b0 {
                        Term::TPi(_, _, b0_body) => subst(0, &Term::TUniv(0), b0_body) == **b0_body,
                        _ => false,
                    };

                    if b_non_dep {
                        // λ a. transport (⟨i⟩ B i) (f a)
                        let x_shifted = shift(1, 0, &x_);
                        Term::TAbs(
                            arg_name,
                            Box::new(eval(&Term::TTransport(
                                Box::new(b_fam),
                                Box::new(eval(&Term::TApp(
                                    Box::new(x_shifted),
                                    Box::new(Term::TVar(0)),
                                ))),
                            ))),
                        )
                    } else {
                        // Dependent B: stuck
                        Term::TTransport(Box::new(Term::PLam(i_name, body.clone())), Box::new(x_))
                    }
                }

                // ------------------------------------------------------
                // Path transport
                // ------------------------------------------------------
                (Term::TPath(ty_a0, _, _), Term::TPath(_, _, _)) => {
                    let i_name = i_name.clone();
                    let ty_a0 = (**ty_a0).clone();

                    // A-family: ⟨i⟩ A i
                    let a_fam = Term::PLam(
                        i_name.clone(),
                        Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                            Term::TPath(a, _, _) => *a,
                            _ => shift(1, 0, &ty_a0),
                        }),
                    );

                    // ⟨j⟩ transport (⟨i⟩ A i) (x @ j)
                    let a_fam_s = shift(1, 0, &a_fam);
                    let x_shifted = shift(1, 0, &x_);
                    Term::PLam(
                        "j".to_string(),
                        Box::new(eval(&Term::TTransport(
                            Box::new(a_fam_s),
                            Box::new(Term::PApp(Box::new(x_shifted), Box::new(Term::TVar(0)))),
                        ))),
                    )
                }

                // ------------------------------------------------------
                // Sigma transport
                // ------------------------------------------------------
                (Term::TSigma(_, _, _), Term::TSigma(_, _, _)) => {
                    match x_ {
                        Term::TPair(ref a, ref b) => {
                            let i_name = i_name.clone();

                            let b0_a = match &b0 {
                                Term::TSigma(_, a, _) => (**a).clone(),
                                _ => b0.clone(),
                            };
                            let b0_b = match &b0 {
                                Term::TSigma(_, _, bz) => (**bz).clone(),
                                _ => b0.clone(),
                            };

                            // A-family: ⟨i⟩ A i
                            let a_fam = Term::PLam(
                                i_name.clone(),
                                Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                                    Term::TSigma(_, a_i, _) => *a_i,
                                    _ => shift(1, 0, &b0_a),
                                }),
                            );

                            // transport along A
                            let a_prime =
                                eval(&Term::TTransport(Box::new(a_fam.clone()), a.clone()));

                            // B-family along fill: ⟨i⟩ B i (fill A a i)
                            let a_clone = (**a).clone();
                            let b_fam = Term::PLam(
                                i_name.clone(),
                                Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                                    Term::TSigma(_, _, b_i) => {
                                        // fill at i=TVar 0: transport (⟨j⟩ A (i∧j)) a
                                        let fill_at_i = eval(&Term::TTransport(
                                            Box::new(Term::PLam(
                                                "j".to_string(),
                                                Box::new(eval(&Term::PApp(
                                                    Box::new(shift(2, 0, &a_fam)),
                                                    Box::new(Term::TInterval(I::Meet(
                                                        Box::new(I::IVar(1)),
                                                        Box::new(I::IVar(0)),
                                                    ))),
                                                ))),
                                            )),
                                            Box::new(shift(1, 0, &a_clone)),
                                        ));
                                        eval(&beta(&b_i, &fill_at_i))
                                    }
                                    _ => shift(1, 0, &b0_b),
                                }),
                            );

                            let b_prime = eval(&Term::TTransport(Box::new(b_fam), b.clone()));
                            Term::TPair(Box::new(a_prime), Box::new(b_prime))
                        }
                        // non-pair: stuck
                        _ => Term::TTransport(
                            Box::new(Term::PLam(i_name.clone(), body.clone())),
                            Box::new(x_),
                        ),
                    }
                }

                // ------------------------------------------------------
                // Glue degenerate cases
                // ------------------------------------------------------
                (Term::TGlue(_, phi0, _), Term::TGlue(_, _, _)) => {
                    let i_name = i_name.clone();
                    if is_bot_dnf(&eval(phi0)) {
                        eval(&Term::TTransport(
                            Box::new(Term::PLam(
                                i_name.clone(),
                                Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                                    Term::TGlue(a, _, _) => *a,
                                    other => other,
                                }),
                            )),
                            Box::new(x_),
                        ))
                    } else if is_top_dnf(&eval(phi0)) {
                        eval(&Term::TTransport(
                            Box::new(Term::PLam(
                                i_name.clone(),
                                Box::new(match eval(&beta(&shift(1, 0, body), &Term::TVar(0))) {
                                    Term::TGlue(_, _, te) => equiv_dom(&eval(&te)),
                                    other => other,
                                }),
                            )),
                            Box::new(x_),
                        ))
                    } else {
                        // General Glue: stuck
                        Term::TTransport(Box::new(Term::PLam(i_name, body.clone())), Box::new(x_))
                    }
                }

                // Everything else: stuck
                _ => Term::TTransport(
                    Box::new(Term::PLam(i_name.clone(), body.clone())),
                    Box::new(x_),
                ),
            }
        }

        // Non-lambda path: stuck
        p_ => Term::TTransport(Box::new(p_), Box::new(x_)),
    }
}

// ---------------------------------------------------------------------------
// Extract the domain type from an equivalence term.
// ---------------------------------------------------------------------------

pub fn equiv_dom(t: &Term) -> Term {
    match t {
        Term::TMkEquiv(a, _, _, _, _, _) => (**a).clone(),
        Term::TEquiv(a, _) => (**a).clone(),
        Term::TPair(a, _) => (**a).clone(),
        other => other.clone(),
    }
}
