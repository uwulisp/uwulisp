use crate::env::{env_get, env_set, new_env, Env};
use crate::expr::{downgrade, is_truthy, upgrade, Expr};
use crate::macros::{eval_quasiquote, expand_macro};
use crate::reader::parse_params;

/// Evaluates an expression in the given environment.
pub fn eval(expr: &Expr, env: &Env) -> Result<Expr, String> {
    match expr {
        Expr::Number(_) => Ok(expr.clone()),
        Expr::Symbol(s) => env_get(env, s),
        Expr::Func(_) | Expr::Lambda(..) | Expr::Macro(..) => Ok(expr.clone()),
        Expr::List(list) => {
            if list.is_empty() {
                return Ok(Expr::List(vec![]));
            }

            if let Expr::Symbol(op) = &list[0] {
                match op.as_str() {
                    "quote" => return Ok(list[1].clone()),
                    "quasiquote" => return eval_quasiquote(&list[1], env, 1),
                    "unquote" => return Err("unquote outside quasiquote".into()),

                    "if" => return eval_if(list, env),
                    "define" => return eval_define(list, env),
                    "lambda" => return eval_lambda(list, env),
                    "defmacro" => return eval_defmacro(list, env),
                    "begin" => return eval_begin(list, env),
                    "let" => return eval_let(list, env),

                    _ => {
                        // If `op` names a macro, expand (with raw, unevaluated
                        // argument expressions) and evaluate the result.
                        if let Ok(Expr::Macro(params, body)) = env_get(env, op) {
                            let expanded = expand_macro(&params, &body, &list[1..])?;
                            return eval(&expanded, env);
                        }
                    }
                }
            }

            // Normal function application: evaluate operator and operands.
            let func = eval(&list[0], env)?;
            let args: Result<Vec<Expr>, String> =
                list[1..].iter().map(|e| eval(e, env)).collect();
            apply(func, &args?)
        }
    }
}

/// (if cond then [else])
fn eval_if(list: &[Expr], env: &Env) -> Result<Expr, String> {
    let cond = eval(&list[1], env)?;
    if is_truthy(&cond) {
        eval(&list[2], env)
    } else if list.len() > 3 {
        eval(&list[3], env)
    } else {
        Ok(Expr::List(vec![]))
    }
}

/// (define name expr)
fn eval_define(list: &[Expr], env: &Env) -> Result<Expr, String> {
    if let Expr::Symbol(name) = &list[1] {
        let val = eval(&list[2], env)?;
        env_set(env, name.clone(), val.clone());
        Ok(val)
    } else {
        Err("invalid define: expected (define <symbol> <expr>)".into())
    }
}

/// (lambda (params...) body)
fn eval_lambda(list: &[Expr], env: &Env) -> Result<Expr, String> {
    let params = parse_params(&list[1])?;
    // Capture a *weak* reference so that storing the lambda back into the
    // same env (e.g. `define`) does not create a strong Rc cycle.
    Ok(Expr::Lambda(params, Box::new(list[2].clone()), downgrade(env)))
}

/// (defmacro name (params...) body)
fn eval_defmacro(list: &[Expr], env: &Env) -> Result<Expr, String> {
    if let Expr::Symbol(name) = &list[1] {
        let params = parse_params(&list[2])?;
        let mac = Expr::Macro(params, Box::new(list[3].clone()));
        env_set(env, name.clone(), mac.clone());
        Ok(mac)
    } else {
        Err("invalid defmacro: expected (defmacro <symbol> (<params...>) <body>)".into())
    }
}

/// (begin expr...)
fn eval_begin(list: &[Expr], env: &Env) -> Result<Expr, String> {
    let mut result = Expr::List(vec![]);
    for e in &list[1..] {
        result = eval(e, env)?;
    }
    Ok(result)
}

/// (let ((name expr)...) body...)
fn eval_let(list: &[Expr], env: &Env) -> Result<Expr, String> {
    let new_e = new_env(Some(env.clone()));
    if let Expr::List(bindings) = &list[1] {
        for b in bindings {
            if let Expr::List(pair) = b {
                if let Expr::Symbol(name) = &pair[0] {
                    let val = eval(&pair[1], env)?;
                    env_set(&new_e, name.clone(), val);
                }
            }
        }
    }
    let mut result = Expr::List(vec![]);
    for e in &list[2..] {
        result = eval(e, &new_e)?;
    }
    Ok(result)
}

/// Applies a function (builtin or lambda) to already-evaluated arguments.
pub fn apply(func: Expr, args: &[Expr]) -> Result<Expr, String> {
    match func {
        Expr::Func(f) => f(args),
        Expr::Lambda(params, body, env) => {
            // Upgrade the weak closure env before creating the call frame.
            let strong_env = upgrade(&env)?;
            let new_e = new_env(Some(strong_env));
            for (p, a) in params.iter().zip(args.iter()) {
                env_set(&new_e, p.clone(), a.clone());
            }
            eval(&body, &new_e)
        }
        other => Err(format!("not a function: {:?}", other)),
    }
}