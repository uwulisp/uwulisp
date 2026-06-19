use std::rc::Rc;

use crate::builtins::num;
use crate::cubical::eval as ctt_eval_mod;
use crate::cubical::interval::{I, eval_interval};
use crate::cubical::syntax::Term;
use crate::cubical::typechecker as tc;
use crate::env::{Env, env_set};
use crate::expr::Expr;
use crate::gc::Heap;

// ── cubical builtins ──────────────────────────────────────────────────────────
//
// Naming conventions
// ──────────────────
// Constructors mirror their Term variant names but use kebab-case and human
// readable spellings so that Lisp code reads naturally:
//
//   (univ 0)                        → TUniv(0)
//   (interval-zero)                 → TInterval(I::I0)
//   (interval-one)                  → TInterval(I::I1)
//   (interval-var n)                → TInterval(I::IVar(n))
//   (interval-meet a b)             → TInterval(I::Meet(…))
//   (interval-join a b)             → TInterval(I::Join(…))
//   (interval-neg a)                → TInterval(I::Neg(…))
//   (var n)                         → TVar(n)           (de Bruijn index)
//   (lambda name body)              → TAbs(name, body)
//   (app f x)                       → TApp(f, x)
//   (pi name domain codomain)       → TPi(name, domain, codomain)
//   (path-type A a b)               → TPath(A, a, b)
//   (path-lambda name body)         → PLam(name, body)
//   (path-app p i)                  → PApp(p, i)
//   (sigma name domain codomain)    → TSigma(name, domain, codomain)
//   (pair a b)                      → TPair(a, b)
//   (fst p)                         → TFst(p)
//   (snd p)                         → TSnd(p)
//   (hcomp A phi tube base)         → THComp(A, phi, tube, base)
//   (transport path x)              → TTransport(path, x)
//   (equiv A B)                     → TEquiv(A, B)
//   (make-equiv A B f g eta eps)    → TMkEquiv(A, B, f, g, eta, eps)
//   (equiv-fwd e x)                 → TEquivFwd(e, x)
//   (ua e)                          → TUa(e)
//   (glue A phi te)                 → TGlue(A, phi, te)   [te = (pair type equiv)]
//   (glue-elem phi t a)             → TGlueElem(phi, t, a)
//   (unglue phi te g)               → TUnglue(phi, te, g)
//
// Inductive / HIT types
// ──────────────────────
//   (data-type name)                → TData(name)
//   (con dt c args...)              → TCon(dt, c, args)
//   (pcon dt pc r args...)          → TPCon(dt, pc, args, r)   [r = interval arg]
//   (elim motive scrut cases...)    → TElim(motive, cases, scrut)
//     each case: (con-name binder... body)
//
// Evaluation / type-checking builtins
// ─────────────────────────────────────
//   (ctt-eval  t)           → normalise t; returns Expr::CubicalTerm
//   (ctt-infer t)           → infer closed type; returns Expr::CubicalTerm
//   (ctt-check t ty)        → check t : ty; returns 1.0 on success, errors otherwise
//   (ctt-equal? t u)        → definitional equality; returns 1.0 / 0.0

/// Extracts a cubical Term from an Expr::CubicalTerm, or errors.
fn ctt(e: &Expr) -> Result<&Term, String> {
    match e {
        Expr::CubicalTerm(t) => Ok(t),
        other => Err(format!("expected cubical term, got {:?}", other)),
    }
}

/// Wraps a Term into an Expr::CubicalTerm.
#[inline]
fn wrap(t: Term) -> Expr {
    Expr::CubicalTerm(Box::new(t))
}


pub fn register_cubical(env: Env, heap: &mut Heap) {
    // ── interval atoms ───────────────────────────────────────────────────────

    env_set(
        heap,
        env,
        "interval-zero".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if !args.is_empty() {
                return Err("interval-zero: no arguments expected".into());
            }
            Ok(wrap(Term::TInterval(I::I0)))
        })),
    );

    env_set(
        heap,
        env,
        "interval-one".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if !args.is_empty() {
                return Err("interval-one: no arguments expected".into());
            }
            Ok(wrap(Term::TInterval(I::I1)))
        })),
    );

    // (interval-var n) — n is a Lisp number used as the interval variable index
    env_set(
        heap,
        env,
        "interval-var".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("interval-var: expects 1 argument".into());
            }
            let n = num(&args[0])? as i32;
            Ok(wrap(Term::TInterval(I::IVar(n))))
        })),
    );

    // (interval-meet a b)
    env_set(
        heap,
        env,
        "interval-meet".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("interval-meet: expects 2 arguments".into());
            }
            let a = ctt(&args[0])?.clone();
            let b = ctt(&args[1])?.clone();
            let (ia, ib) = (unwrap_interval(&a)?, unwrap_interval(&b)?);
            // Evaluate immediately so the DNF stays normalised.
            let dnf = eval_interval(&I::Meet(Box::new(ia), Box::new(ib)));
            Ok(wrap(Term::TCube(dnf)))
        })),
    );

    // (interval-join a b)
    env_set(
        heap,
        env,
        "interval-join".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("interval-join: expects 2 arguments".into());
            }
            let a = ctt(&args[0])?.clone();
            let b = ctt(&args[1])?.clone();
            let (ia, ib) = (unwrap_interval(&a)?, unwrap_interval(&b)?);
            let dnf = eval_interval(&I::Join(Box::new(ia), Box::new(ib)));
            Ok(wrap(Term::TCube(dnf)))
        })),
    );

    // (interval-neg a)
    env_set(
        heap,
        env,
        "interval-neg".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("interval-neg: expects 1 argument".into());
            }
            let a = ctt(&args[0])?.clone();
            let ia = unwrap_interval(&a)?;
            let dnf = eval_interval(&I::Neg(Box::new(ia)));
            Ok(wrap(Term::TCube(dnf)))
        })),
    );

    // ── de Bruijn variable ───────────────────────────────────────────────────

    // (var n) — de Bruijn index
    env_set(
        heap,
        env,
        "var".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("var: expects 1 argument (de Bruijn index)".into());
            }
            let n = num(&args[0])? as i32;
            Ok(wrap(Term::TVar(n)))
        })),
    );

    // ── universe ─────────────────────────────────────────────────────────────

    // (univ level)
    env_set(
        heap,
        env,
        "univ".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("univ: expects 1 argument (universe level)".into());
            }
            let level = num(&args[0])? as i32;
            Ok(wrap(Term::TUniv(level)))
        })),
    );

    // The interval type itself as a constant.
    env_set(
        heap,
        env,
        "interval-type".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if !args.is_empty() {
                return Err("interval-type: no arguments expected".into());
            }
            Ok(wrap(Term::TIntervalTy))
        })),
    );

    // ── function types and terms ─────────────────────────────────────────────

    // (lambda name body)  — TAbs
    env_set(
        heap,
        env,
        "clambda".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("lambda: expects (lambda name body)".into());
            }
            let name = sym_name(&args[0], "lambda")?;
            let body = ctt(&args[1])?.clone();
            Ok(wrap(Term::TAbs(name, Box::new(body))))
        })),
    );

    // (app f x)  — TApp
    env_set(
        heap,
        env,
        "app".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("app: expects (app f x)".into());
            }
            let f = ctt(&args[0])?.clone();
            let x = ctt(&args[1])?.clone();
            Ok(wrap(Term::TApp(Box::new(f), Box::new(x))))
        })),
    );

    // (pi name domain codomain)  — TPi
    env_set(
        heap,
        env,
        "pi".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("pi: expects (pi name domain codomain)".into());
            }
            let name = sym_name(&args[0], "pi")?;
            let domain = ctt(&args[1])?.clone();
            let cod = ctt(&args[2])?.clone();
            Ok(wrap(Term::TPi(name, Box::new(domain), Box::new(cod))))
        })),
    );

    // ── path types and path-lambdas ──────────────────────────────────────────

    // (path-type A a b)  — TPath(A, a, b)
    env_set(
        heap,
        env,
        "path-type".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("path-type: expects (path-type A a b)".into());
            }
            let a_ty = ctt(&args[0])?.clone();
            let a = ctt(&args[1])?.clone();
            let b = ctt(&args[2])?.clone();
            Ok(wrap(Term::TPath(Box::new(a_ty), Box::new(a), Box::new(b))))
        })),
    );

    // (path-lambda name body)  — PLam
    env_set(
        heap,
        env,
        "path-lambda".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("path-lambda: expects (path-lambda name body)".into());
            }
            let name = sym_name(&args[0], "path-lambda")?;
            let body = ctt(&args[1])?.clone();
            Ok(wrap(Term::PLam(name, Box::new(body))))
        })),
    );

    // (path-app p i)  — PApp
    env_set(
        heap,
        env,
        "path-app".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("path-app: expects (path-app p i)".into());
            }
            let p = ctt(&args[0])?.clone();
            let i = ctt(&args[1])?.clone();
            Ok(wrap(Term::PApp(Box::new(p), Box::new(i))))
        })),
    );

    // ── sigma types and pairs ────────────────────────────────────────────────

    // (sigma name domain codomain)  — TSigma
    env_set(
        heap,
        env,
        "sigma".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("sigma: expects (sigma name domain codomain)".into());
            }
            let name = sym_name(&args[0], "sigma")?;
            let domain = ctt(&args[1])?.clone();
            let cod = ctt(&args[2])?.clone();
            Ok(wrap(Term::TSigma(name, Box::new(domain), Box::new(cod))))
        })),
    );

    // (pair a b)  — TPair
    env_set(
        heap,
        env,
        "pair".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("pair: expects (pair a b)".into());
            }
            let a = ctt(&args[0])?.clone();
            let b = ctt(&args[1])?.clone();
            Ok(wrap(Term::TPair(Box::new(a), Box::new(b))))
        })),
    );

    // (fst p)  — TFst
    env_set(
        heap,
        env,
        "fst".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("fst: expects (fst pair)".into());
            }
            let p = ctt(&args[0])?.clone();
            Ok(wrap(Term::TFst(Box::new(p))))
        })),
    );

    // (snd p)  — TSnd
    env_set(
        heap,
        env,
        "snd".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("snd: expects (snd pair)".into());
            }
            let p = ctt(&args[0])?.clone();
            Ok(wrap(Term::TSnd(Box::new(p))))
        })),
    );

    // ── homogeneous composition ──────────────────────────────────────────────

    // (hcomp A phi tube base)  — THComp
    env_set(
        heap,
        env,
        "hcomp".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 4 {
                return Err("hcomp: expects (hcomp A phi tube base)".into());
            }
            let a_ty = ctt(&args[0])?.clone();
            let phi = ctt(&args[1])?.clone();
            let tube = ctt(&args[2])?.clone();
            let base = ctt(&args[3])?.clone();
            Ok(wrap(Term::THComp(
                Box::new(a_ty),
                Box::new(phi),
                Box::new(tube),
                Box::new(base),
            )))
        })),
    );

    // ── transport ────────────────────────────────────────────────────────────

    // (transport path x)  — TTransport
    env_set(
        heap,
        env,
        "transport".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("transport: expects (transport path x)".into());
            }
            let path = ctt(&args[0])?.clone();
            let x = ctt(&args[1])?.clone();
            Ok(wrap(Term::TTransport(Box::new(path), Box::new(x))))
        })),
    );

    // ── equivalences and univalence ──────────────────────────────────────────

    // (equiv A B)  — TEquiv
    env_set(
        heap,
        env,
        "equiv".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("equiv: expects (equiv A B)".into());
            }
            let a = ctt(&args[0])?.clone();
            let b = ctt(&args[1])?.clone();
            Ok(wrap(Term::TEquiv(Box::new(a), Box::new(b))))
        })),
    );

    // (make-equiv A B f g eta eps)  — TMkEquiv
    env_set(
        heap,
        env,
        "make-equiv".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 6 {
                return Err("make-equiv: expects (make-equiv A B f g eta eps)".into());
            }
            let a = ctt(&args[0])?.clone();
            let b = ctt(&args[1])?.clone();
            let f = ctt(&args[2])?.clone();
            let g = ctt(&args[3])?.clone();
            let eta = ctt(&args[4])?.clone();
            let eps = ctt(&args[5])?.clone();
            Ok(wrap(Term::TMkEquiv(
                Box::new(a),
                Box::new(b),
                Box::new(f),
                Box::new(g),
                Box::new(eta),
                Box::new(eps),
            )))
        })),
    );

    // (equiv-fwd e x)  — TEquivFwd: apply the forward direction of an equivalence
    env_set(
        heap,
        env,
        "equiv-fwd".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("equiv-fwd: expects (equiv-fwd e x)".into());
            }
            let e = ctt(&args[0])?.clone();
            let x = ctt(&args[1])?.clone();
            Ok(wrap(Term::TEquivFwd(Box::new(e), Box::new(x))))
        })),
    );

    // (ua e)  — TUa: univalence, turns an equivalence into a path of types
    env_set(
        heap,
        env,
        "ua".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("ua: expects (ua equiv)".into());
            }
            let e = ctt(&args[0])?.clone();
            Ok(wrap(Term::TUa(Box::new(e))))
        })),
    );

    // ── Glue types ───────────────────────────────────────────────────────────

    // (glue A phi T)
    // T bundles the equivalent-type family and the equivalence together as a
    // pair term — matching the actual 3-field TGlue(A, phi, T) variant.
    // The API doc's 4-field description was inaccurate; the real source folds
    // the equivalence into T (use `pair` to build it: (pair T-type equiv)).
    env_set(
        heap,
        env,
        "glue".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("glue: expects (glue A phi T) where T = (pair type equiv)".into());
            }
            let a = ctt(&args[0])?.clone();
            let phi = ctt(&args[1])?.clone();
            let t = ctt(&args[2])?.clone();
            Ok(wrap(Term::TGlue(Box::new(a), Box::new(phi), Box::new(t))))
        })),
    );

    // (glue-elem phi t a)
    // Field order matches TGlueElem(phi, t, a) in syntax.rs:
    //   phi — the face formula
    //   t   — the element on the glued side
    //   a   — the underlying element on the base type side
    env_set(
        heap,
        env,
        "glue-elem".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("glue-elem: expects (glue-elem phi t a)".into());
            }
            let phi = ctt(&args[0])?.clone();
            let t = ctt(&args[1])?.clone();
            let a = ctt(&args[2])?.clone();
            Ok(wrap(Term::TGlueElem(
                Box::new(phi),
                Box::new(t),
                Box::new(a),
            )))
        })),
    );

    // (unglue phi te g)
    // Field order matches TUnglue(phi, te, g) in syntax.rs:
    //   phi — the face formula
    //   te  — the bundled (type, equiv) pair
    //   g   — the glued term to unglue
    env_set(
        heap,
        env,
        "unglue".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("unglue: expects (unglue phi te g)".into());
            }
            let phi = ctt(&args[0])?.clone();
            let te = ctt(&args[1])?.clone();
            let g = ctt(&args[2])?.clone();
            Ok(wrap(Term::TUnglue(
                Box::new(phi),
                Box::new(te),
                Box::new(g),
            )))
        })),
    );

    // ── inductive / HIT types ────────────────────────────────────────────────
    //
    //   (data-type name)                    → TData(name)
    //   (con datatype constructor args...)  → TCon(datatype, constructor, args)
    //   (pcon datatype pconstructor r args...) → TPCon(datatype, pconstructor, args, r)
    //   (elim motive scrutinee cases...)    → TElim(motive, cases, scrutinee)
    //
    // Each `case` passed to (elim ...) must be a list of the form:
    //   (con-name binder1 binder2 ... body)
    // where `body` is a cubical term and the binders are symbols.

    // (data-type name)  — TData: the type of a declared datatype
    env_set(
        heap,
        env,
        "data-type".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("data-type: expects (data-type name)".into());
            }
            let name = sym_name(&args[0], "data-type")?;
            Ok(wrap(Term::TData(name)))
        })),
    );

    // (con datatype constructor arg0 arg1 ...)  — TCon
    // datatype and constructor are symbols; remaining args are cubical terms.
    env_set(
        heap,
        env,
        "con".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() < 2 {
                return Err("con: expects (con datatype constructor args...)".into());
            }
            let dt = sym_name(&args[0], "con")?;
            let c = sym_name(&args[1], "con")?;
            let con_args: Result<Vec<Term>, String> =
                args[2..].iter().map(|a| Ok(ctt(a)?.clone())).collect();
            Ok(wrap(Term::TCon(dt, c, con_args?)))
        })),
    );

    // (pcon datatype pconstructor r arg0 arg1 ...)  — TPCon
    // r (the interval argument) comes right after the constructor name so
    // that it mirrors the surface syntax `pcon d c r args...`.
    env_set(
        heap,
        env,
        "pcon".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() < 3 {
                return Err(
                    "pcon: expects (pcon datatype pconstructor r args...)".into(),
                );
            }
            let dt = sym_name(&args[0], "pcon")?;
            let c = sym_name(&args[1], "pcon")?;
            let r = ctt(&args[2])?.clone();
            let ord_args: Result<Vec<Term>, String> =
                args[3..].iter().map(|a| Ok(ctt(a)?.clone())).collect();
            Ok(wrap(Term::TPCon(dt, c, ord_args?, Box::new(r))))
        })),
    );

    // (elim motive scrutinee case0 case1 ...)  — TElim
    //
    // Each `caseN` must be a Lisp list:
    //   (con-name binder0 binder1 ... body)
    // The first element is a symbol (constructor name), the last element is a
    // cubical term (the case body), and everything in between is a symbol
    // (binder name).  For a path-constructor case the interval binder is the
    // last binder before the body, matching the ElimCase convention in
    // syntax.rs.
    env_set(
        heap,
        env,
        "elim".into(),
        Expr::Func(Rc::new(|args, _heap| {
            use crate::cubical::syntax::ElimCase;
            if args.len() < 2 {
                return Err(
                    "elim: expects (elim motive scrutinee case0 case1 ...)".into(),
                );
            }
            let motive = ctt(&args[0])?.clone();
            let scrut = ctt(&args[1])?.clone();

            let mut cases: Vec<ElimCase> = Vec::new();
            for raw in &args[2..] {
                // Each case must be a Lisp list (Expr::List or similar).
                let elems = match raw {
                    Expr::List(xs) => xs,
                    other => {
                        return Err(format!(
                            "elim: each case must be a list, got {:?}",
                            other
                        ))
                    }
                };
                if elems.len() < 2 {
                    return Err(
                        "elim: each case must have at least a constructor name and a body"
                            .into(),
                    );
                }
                let con_name = sym_name(&elems[0], "elim case")?;
                // Everything between the constructor name and the final element
                // is a binder name; the final element is the body term.
                let binders: Result<Vec<String>, String> = elems[1..elems.len() - 1]
                    .iter()
                    .map(|b| sym_name(b, "elim case binder"))
                    .collect();
                let body = ctt(elems.last().unwrap())?.clone();
                cases.push(ElimCase {
                    con: con_name,
                    binders: binders?,
                    body: Box::new(body),
                });
            }

            Ok(wrap(Term::TElim(
                Box::new(motive),
                cases,
                Box::new(scrut),
            )))
        })),
    );

    // ── evaluation and type-checking ─────────────────────────────────────────

    // (ctt-eval t) — normalise a cubical term
    env_set(
        heap,
        env,
        "ctt-eval".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("ctt-eval: expects exactly 1 argument".into());
            }
            let t = ctt(&args[0])?.clone();
            Ok(wrap(ctt_eval_mod::eval(&t)))
        })),
    );

    // (ctt-infer t) — infer the closed type of a cubical term.
    // Uses infer_closed_dt with an empty datatype slice; pass datatypes via the
    // full Env integration (see env.rs / infer_with_full_env) for HIT terms.
    env_set(
        heap,
        env,
        "ctt-infer".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("ctt-infer: expects exactly 1 argument".into());
            }
            let t = ctt(&args[0])?.clone();
            let ty = tc::infer_closed_dt(&[], &t).map_err(|e| format!("ctt-infer: {}", e))?;
            Ok(wrap(ty))
        })),
    );

    // (ctt-check t ty) — check that t has type ty in the empty context;
    // returns 1.0 on success and raises a Lisp error on failure.
    // Uses check_closed_dt with an empty datatype slice; for HIT terms use
    // the full env integration (check_with_full_env in env.rs).
    env_set(
        heap,
        env,
        "ctt-check".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("ctt-check: expects (ctt-check term type)".into());
            }
            let t = ctt(&args[0])?.clone();
            let ty = ctt(&args[1])?.clone();
            tc::check_closed_dt(&[], &t, &ty).map_err(|e| format!("ctt-check: {}", e))?;
            Ok(Expr::Number(1.0))
        })),
    );

    // (ctt-equal? t u) — definitional equality of two closed cubical terms;
    // returns 1.0 if equal, 0.0 otherwise.
    // `definitionally_equal` returns a plain bool (the EtaResult the API doc
    // described is the internal 3-valued type; the public wrapper collapses it).
    env_set(
        heap,
        env,
        "ctt-equal?".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("ctt-equal?: expects (ctt-equal? t u)".into());
            }
            let t = ctt(&args[0])?.clone();
            let u = ctt(&args[1])?.clone();
            use crate::cubical::equality::definitionally_equal;
            Ok(Expr::Number(if definitionally_equal(&t, &u) {
                1.0
            } else {
                0.0
            }))
        })),
    );
}

// ── helper functions ──────────────────────────────────────────────────────────

/// Extracts the name string from an Expr::Symbol (used for binder names).
fn sym_name(e: &Expr, ctx: &str) -> Result<String, String> {
    match e {
        Expr::Symbol(s) => Ok(s.clone()),
        // Allow a Lisp string stored as a quoted symbol list to be passed too.
        other => Err(format!(
            "{}: expected a symbol for the binder name, got {:?}",
            ctx, other
        )),
    }
}

/// Extracts the underlying `I` (interval expression) from a `TInterval` term,
/// or synthesises one from a `TCube` (re-wrapping the DNF as a variable-free
/// constant so that meet/join/neg can still consume it).
fn unwrap_interval(t: &Term) -> Result<I, String> {
    match t {
        Term::TInterval(i) => Ok(i.clone()),
        // A fully-evaluated cube can be re-used as a constant interval expr.
        Term::TCube(_) => Err(
            "interval-meet/join/neg: argument is already a normalised cube (TCube); \
             construct with interval-var/interval-zero/interval-one first"
                .into(),
        ),
        other => Err(format!(
            "expected an interval expression (TInterval), got {:?}",
            other
        )),
    }
}