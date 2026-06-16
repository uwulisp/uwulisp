use std::rc::Rc;

use crate::{env::{Env, env_set}, expr::Expr};

pub fn register_intervals(env: &Env) {
    // The two canonical endpoints of the interval I = [0,1].
    env_set(env, "i0".into(), Expr::Number(0.0));
    env_set(env, "i1".into(), Expr::Number(1.0));

    // (refl x): the constant path at x, i.e. a path that ignores its
    // interval argument and always evaluates to x. This is the cubical
    // "reflexivity" path -- evidence that x equals itself, viewed as a
    // degenerate line I -> A.
    //
    // Implementation: we store x at De Bruijn index 1 in the path's lexical
    // environment (index 0 is reserved for the interval variable pushed by
    // eval_papply). The body Expr::Index(1) therefore retrieves x regardless
    // of what interval point the path is applied to.
    env_set(
        env,
        "refl".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("refl: expects exactly 1 argument".into());
            }
            let x = args[0].clone();
            // Build a lexical env that holds x at index 0 of the *captured*
            // environment.  eval_papply will later prepend the interval value
            // at index 0, shifting x to index 1 — but we pre-shift here by
            // placing x at index 0 of the path env and using Index(1) as the
            // body, so it reads correctly both before and after the interval
            // is pushed.
            //
            // Concretely: path env = [x | Empty]
            //   eval_papply pushes i → [i | x | Empty]
            //   body = Index(1) → retrieves x  ✓
            let penv = Rc::new(crate::expr::LexEnv::Node(
                x,
                Rc::new(crate::expr::LexEnv::Empty),
            ));
            Ok(Expr::Path(
                Box::new(Expr::Index(1)),
                penv,
            ))
        })),
    );
}

/// Register the three fundamental derived path operations that every cubical
/// program needs immediately after `refl`.
///
///   symm  : Path a b  →  Path b a            (path reversal)
///   trans : Path a b  →  Path b c  →  Path a c   (path composition)
///   cong  : (f : A → B) → Path a b → Path (f a) (f b)   (congruence / ap)
///
/// All three are implemented as `Expr::Func` closures over `Expr::Path` with
/// `Expr::Func` bodies (the same Func-path style used by `funext`), so that
/// `eval_papply` calls the body closure directly with the interval point.
pub fn register_interval_ops(env: &Env) {
    use crate::eval::{apply, eval};
    use crate::expr::LexEnv;

    // -----------------------------------------------------------------------
    // (symm p) : Path b a  given  p : Path a b
    //
    // The reversed path is  λ i. p (1 - i).
    // At i=0 → p(1) = b  (new start).
    // At i=1 → p(0) = a  (new end).
    // -----------------------------------------------------------------------
    env_set(
        env,
        "symm".into(),
        Expr::Func(Rc::new(move |args| {
            if args.len() != 1 {
                return Err("symm: expects exactly 1 argument (a path)".into());
            }
            let p = args[0].clone();
            // Validate eagerly that we have a path.
            match &p {
                Expr::Path(..) => {}
                other => return Err(format!("symm: argument is not a path: {:?}", other)),
            }
            let body = Expr::Func(Rc::new(move |iargs: &[Expr]| {
                let i = match &iargs[0] {
                    Expr::Number(n) => *n,
                    other => return Err(format!("symm body: expected number, got {:?}", other)),
                };
                // Apply the original path at the mirror point 1-i.
                match &p {
                    Expr::Path(inner_body, penv) => match inner_body.as_ref() {
                        Expr::Func(f) => f(&[Expr::Number(1.0 - i)]),
                        other => {
                            let new_lex = Rc::new(LexEnv::Node(
                                Expr::Number(1.0 - i),
                                penv.clone(),
                            ));
                            // We have no Env handle here; use a dummy global env.
                            // symm only needs to evaluate the path body, which is
                            // a closed expression (all free vars are in penv).
                            let dummy_env = crate::env::make_env();
                            eval(other, &dummy_env, &new_lex)
                        }
                    },
                    _ => unreachable!(),
                }
            }));
            Ok(Expr::Path(
                Box::new(body),
                Rc::new(LexEnv::Empty),
            ))
        })),
    );

    // -----------------------------------------------------------------------
    // (trans p q) : Path a c  given  p : Path a b  and  q : Path b c
    //
    // Uses the standard cubical "double-speed" composition:
    //   i < 0.5  →  p (2i)       (runs p from a to b on [0, 0.5])
    //   i ≥ 0.5  →  q (2i - 1)   (runs q from b to c on [0.5, 1])
    //
    // This is the simplest correct definition; it is definitionally equal at
    // endpoints and compositional, though not symmetric under reversal.
    // -----------------------------------------------------------------------
    env_set(
        env,
        "trans".into(),
        Expr::Func(Rc::new(move |args| {
            if args.len() != 2 {
                return Err("trans: expects exactly 2 arguments (two paths)".into());
            }
            let p = args[0].clone();
            let q = args[1].clone();
            for (name, path) in [("trans p", &p), ("trans q", &q)] {
                match path {
                    Expr::Path(..) => {}
                    other => return Err(format!("{}: argument is not a path: {:?}", name, other)),
                }
            }
            let body = Expr::Func(Rc::new(move |iargs: &[Expr]| {
                let i = match &iargs[0] {
                    Expr::Number(n) => *n,
                    other => return Err(format!("trans body: expected number, got {:?}", other)),
                };
                let dummy_env = crate::env::make_env();
                // Helper: apply a Path (either Func-body or expr-body) at point t.
                let papply_path = |path: &Expr, t: f64| -> Result<Expr, String> {
                    match path {
                        Expr::Path(inner_body, penv) => match inner_body.as_ref() {
                            Expr::Func(f) => f(&[Expr::Number(t)]),
                            other => {
                                let new_lex = Rc::new(LexEnv::Node(
                                    Expr::Number(t),
                                    penv.clone(),
                                ));
                                eval(other, &dummy_env, &new_lex)
                            }
                        },
                        _ => unreachable!(),
                    }
                };
                if i < 0.5 {
                    papply_path(&p, 2.0 * i)
                } else {
                    papply_path(&q, 2.0 * i - 1.0)
                }
            }));
            Ok(Expr::Path(
                Box::new(body),
                Rc::new(LexEnv::Empty),
            ))
        })),
    );

    // -----------------------------------------------------------------------
    // (cong f p) : Path (f a) (f b)  given  f : A → B  and  p : Path a b
    //
    // cong f p  =  λ i. f (p i)
    //
    // This is the standard congruence principle (also called `ap` in HoTT).
    // f must be a function value (Expr::Func or Expr::Lambda); p must be a path.
    // -----------------------------------------------------------------------
    env_set(
        env,
        "cong".into(),
        Expr::Func(Rc::new(move |args| {
            if args.len() != 2 {
                return Err("cong: expects exactly 2 arguments (function, path)".into());
            }
            let f = args[0].clone();
            let p = args[1].clone();
            match &p {
                Expr::Path(..) => {}
                other => return Err(format!("cong: second argument is not a path: {:?}", other)),
            }
            let body = Expr::Func(Rc::new(move |iargs: &[Expr]| {
                let i = match &iargs[0] {
                    Expr::Number(n) => *n,
                    other => return Err(format!("cong body: expected number, got {:?}", other)),
                };
                let dummy_env = crate::env::make_env();
                // Evaluate p at i to get the intermediate value.
                let pi_val = match &p {
                    Expr::Path(inner_body, penv) => match inner_body.as_ref() {
                        Expr::Func(g) => g(&[Expr::Number(i)])?,
                        other => {
                            let new_lex = Rc::new(LexEnv::Node(
                                Expr::Number(i),
                                penv.clone(),
                            ));
                            eval(other, &dummy_env, &new_lex)?
                        }
                    },
                    _ => unreachable!(),
                };
                // Apply f to that value.
                apply(f.clone(), &[pi_val], &dummy_env)
            }));
            Ok(Expr::Path(
                Box::new(body),
                Rc::new(LexEnv::Empty),
            ))
        })),
    );
}

pub fn register_pi_types(env: &Env) {
    // (pi? x) -- returns 1 if x is a Pi-type value, 0 otherwise.
    // Useful for runtime type inspection / dispatch.
    env_set(
        env,
        "pi?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("pi?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(match &args[0] {
                Expr::Pi(..) => 1.0,
                _ => 0.0,
            }))
        })),
    );

    // (path? x) -- returns 1 if x is a Path value, 0 otherwise.
    env_set(
        env,
        "path?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("path?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(match &args[0] {
                Expr::Path(..) => 1.0,
                _ => 0.0,
            }))
        })),
    );
}

pub fn register_sigma_types(env: &Env) {
    // (sigma? x) -- returns 1 if x is a Sigma-type value, 0 otherwise.
    env_set(
        env,
        "sigma?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("sigma?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(match &args[0] {
                Expr::Sigma(..) => 1.0,
                _ => 0.0,
            }))
        })),
    );
}

pub fn register_glue_types(env: &Env) {
    // (glue? x) -- returns 1 if x is a Glue introduction term, 0 otherwise.
    env_set(
        env,
        "glue?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("glue?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(match &args[0] {
                Expr::Glue(..) => 1.0,
                _ => 0.0,
            }))
        })),
    );

    // (glue-type? x) -- returns 1 if x is a GlueType type former, 0 otherwise.
    env_set(
        env,
        "glue-type?".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("glue-type?: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(match &args[0] {
                Expr::GlueType(..) => 1.0,
                _ => 0.0,
            }))
        })),
    );
}