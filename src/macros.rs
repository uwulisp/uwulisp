use std::collections::HashMap;

use crate::env::Env;
use crate::eval::eval;
use crate::expr::Expr;

/// Expands a macro call by substituting argument *expressions* (unevaluated)
/// for the macro's parameters in its body.
pub fn expand_macro(params: &[String], body: &Expr, args: &[Expr]) -> Result<Expr, String> {
    if args.len() != params.len() {
        return Err(format!(
            "macro expects {} argument(s), got {}",
            params.len(),
            args.len()
        ));
    }
    let subst: HashMap<String, Expr> = params
        .iter()
        .zip(args.iter())
        .map(|(p, a)| (p.clone(), a.clone()))
        .collect();
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

/// Returns `Some(op_name)` when `expr` is a list whose first element is a
/// symbol — e.g. `(unquote foo)` → `Some("unquote")`. Used to identify
/// special forms inside quasiquote without deeply nested `if let` chains.
fn qq_op(expr: &Expr) -> Option<&str> {
    if let Expr::List(l) = expr {
        if let Some(Expr::Symbol(s)) = l.first() {
            return Some(s.as_str());
        }
    }
    None
}

/// Evaluates a `quasiquote` form, handling nested `unquote` and
/// `unquote-splicing` at the appropriate depth.
pub fn eval_quasiquote(expr: &Expr, env: &Env, depth: usize) -> Result<Expr, String> {
    match expr {
        Expr::List(list) if !list.is_empty() => {
            match qq_op(expr) {
                Some("unquote") => {
                    if list.len() != 2 {
                        return Err(format!(
                            "unquote expects 1 argument, got {}",
                            list.len() - 1
                        ));
                    }
                    if depth == 1 {
                        eval(&list[1], env)
                    } else {
                        Ok(Expr::List(vec![
                            Expr::Symbol("unquote".into()),
                            eval_quasiquote(&list[1], env, depth - 1)?,
                        ]))
                    }
                }

                Some("quasiquote") => {
                    if list.len() != 2 {
                        return Err(format!(
                            "quasiquote expects 1 argument, got {}",
                            list.len() - 1
                        ));
                    }
                    Ok(Expr::List(vec![
                        Expr::Symbol("quasiquote".into()),
                        eval_quasiquote(&list[1], env, depth + 1)?,
                    ]))
                }

                _ => {
                    let mut result = Vec::new();
                    for item in list {
                        if qq_op(item) == Some("unquote-splicing") {
                            let inner = match item {
                                Expr::List(l) => l,
                                _ => unreachable!(),
                            };
                            if inner.len() != 2 {
                                return Err(format!(
                                    "unquote-splicing expects 1 argument, got {}",
                                    inner.len() - 1
                                ));
                            }
                            if depth == 1 {
                                // Evaluate and splice the resulting list.
                                match eval(&inner[1], env)? {
                                    Expr::List(items) => result.extend(items),
                                    other => {
                                        return Err(format!(
                                            "unquote-splicing: expected a list, got {:?}",
                                            other
                                        ))
                                    }
                                }
                            } else {
                                // At depth > 1 reconstruct the form, like unquote does.
                                result.push(Expr::List(vec![
                                    Expr::Symbol("unquote-splicing".into()),
                                    eval_quasiquote(&inner[1], env, depth - 1)?,
                                ]));
                            }
                        } else {
                            result.push(eval_quasiquote(item, env, depth)?);
                        }
                    }
                    Ok(Expr::List(result))
                }
            }
        }
        _ => Ok(expr.clone()),
    }
}