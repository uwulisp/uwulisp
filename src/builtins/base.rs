use std::rc::Rc;

use crate::builtins::num;
use crate::env::{Env, env_set};
use crate::eval::apply;
use crate::expr::{Expr, is_truthy};
use crate::gc::Heap;

pub fn register_arithmetic(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "+".into(),
        Expr::Func(Rc::new(|args, _heap| {
            let mut sum = 0.0;
            let mut any_float = false;
            for a in args {
                let n = num(a)?;
                any_float = any_float || matches!(a, Expr::Float(_));
                sum += n;
            }
            if any_float { Ok(Expr::Float(sum)) } else { Ok(Expr::Int(sum as i64)) }
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
                let n = num(&args[0])?;
                let any_float = matches!(args[0], Expr::Float(_));
                return if any_float { Ok(Expr::Float(-n)) } else { Ok(Expr::Int(-(n as i64))) };
            }
            let mut it = args.iter();
            let first = it.next().unwrap();
            let mut any_float = matches!(first, Expr::Float(_));
            let mut acc = num(first)?;
            for a in it {
                any_float = any_float || matches!(a, Expr::Float(_));
                acc -= num(a)?;
            }
            if any_float { Ok(Expr::Float(acc)) } else { Ok(Expr::Int(acc as i64)) }
        })),
    );

    env_set(
        heap,
        env,
        "*".into(),
        Expr::Func(Rc::new(|args, _heap| {
            let mut prod = 1.0;
            let mut any_float = false;
            for a in args {
                let n = num(a)?;
                any_float = any_float || matches!(a, Expr::Float(_));
                prod *= n;
            }
            if any_float { Ok(Expr::Float(prod)) } else { Ok(Expr::Int(prod as i64)) }
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
            Ok(Expr::Float(acc))
        })),
    );

    env_set(
        heap,
        env,
        "%".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("%: expects exactly 2 arguments".into());
            }
            let a = match &args[0] {
                Expr::Int(n) => *n,
                _ => return Err("%: arguments must be integers".into()),
            };
            let b = match &args[1] {
                Expr::Int(n) => *n,
                _ => return Err("%: arguments must be integers".into()),
            };
            if b == 0 {
                return Err("%: division by zero".into());
            }
            Ok(Expr::Int(a % b))
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
                Ok(Expr::Bool(a $op b))
            }))
        };
    }

    env_set(heap, env, "=".into(), cmp_fn!(==));
    env_set(heap, env, "<".into(), cmp_fn!(<));
    env_set(heap, env, ">".into(), cmp_fn!(>));
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
            Ok(Expr::Bool(!is_truthy(&args[0])))
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
                other => result.push(other.clone()),
            }
            Ok(Expr::List(result))
        })),
    );

    env_set(
        heap,
        env,
        "null?".into(),
        Expr::Func(Rc::new(|args, _heap| match &args[0] {
            Expr::List(l) => Ok(Expr::Bool(l.is_empty())),
            _ => Ok(Expr::Bool(false)),
        })),
    );
}

pub fn register_higher_order(env: Env, heap: &mut Heap) {
    // `apply` needs a `call_site_env` to use as a GC root while the call is
    // in flight (see eval.rs). It does NOT affect scoping — a `Lambda`
    // always runs in its own captured `closure_env` — so it's safe to use
    // the env these builtins were registered into as that root for every
    // call made through them, regardless of where `map`/`filter`/`fold`
    // are actually invoked from.

    env_set(
        heap,
        env,
        "map".into(),
        Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 2 {
                return Err("map: expects exactly 2 arguments (f list)".into());
            }
            let f = args[0].clone();
            let list = match &args[1] {
                Expr::List(l) => l,
                other => return Err(format!("map: not a list: {:?}", other)),
            };
            let mut result = Vec::with_capacity(list.len());
            for item in list {
                result.push(apply(f.clone(), &[item.clone()], env, heap)?);
            }
            Ok(Expr::List(result))
        })),
    );

    env_set(
        heap,
        env,
        "filter".into(),
        Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 2 {
                return Err("filter: expects exactly 2 arguments (pred list)".into());
            }
            let pred = args[0].clone();
            let list = match &args[1] {
                Expr::List(l) => l,
                other => return Err(format!("filter: not a list: {:?}", other)),
            };
            let mut result = Vec::new();
            for item in list {
                let keep = apply(pred.clone(), &[item.clone()], env, heap)?;
                if is_truthy(&keep) {
                    result.push(item.clone());
                }
            }
            Ok(Expr::List(result))
        })),
    );

    env_set(
        heap,
        env,
        "fold".into(),
        Expr::Func(Rc::new(move |args, heap| {
            if args.len() != 3 {
                return Err("fold: expects exactly 3 arguments (f init list)".into());
            }
            let f = args[0].clone();
            let mut acc = args[1].clone();
            let list = match &args[2] {
                Expr::List(l) => l,
                other => return Err(format!("fold: not a list: {:?}", other)),
            };
            for item in list {
                acc = apply(f.clone(), &[acc, item.clone()], env, heap)?;
            }
            Ok(acc)
        })),
    );
}
