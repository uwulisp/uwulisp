use std::rc::Rc;

use crate::env::{env_set, new_env, Env};
use crate::expr::{is_truthy, Expr};

/// Extracts a number from an Expr, or errors with context.
fn num(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n) => Ok(*n),
        other => Err(format!("expected number, got {:?}", other)),
    }
}

/// Builds the global environment populated with builtin procedures.
pub fn global_env() -> Env {
    let env = new_env();

    register_arithmetic(&env);
    register_comparisons(&env);
    register_lists(&env);
    register_misc(&env);
    register_intervals(&env);
    register_pi_types(&env);
    register_sigma_types(&env);
    register_glue_types(&env);

    env
}

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

    env_set(env, "=".into(), cmp_fn!(==));
    env_set(env, "<".into(), cmp_fn!(<));
    env_set(env, ">".into(), cmp_fn!(>));
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

fn register_misc(env: &Env) {
    env_set(
        env,
        "print".into(),
        Expr::Func(Rc::new(|args| {
            for a in args {
                print!("{:?} ", a);
            }
            println!();
            Ok(Expr::List(vec![]))
        })),
    );
}

fn register_intervals(env: &Env) {
    // The two canonical endpoints of the interval I = [0,1].
    env_set(env, "i0".into(), Expr::Number(0.0));
    env_set(env, "i1".into(), Expr::Number(1.0));

    // (refl x): the constant path at x, i.e. a path that ignores its
    // interval argument and always evaluates to x. This is the cubical
    // "reflexivity" path -- evidence that x equals itself, viewed as a
    // degenerate line I -> A.
    env_set(
        env,
        "refl".into(),
        Expr::Func(Rc::new(|args| {
            if args.len() != 1 {
                return Err("refl: expects exactly 1 argument".into());
            }
            // The body is `(quote x)` so that re-evaluating it always
            // yields the value `x` unchanged.
            Ok(Expr::Path(
                Box::new(Expr::List(vec![
                    Expr::Symbol("quote".into()),
                    args[0].clone(),
                ])),
                Rc::new(crate::expr::LexEnv::Empty),
            ))
        })),
    );
}

fn register_pi_types(env: &Env) {
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

fn register_sigma_types(env: &Env) {
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

fn register_glue_types(env: &Env) {
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