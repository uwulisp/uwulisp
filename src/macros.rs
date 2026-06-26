use std::collections::HashMap;

use crate::env::Env;
use crate::eval::eval;
use crate::expr::Expr;
use crate::gc::Heap;

/// Expands a macro call by substituting argument *expressions* (unevaluated)
/// for the macro's parameters in its body.
///
/// Pure substitution — no evaluation, no heap access needed.
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
/// symbol — e.g. `(unquote foo)` → `Some("unquote")`.
fn qq_op(expr: &Expr) -> Option<&str> {
    if let Expr::List(l) = expr
        && let Some(Expr::Symbol(s)) = l.first() {
            return Some(s.as_str());
        }
    None
}

/// Evaluates a `quasiquote` form, handling nested `unquote` and
/// `unquote-splicing` at the appropriate depth.
///
/// ### Signature change from the Rc era
///
/// `env` is now `Env` (`GcHandle`, a `Copy` integer) instead of `&Env`
/// (`&Rc<RefCell<EnvData>>`).  Passing by value is both cheaper and simpler.
/// `heap` is required because `unquote`/`unquote-splicing` at depth 1 call
/// back into `eval`, which needs the heap for variable lookups and allocation.
pub fn eval_quasiquote(
    expr: &Expr,
    env: Env,
    heap: &mut Heap,
    depth: usize,
) -> Result<Expr, String> {
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
                        // Fully escape: evaluate the inner expression normally.
                        eval(&list[1], env, heap)
                    } else {
                        // Still nested — descend one level but keep the
                        // `unquote` wrapper for the outer quasiquote to see.
                        Ok(Expr::List(vec![
                            Expr::Symbol("unquote".into()),
                            eval_quasiquote(&list[1], env, heap, depth - 1)?,
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
                    // Entering a nested quasiquote — increment depth.
                    Ok(Expr::List(vec![
                        Expr::Symbol("quasiquote".into()),
                        eval_quasiquote(&list[1], env, heap, depth + 1)?,
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
                                // Evaluate and splice the resulting list inline.
                                match eval(&inner[1], env, heap)? {
                                    Expr::List(items) => result.extend(items),
                                    other => {
                                        return Err(format!(
                                            "unquote-splicing: expected a list, got {:?}",
                                            other
                                        ));
                                    }
                                }
                            } else {
                                // At depth > 1 reconstruct the form, like unquote does.
                                result.push(Expr::List(vec![
                                    Expr::Symbol("unquote-splicing".into()),
                                    eval_quasiquote(&inner[1], env, heap, depth - 1)?,
                                ]));
                            }
                        } else {
                            result.push(eval_quasiquote(item, env, heap, depth)?);
                        }
                    }
                    Ok(Expr::List(result))
                }
            }
        }
        // Atoms (numbers, strings, symbols not in unquote position) are
        // returned as-is regardless of depth.
        _ => Ok(expr.clone()),
    }
}
