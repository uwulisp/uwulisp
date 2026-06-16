use std::collections::HashMap;

use crate::env::Env;
use crate::eval::eval;
use crate::expr::Expr;

/// Expands a macro call by substituting argument *expressions* (unevaluated)
/// for the macro's parameters in its body.
pub fn expand_macro(params: &[String], body: &Expr, args: &[Expr]) -> Result<Expr, String> {
    let mut subst: HashMap<String, Expr> = HashMap::new();
    for (p, a) in params.iter().zip(args.iter()) {
        subst.insert(p.clone(), a.clone());
    }
    Ok(substitute(body, &subst))
}

/// Recursively replaces symbols found in `subst` throughout `expr`.
fn substitute(expr: &Expr, subst: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Symbol(s) => subst.get(s).cloned().unwrap_or_else(|| expr.clone()),
        Expr::List(l) => Expr::List(l.iter().map(|e| substitute(e, subst)).collect()),
        _ => expr.clone(),
    }
}

/// Evaluates a `quasiquote` form, handling nested `unquote` and
/// `unquote-splicing` at the appropriate depth.
pub fn eval_quasiquote(expr: &Expr, env: &Env, depth: i32) -> Result<Expr, String> {
    match expr {
        Expr::List(list) if !list.is_empty() => {
            if let Expr::Symbol(s) = &list[0] {
                if s == "unquote" {
                    if depth == 1 {
                        return eval(&list[1], env);
                    } else {
                        return Ok(Expr::List(vec![
                            Expr::Symbol("unquote".into()),
                            eval_quasiquote(&list[1], env, depth - 1)?,
                        ]));
                    }
                }
                if s == "quasiquote" {
                    return Ok(Expr::List(vec![
                        Expr::Symbol("quasiquote".into()),
                        eval_quasiquote(&list[1], env, depth + 1)?,
                    ]));
                }
            }

            let mut result = Vec::new();
            for item in list {
                // unquote-splicing: (unquote-splicing expr)
                if let Expr::List(inner) = item {
                    if inner.len() == 2 {
                        if let Expr::Symbol(s) = &inner[0] {
                            if s == "unquote-splicing" && depth == 1 {
                                let spliced = eval(&inner[1], env)?;
                                if let Expr::List(items) = spliced {
                                    result.extend(items);
                                    continue;
                                }
                            }
                        }
                    }
                }
                result.push(eval_quasiquote(item, env, depth)?);
            }
            Ok(Expr::List(result))
        }
        _ => Ok(expr.clone()),
    }
}