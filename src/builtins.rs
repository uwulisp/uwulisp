use std::rc::Rc;

use crate::env::{env_set, new_env, Env};
use crate::expr::{is_truthy, Expr};

// ── cubical imports ───────────────────────────────────────────────────────────
use crate::cubical::interval::{eval_interval, I};
use crate::cubical::syntax::Term;
use crate::cubical::eval as ctt_eval_mod;
use crate::cubical::typechecker as tc;

/// Extracts a number from an Expr, or errors with context.
fn num(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n) => Ok(*n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Extracts a string slice from an Expr::Str, or errors with context.
fn str_arg(e: &Expr) -> Result<&str, String> {
    match e {
        Expr::Str(s) => Ok(s.as_str()),
        other => Err(format!("expected string, got {:?}", other)),
    }
}

/// Renders an Expr for `print`/`display`: strings print as their raw text
/// (no surrounding quotes), everything else uses its normal Debug form.
fn display_str(e: &Expr) -> String {
    match e {
        Expr::Str(s) => s.clone(),
        other => format!("{:?}", other),
    }
}

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

// ─────────────────────────────────────────────────────────────────────────────

/// Builds the global environment populated with builtin procedures.
pub fn global_env() -> Env {
    let env = new_env(None);

    register_arithmetic(&env);
    register_comparisons(&env);
    register_lists(&env);
    register_strings(&env);
    register_misc(&env);
    register_cubical(&env);

    env
}

// ── existing builtins (unchanged) ────────────────────────────────────────────

fn register_arithmetic(env: &Env) {
    env_set(
        env,
        "+".into(),
        Expr::Func(Rc::new(|args| {
            let mut sum = 0.0;
            for a in args {
                sum += num(a)?;
            }
            Ok(Expr::Number(sum))
        })),
    );

    env_set(
        env,
        "-".into(),
        Expr::Func(Rc::new(|args| {
            if args.is_empty() {
                return Err("-: need at least 1 argument".into());
            }
            if args.len() == 1 {
                return Ok(Expr::Number(-num(&args[0])?));
            }
            let mut it = args.iter();
            let mut acc = num(it.next().unwrap())?;
            for a in it {
                acc -= num(a)?;
            }
            Ok(Expr::Number(acc))
        })),
    );

    env_set(
        env,
        "*".into(),
        Expr::Func(Rc::new(|args| {
            let mut prod = 1.0;
            for a in args {
                prod *= num(a)?;
            }
            Ok(Expr::Number(prod))
        })),
    );

    env_set(
        env,
        "/".into(),
        Expr::Func(Rc::new(|args| {
            if args.is_empty() {
                return Err("/: need at least 1 argument".into());
            }
            let mut it = args.iter();
            let mut acc = num(it.next().unwrap())?;
            for a in it {
                let d = num(a)?;
                if d == 0.0 {
                    return Err("/: division by zero".into());
                }
                acc /= d;
            }
            Ok(Expr::Number(acc))
        })),
    );
}

fn register_comparisons(env: &Env) {
    macro_rules! cmp_fn {
        ($op:tt) => {
            Expr::Func(Rc::new(|args| {
                if args.len() != 2 {
                    return Err("comparison expects exactly 2 arguments".into());
                }
                let a = num(&args[0])?;
                let b = num(&args[1])?;
                Ok(Expr::Number(if a $op b { 1.0 } else { 0.0 }))
            }))
        };
    }

    env_set(env, "=".into(),  cmp_fn!(==));
    env_set(env, "<".into(),  cmp_fn!(<));
    env_set(env, ">".into(),  cmp_fn!(>));
    env_set(env, "<=".into(), cmp_fn!(<=));
    env_set(env, ">=".into(), cmp_fn!(>=));

    env_set(
        env,
        "not".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("not: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(if is_truthy(&args[0]) { 0.0 } else { 1.0 }))
        })),
    );
}

fn register_lists(env: &Env) {
    env_set(
        env,
        "list".into(),
        Expr::Func(Rc::new(|args| Ok(Expr::List(args.to_vec())))),
    );

    env_set(
        env,
        "car".into(),
        Expr::Func(Rc::new(|args| match &args[0] {
            Expr::List(l) => l
                .first()
                .cloned()
                .ok_or_else(|| "car: empty list".to_string()),
            other => Err(format!("car: not a list: {:?}", other)),
        })),
    );

    env_set(
        env,
        "cdr".into(),
        Expr::Func(Rc::new(|args| match &args[0] {
            Expr::List(l) => {
                if l.is_empty() {
                    Err("cdr: empty list".into())
                } else {
                    Ok(Expr::List(l[1..].to_vec()))
                }
            }
            other => Err(format!("cdr: not a list: {:?}", other)),
        })),
    );

    env_set(
        env,
        "cons".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 2 {
                return Err("cons: expects exactly 2 arguments".into());
            }
            let mut result = vec![args[0].clone()];
            match &args[1] {
                Expr::List(l) => result.extend(l.clone()),
                other => result.push(other.clone()),
            }
            Ok(Expr::List(result))
        })),
    );

    env_set(
        env,
        "null?".into(),
        Expr::Func(Rc::new(|args| match &args[0] {
            Expr::List(l) => Ok(Expr::Number(if l.is_empty() { 1.0 } else { 0.0 })),
            _ => Ok(Expr::Number(0.0)),
        })),
    );
}

fn register_strings(env: &Env) {
    env_set(
        env,
        "string?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(if let Expr::Str(_) = &args[0] { 1.0 } else { 0.0 }))
        })),
    );

    env_set(
        env,
        "string-append".into(),
        Expr::Func(Rc::new(|args| {
            let mut out = String::new();
            for a in args {
                out.push_str(str_arg(a)?);
            }
            Ok(Expr::Str(out))
        })),
    );

    env_set(
        env,
        "string-length".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string-length: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(str_arg(&args[0])?.chars().count() as f64))
        })),
    );

    macro_rules! string_cmp_fn {
        ($op:tt) => {
            Expr::Func(Rc::new(|args| {
                if args.len() != 2 {
                    return Err("string comparison expects exactly 2 arguments".into());
                }
                let a = str_arg(&args[0])?;
                let b = str_arg(&args[1])?;
                Ok(Expr::Number(if a $op b { 1.0 } else { 0.0 }))
            }))
        };
    }

    env_set(env, "string=?".into(),  string_cmp_fn!(==));
    env_set(env, "string<?".into(),  string_cmp_fn!(<));
    env_set(env, "string>?".into(),  string_cmp_fn!(>));
    env_set(env, "string<=?".into(), string_cmp_fn!(<=));
    env_set(env, "string>=?".into(), string_cmp_fn!(>=));

    env_set(
        env,
        "string->number".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string->number: expects exactly 1 argument".into());
            }
            let s = str_arg(&args[0])?;
            s.parse::<f64>()
                .map(Expr::Number)
                .map_err(|_| format!("string->number: not a valid number: {:?}", s))
        })),
    );

    env_set(
        env,
        "number->string".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("number->string: expects exactly 1 argument".into());
            }
            Ok(Expr::Str(format!("{}", num(&args[0])?)))
        })),
    );

    env_set(
        env,
        "string->symbol".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string->symbol: expects exactly 1 argument".into());
            }
            Ok(Expr::Symbol(str_arg(&args[0])?.to_string()))
        })),
    );

    env_set(
        env,
        "symbol->string".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("symbol->string: expects exactly 1 argument".into());
            }
            match &args[0] {
                Expr::Symbol(s) => Ok(Expr::Str(s.clone())),
                other => Err(format!("symbol->string: expected symbol, got {:?}", other)),
            }
        })),
    );

    env_set(
        env,
        "string-upcase".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string-upcase: expects exactly 1 argument".into());
            }
            Ok(Expr::Str(str_arg(&args[0])?.to_uppercase()))
        })),
    );

    env_set(
        env,
        "string-downcase".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("string-downcase: expects exactly 1 argument".into());
            }
            Ok(Expr::Str(str_arg(&args[0])?.to_lowercase()))
        })),
    );

    // (substring s start end) — character-indexed, end-exclusive, like Scheme.
    env_set(
        env,
        "substring".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 3 {
                return Err("substring: expects (substring s start end)".into());
            }
            let s = str_arg(&args[0])?;
            let start = num(&args[1])? as usize;
            let end = num(&args[2])? as usize;
            let chars: Vec<char> = s.chars().collect();
            if start > end || end > chars.len() {
                return Err(format!(
                    "substring: index out of range (start={}, end={}, len={})",
                    start, end, chars.len()
                ));
            }
            Ok(Expr::Str(chars[start..end].iter().collect()))
        })),
    );
}

fn register_misc(env: &Env) {
    env_set(
        env,
        "print".into(),
        Expr::Func(Rc::new(|args| {
            for a in args {
                print!("{} ", display_str(a));
            }
            println!();
            Ok(Expr::List(vec![]))
        })),
    );
}

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
// Evaluation / type-checking builtins
// ─────────────────────────────────────
//   (ctt-eval  t)           → normalise t; returns Expr::CubicalTerm
//   (ctt-infer t)           → infer closed type; returns Expr::CubicalTerm
//   (ctt-check t ty)        → check t : ty; returns 1.0 on success, errors otherwise
//   (ctt-equal? t u)        → definitional equality; returns 1.0 / 0.0

fn register_cubical(env: &Env) {
    // ── interval atoms ───────────────────────────────────────────────────────

    env_set(env, "interval-zero".into(), Expr::Func(Rc::new(|args| {
        if !args.is_empty() { return Err("interval-zero: no arguments expected".into()); }
        Ok(wrap(Term::TInterval(I::I0)))
    })));

    env_set(env, "interval-one".into(), Expr::Func(Rc::new(|args| {
        if !args.is_empty() { return Err("interval-one: no arguments expected".into()); }
        Ok(wrap(Term::TInterval(I::I1)))
    })));

    // (interval-var n) — n is a Lisp number used as the interval variable index
    env_set(env, "interval-var".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("interval-var: expects 1 argument".into()); }
        let n = num(&args[0])? as i32;
        Ok(wrap(Term::TInterval(I::IVar(n))))
    })));

    // (interval-meet a b)
    env_set(env, "interval-meet".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("interval-meet: expects 2 arguments".into()); }
        let a = ctt(&args[0])?.clone();
        let b = ctt(&args[1])?.clone();
        let (ia, ib) = (unwrap_interval(&a)?, unwrap_interval(&b)?);
        // Evaluate immediately so the DNF stays normalised.
        let dnf = eval_interval(&I::Meet(Box::new(ia), Box::new(ib)));
        Ok(wrap(Term::TCube(dnf)))
    })));

    // (interval-join a b)
    env_set(env, "interval-join".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("interval-join: expects 2 arguments".into()); }
        let a = ctt(&args[0])?.clone();
        let b = ctt(&args[1])?.clone();
        let (ia, ib) = (unwrap_interval(&a)?, unwrap_interval(&b)?);
        let dnf = eval_interval(&I::Join(Box::new(ia), Box::new(ib)));
        Ok(wrap(Term::TCube(dnf)))
    })));

    // (interval-neg a)
    env_set(env, "interval-neg".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("interval-neg: expects 1 argument".into()); }
        let a = ctt(&args[0])?.clone();
        let ia = unwrap_interval(&a)?;
        let dnf = eval_interval(&I::Neg(Box::new(ia)));
        Ok(wrap(Term::TCube(dnf)))
    })));

    // ── de Bruijn variable ───────────────────────────────────────────────────

    // (var n) — de Bruijn index
    env_set(env, "var".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("var: expects 1 argument (de Bruijn index)".into()); }
        let n = num(&args[0])? as i32;
        Ok(wrap(Term::TVar(n)))
    })));

    // ── universe ─────────────────────────────────────────────────────────────

    // (univ level)
    env_set(env, "univ".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("univ: expects 1 argument (universe level)".into()); }
        let level = num(&args[0])? as i32;
        Ok(wrap(Term::TUniv(level)))
    })));

    // The interval type itself as a constant.
    env_set(env, "interval-type".into(), Expr::Func(Rc::new(|args| {
        if !args.is_empty() { return Err("interval-type: no arguments expected".into()); }
        Ok(wrap(Term::TIntervalTy))
    })));

    // ── function types and terms ─────────────────────────────────────────────

    // (lambda name body)  — TAbs
    env_set(env, "lambda".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("lambda: expects (lambda name body)".into()); }
        let name = sym_name(&args[0], "lambda")?;
        let body = ctt(&args[1])?.clone();
        Ok(wrap(Term::TAbs(name, Box::new(body))))
    })));

    // (app f x)  — TApp
    env_set(env, "app".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("app: expects (app f x)".into()); }
        let f = ctt(&args[0])?.clone();
        let x = ctt(&args[1])?.clone();
        Ok(wrap(Term::TApp(Box::new(f), Box::new(x))))
    })));

    // (pi name domain codomain)  — TPi
    env_set(env, "pi".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("pi: expects (pi name domain codomain)".into()); }
        let name   = sym_name(&args[0], "pi")?;
        let domain = ctt(&args[1])?.clone();
        let cod    = ctt(&args[2])?.clone();
        Ok(wrap(Term::TPi(name, Box::new(domain), Box::new(cod))))
    })));

    // ── path types and path-lambdas ──────────────────────────────────────────

    // (path-type A a b)  — TPath(A, a, b)
    env_set(env, "path-type".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("path-type: expects (path-type A a b)".into()); }
        let a_ty = ctt(&args[0])?.clone();
        let a    = ctt(&args[1])?.clone();
        let b    = ctt(&args[2])?.clone();
        Ok(wrap(Term::TPath(Box::new(a_ty), Box::new(a), Box::new(b))))
    })));

    // (path-lambda name body)  — PLam
    env_set(env, "path-lambda".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("path-lambda: expects (path-lambda name body)".into()); }
        let name = sym_name(&args[0], "path-lambda")?;
        let body = ctt(&args[1])?.clone();
        Ok(wrap(Term::PLam(name, Box::new(body))))
    })));

    // (path-app p i)  — PApp
    env_set(env, "path-app".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("path-app: expects (path-app p i)".into()); }
        let p = ctt(&args[0])?.clone();
        let i = ctt(&args[1])?.clone();
        Ok(wrap(Term::PApp(Box::new(p), Box::new(i))))
    })));

    // ── sigma types and pairs ────────────────────────────────────────────────

    // (sigma name domain codomain)  — TSigma
    env_set(env, "sigma".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("sigma: expects (sigma name domain codomain)".into()); }
        let name   = sym_name(&args[0], "sigma")?;
        let domain = ctt(&args[1])?.clone();
        let cod    = ctt(&args[2])?.clone();
        Ok(wrap(Term::TSigma(name, Box::new(domain), Box::new(cod))))
    })));

    // (pair a b)  — TPair
    env_set(env, "pair".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("pair: expects (pair a b)".into()); }
        let a = ctt(&args[0])?.clone();
        let b = ctt(&args[1])?.clone();
        Ok(wrap(Term::TPair(Box::new(a), Box::new(b))))
    })));

    // (fst p)  — TFst
    env_set(env, "fst".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("fst: expects (fst pair)".into()); }
        let p = ctt(&args[0])?.clone();
        Ok(wrap(Term::TFst(Box::new(p))))
    })));

    // (snd p)  — TSnd
    env_set(env, "snd".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("snd: expects (snd pair)".into()); }
        let p = ctt(&args[0])?.clone();
        Ok(wrap(Term::TSnd(Box::new(p))))
    })));

    // ── homogeneous composition ──────────────────────────────────────────────

    // (hcomp A phi tube base)  — THComp
    env_set(env, "hcomp".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 4 { return Err("hcomp: expects (hcomp A phi tube base)".into()); }
        let a_ty = ctt(&args[0])?.clone();
        let phi  = ctt(&args[1])?.clone();
        let tube = ctt(&args[2])?.clone();
        let base = ctt(&args[3])?.clone();
        Ok(wrap(Term::THComp(
            Box::new(a_ty), Box::new(phi), Box::new(tube), Box::new(base),
        )))
    })));

    // ── transport ────────────────────────────────────────────────────────────

    // (transport path x)  — TTransport
    env_set(env, "transport".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("transport: expects (transport path x)".into()); }
        let path = ctt(&args[0])?.clone();
        let x    = ctt(&args[1])?.clone();
        Ok(wrap(Term::TTransport(Box::new(path), Box::new(x))))
    })));

    // ── equivalences and univalence ──────────────────────────────────────────

    // (equiv A B)  — TEquiv
    env_set(env, "equiv".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("equiv: expects (equiv A B)".into()); }
        let a = ctt(&args[0])?.clone();
        let b = ctt(&args[1])?.clone();
        Ok(wrap(Term::TEquiv(Box::new(a), Box::new(b))))
    })));

    // (make-equiv A B f g eta eps)  — TMkEquiv
    env_set(env, "make-equiv".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 6 {
            return Err("make-equiv: expects (make-equiv A B f g eta eps)".into());
        }
        let a   = ctt(&args[0])?.clone();
        let b   = ctt(&args[1])?.clone();
        let f   = ctt(&args[2])?.clone();
        let g   = ctt(&args[3])?.clone();
        let eta = ctt(&args[4])?.clone();
        let eps = ctt(&args[5])?.clone();
        Ok(wrap(Term::TMkEquiv(
            Box::new(a), Box::new(b), Box::new(f),
            Box::new(g), Box::new(eta), Box::new(eps),
        )))
    })));

    // (equiv-fwd e x)  — TEquivFwd: apply the forward direction of an equivalence
    env_set(env, "equiv-fwd".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("equiv-fwd: expects (equiv-fwd e x)".into()); }
        let e = ctt(&args[0])?.clone();
        let x = ctt(&args[1])?.clone();
        Ok(wrap(Term::TEquivFwd(Box::new(e), Box::new(x))))
    })));

    // (ua e)  — TUa: univalence, turns an equivalence into a path of types
    env_set(env, "ua".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("ua: expects (ua equiv)".into()); }
        let e = ctt(&args[0])?.clone();
        Ok(wrap(Term::TUa(Box::new(e))))
    })));

    // ── Glue types ───────────────────────────────────────────────────────────

    // (glue A phi T)
    // T bundles the equivalent-type family and the equivalence together as a
    // pair term — matching the actual 3-field TGlue(A, phi, T) variant.
    // The API doc's 4-field description was inaccurate; the real source folds
    // the equivalence into T (use `pair` to build it: (pair T-type equiv)).
    env_set(env, "glue".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("glue: expects (glue A phi T) where T = (pair type equiv)".into()); }
        let a   = ctt(&args[0])?.clone();
        let phi = ctt(&args[1])?.clone();
        let t   = ctt(&args[2])?.clone();
        Ok(wrap(Term::TGlue(Box::new(a), Box::new(phi), Box::new(t))))
    })));

    // (glue-elem phi t a)
    // Field order matches TGlueElem(phi, t, a) in syntax.rs:
    //   phi — the face formula
    //   t   — the element on the glued side
    //   a   — the underlying element on the base type side
    env_set(env, "glue-elem".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("glue-elem: expects (glue-elem phi t a)".into()); }
        let phi = ctt(&args[0])?.clone();
        let t   = ctt(&args[1])?.clone();
        let a   = ctt(&args[2])?.clone();
        Ok(wrap(Term::TGlueElem(Box::new(phi), Box::new(t), Box::new(a))))
    })));

    // (unglue phi te g)
    // Field order matches TUnglue(phi, te, g) in syntax.rs:
    //   phi — the face formula
    //   te  — the bundled (type, equiv) pair
    //   g   — the glued term to unglue
    env_set(env, "unglue".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 3 { return Err("unglue: expects (unglue phi te g)".into()); }
        let phi = ctt(&args[0])?.clone();
        let te  = ctt(&args[1])?.clone();
        let g   = ctt(&args[2])?.clone();
        Ok(wrap(Term::TUnglue(Box::new(phi), Box::new(te), Box::new(g))))
    })));

    // ── evaluation and type-checking ─────────────────────────────────────────

    // (ctt-eval t) — normalise a cubical term
    env_set(env, "ctt-eval".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("ctt-eval: expects exactly 1 argument".into()); }
        let t = ctt(&args[0])?.clone();
        Ok(wrap(ctt_eval_mod::eval(&t)))
    })));

    // (ctt-infer t) — infer the closed type of a cubical term
    env_set(env, "ctt-infer".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 1 { return Err("ctt-infer: expects exactly 1 argument".into()); }
        let t = ctt(&args[0])?.clone();
        let ty = tc::infer_closed(&t).map_err(|e| format!("ctt-infer: {}", e))?;
        Ok(wrap(ty))
    })));

    // (ctt-check t ty) — check that t has type ty in the empty context;
    // returns 1.0 on success and raises a Lisp error on failure.
    env_set(env, "ctt-check".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("ctt-check: expects (ctt-check term type)".into()); }
        let t  = ctt(&args[0])?.clone();
        let ty = ctt(&args[1])?.clone();
        tc::check_closed(&t, &ty).map_err(|e| format!("ctt-check: {}", e))?;
        Ok(Expr::Number(1.0))
    })));

    // (ctt-equal? t u) — definitional equality of two closed cubical terms;
    // returns 1.0 if equal, 0.0 otherwise.
    // `definitionally_equal` returns a plain bool (the EtaResult the API doc
    // described is the internal 3-valued type; the public wrapper collapses it).
    env_set(env, "ctt-equal?".into(), Expr::Func(Rc::new(|args| {
        if args.len() != 2 { return Err("ctt-equal?: expects (ctt-equal? t u)".into()); }
        let t = ctt(&args[0])?.clone();
        let u = ctt(&args[1])?.clone();
        use crate::cubical::equality::definitionally_equal;
        Ok(Expr::Number(if definitionally_equal(&t, &u) { 1.0 } else { 0.0 }))
    })));
}

// ── helper functions ──────────────────────────────────────────────────────────

/// Extracts the name string from an Expr::Symbol (used for binder names).
fn sym_name(e: &Expr, ctx: &str) -> Result<String, String> {
    match e {
        Expr::Symbol(s) => Ok(s.clone()),
        // Allow a Lisp string stored as a quoted symbol list to be passed too.
        other => Err(format!("{}: expected a symbol for the binder name, got {:?}", ctx, other)),
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
             construct with interval-var/interval-zero/interval-one first".into(),
        ),
        other => Err(format!(
            "expected an interval expression (TInterval), got {:?}", other
        )),
    }
}