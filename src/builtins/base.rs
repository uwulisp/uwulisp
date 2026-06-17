use std::rc::Rc;

use crate::builtins::num;
use crate::env::{Env, env_set};
use crate::expr::{Expr, is_truthy};
use crate::gc::Heap;

pub fn register_arithmetic(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "+".into(),
        Expr::Func(Rc::new(|args, _heap| {
            let mut sum = 0.0;
            for a in args {
                sum += num(a)?;
            }
            Ok(Expr::Number(sum))
        })),
    );

    env_set(
        heap,
        env,
        "-".into(),
        Expr::Func(Rc::new(|args, _heap| {
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
        heap,
        env,
        "*".into(),
        Expr::Func(Rc::new(|args, _heap| {
            let mut prod = 1.0;
            for a in args {
                prod *= num(a)?;
            }
            Ok(Expr::Number(prod))
        })),
    );

    env_set(
        heap,
        env,
        "/".into(),
        Expr::Func(Rc::new(|args, _heap| {
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

pub fn register_comparisons(env: Env, heap: &mut Heap) {
    // The macro expands to a closure whose second parameter is `_heap`.
    // Every comparison is a pure numeric computation so the heap is unused.
    macro_rules! cmp_fn {
        ($op:tt) => {
            Expr::Func(Rc::new(|args, _heap| {
                if args.len() != 2 {
                    return Err("comparison expects exactly 2 arguments".into());
                }
                let a = num(&args[0])?;
                let b = num(&args[1])?;
                Ok(Expr::Number(if a $op b { 1.0 } else { 0.0 }))
            }))
        };
    }

    env_set(heap, env, "=".into(),  cmp_fn!(==));
    env_set(heap, env, "<".into(),  cmp_fn!(<));
    env_set(heap, env, ">".into(),  cmp_fn!(>));
    env_set(heap, env, "<=".into(), cmp_fn!(<=));
    env_set(heap, env, ">=".into(), cmp_fn!(>=));

    env_set(
        heap,
        env,
        "not".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("not: expects exactly 1 argument".into());
            }
            Ok(Expr::Number(if is_truthy(&args[0]) { 0.0 } else { 1.0 }))
        })),
    );
}

pub fn register_lists(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "list".into(),
        Expr::Func(Rc::new(|args, _heap| Ok(Expr::List(args.to_vec())))),
    );

    env_set(
        heap,
        env,
        "car".into(),
        Expr::Func(Rc::new(|args, _heap| match &args[0] {
            Expr::List(l) => l
                .first()
                .cloned()
                .ok_or_else(|| "car: empty list".to_string()),
            other => Err(format!("car: not a list: {:?}", other)),
        })),
    );

    env_set(
        heap,
        env,
        "cdr".into(),
        Expr::Func(Rc::new(|args, _heap| match &args[0] {
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
        heap,
        env,
        "cons".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("cons: expects exactly 2 arguments".into());
            }
            let mut result = vec![args[0].clone()];
            match &args[1] {
                Expr::List(l) => result.extend(l.clone()),
                other         => result.push(other.clone()),
            }
            Ok(Expr::List(result))
        })),
    );

    env_set(
        heap,
        env,
        "null?".into(),
        Expr::Func(Rc::new(|args, _heap| match &args[0] {
            Expr::List(l) => Ok(Expr::Number(if l.is_empty() { 1.0 } else { 0.0 })),
            _             => Ok(Expr::Number(0.0)),
        })),
    );
}