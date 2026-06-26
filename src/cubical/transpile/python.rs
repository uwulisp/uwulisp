use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cubical::interval::I;
use crate::cubical::parser::Decl;
use crate::cubical::syntax::{ConSig, Datatype, ElimCase, Name, PConSig, Term};

pub struct PythonModuleCtx {
    #[allow(dead_code)]
    pub module_name: String,
    pub imports: Vec<String>,
    pub constructors: HashMap<Name, Name>,
    #[allow(dead_code)]
    pub datatypes: HashSet<Name>,
    pub pconstructors: HashSet<Name>,
}

impl PythonModuleCtx {
    pub fn from_decls(module_name: String, decls: &[Decl]) -> Self {
        let mut imports = Vec::new();
        let mut constructors = HashMap::new();
        let mut datatypes = HashSet::new();
        let mut pconstructors = HashSet::new();

        for decl in decls {
            match decl {
                Decl::Import { path } => {
                    if let Some(name) = module_name_from_uwuc_path(path) {
                        imports.push(name.to_lowercase());
                    }
                }
                Decl::Data(dt) => {
                    register_datatype(dt, &mut constructors, &mut datatypes, &mut pconstructors);
                }
                Decl::Def { .. } => {}
            }
        }

        Self {
            module_name,
            imports,
            constructors,
            datatypes,
            pconstructors,
        }
    }
}

pub fn module_name_from_uwuc_path(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())?;
    Some(capitalize_ident(stem))
}

pub fn py_path_for_module(module_name: &str) -> PathBuf {
    PathBuf::from(format!("{}.py", module_name.to_lowercase()))
}

pub fn py_path_from_uwuc_path(path: &Path) -> PathBuf {
    py_path_for_module(&module_name_from_path(path))
}

pub fn emit_module(ctx: &mut PythonModuleCtx, decls: &[Decl], source_comment: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# generated from {}\n", source_comment));

    for imp in &ctx.imports {
        out.push_str(&format!("from {} import *\n", imp));
    }
    if !ctx.imports.is_empty() {
        out.push('\n');
    }

    let mut name_env: Vec<Name> = Vec::new();

    for decl in decls {
        match decl {
            Decl::Import { .. } => {}
            Decl::Data(dt) => {
                out.push_str(&emit_datatype(dt, &ctx.constructors));
                out.push('\n');
            }
            Decl::Def { name, ty: _, val } => {
                let py_name = sanitize_py_ident(name);
                name_env.insert(0, py_name.clone());
                out.push_str(&format!("{} = {}\n", py_name, emit_term(val, &name_env, ctx)));
                out.push('\n');
            }
        }
    }

    out
}

fn register_datatype(
    dt: &Datatype,
    constructors: &mut HashMap<Name, Name>,
    datatypes: &mut HashSet<Name>,
    pconstructors: &mut HashSet<Name>,
) {
    datatypes.insert(dt.name.clone());
    for con in &dt.cons {
        constructors.insert(con.name.clone(), sanitize_py_ident(&capitalize_ident(&con.name)));
    }
    for pcon in &dt.pcons {
        constructors.insert(pcon.name.clone(), sanitize_py_ident(&capitalize_ident(&pcon.name)));
        pconstructors.insert(pcon.name.clone());
    }
}

fn emit_datatype(dt: &Datatype, constructors: &HashMap<Name, Name>) -> String {
    let mut lines = Vec::new();
    let dt_name = sanitize_py_ident(&dt.name);
    lines.push(format!("# data {}", dt.name));
    lines.push(format!("{} = None", dt_name));
    for con in &dt.cons {
        lines.push(emit_constructor_def(con, constructors));
    }
    for pcon in &dt.pcons {
        lines.push(emit_path_constructor_def(pcon, constructors));
    }
    lines.join("\n")
}

fn emit_constructor_def(con: &ConSig, constructors: &HashMap<Name, Name>) -> String {
    let py_name = constructors
        .get(&con.name)
        .cloned()
        .unwrap_or_else(|| sanitize_py_ident(&capitalize_ident(&con.name)));
    if con.arg_tys.is_empty() {
        format!("{} = (\"{}\",)", py_name, py_name)
    } else {
        let args: Vec<String> = (0..con.arg_tys.len())
            .map(|i| format!("a{}", i))
            .collect();
        format!("{} = lambda {}: (\"{}\", {})", py_name, args.join(", "), py_name, args.join(", "))
    }
}

fn emit_path_constructor_def(pcon: &PConSig, constructors: &HashMap<Name, Name>) -> String {
    let py_name = constructors
        .get(&pcon.name)
        .cloned()
        .unwrap_or_else(|| sanitize_py_ident(&capitalize_ident(&pcon.name)));
    if pcon.arg_tys.is_empty() {
        format!("{} = (\"{}\",)", py_name, py_name)
    } else {
        let args: Vec<String> = (0..pcon.arg_tys.len())
            .map(|i| format!("a{}", i))
            .collect();
        format!("{} = lambda {}: (\"{}\", {})", py_name, args.join(", "), py_name, args.join(", "))
    }
}

pub fn emit_term(term: &Term, env: &[Name], ctx: &mut PythonModuleCtx) -> String {
    if let Some(let_expr) = try_emit_let(term, env, ctx) {
        return let_expr;
    }

    match term {
        Term::TVar(i) => env
            .get(*i as usize)
            .cloned()
            .unwrap_or_else(|| format!("v{}", i)),
        Term::TAbs(x, body) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc.clone()];
            env2.extend_from_slice(env);
            format!("lambda {}: {}", lc, emit_term(body, &env2, ctx))
        }
        Term::TApp(f, a) => format!(
            "{}({})",
            emit_term(f, env, ctx),
            emit_term(a, env, ctx)
        ),
        Term::TUniv(_) => "None".to_string(),
        Term::TIntervalTy => "None".to_string(),
        Term::TPi(_, a, b) => {
            let _a_str = emit_term(a, env, ctx);
            format!("(lambda _: {})", emit_term(b, env, ctx))
        }
        Term::TInterval(i) => emit_interval(i, env),
        Term::TCube(_) => "None".to_string(),
        Term::TPath(a, _, _) => emit_term(a, env, ctx),
        Term::PLam(x, body) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            emit_term(body, &env2, ctx)
        }
        Term::PApp(p, _) => emit_term(p, env, ctx),
        Term::THComp(_, _, _, u0) => emit_term(u0, env, ctx),
        Term::TEquiv(_, _) => "(lambda x: x)".to_string(),
        Term::TMkEquiv(_, _, f, _, _, _) => emit_term(f, env, ctx),
        Term::TEquivFwd(e, x) => format!("{}({})", emit_term(e, env, ctx), emit_term(x, env, ctx)),
        Term::TUa(_) => "(lambda x: x)".to_string(),
        Term::TTransport(_, x) => emit_term(x, env, ctx),
        Term::TGlue(a, _, _) => emit_term(a, env, ctx),
        Term::TGlueElem(_, t, _) => emit_term(t, env, ctx),
        Term::TUnglue(_, _, g) => emit_term(g, env, ctx),
        Term::TSigma(_, a, b) => {
            format!("({}, {})", emit_term(a, env, ctx), emit_term(b, env, ctx))
        }
        Term::TPair(a, b) => format!(
            "({}, {})",
            emit_term(a, env, ctx),
            emit_term(b, env, ctx)
        ),
        Term::TFst(p) => format!("{}[0]", emit_term(p, env, ctx)),
        Term::TSnd(p) => format!("{}[1]", emit_term(p, env, ctx)),
        Term::TData(name) => name.clone(),
        Term::TCon(_, con, args) => {
            let py_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| sanitize_py_ident(&capitalize_ident(con)));
            if args.is_empty() {
                py_con
            } else {
                let arg_strs: Vec<String> =
                    args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("{}({})", py_con, arg_strs.join(", "))
            }
        }
        Term::TPCon(_, con, args, _) => {
            let py_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| sanitize_py_ident(&capitalize_ident(con)));
            if args.is_empty() {
                py_con
            } else {
                let arg_strs: Vec<String> =
                    args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("{}({})", py_con, arg_strs.join(", "))
            }
        }
        Term::TElim(_motive, cases, scrut) => emit_elim(cases, scrut, env, ctx),
    }
}

fn emit_elim(cases: &[ElimCase], scrut: &Term, env: &[Name], ctx: &mut PythonModuleCtx) -> String {
    let scrut_str = emit_term(scrut, env, ctx);
    let mut arms = Vec::new();

    for case in cases {
        let is_pcon = ctx.pconstructors.contains(&case.con);
        let lc_binders: Vec<Name> = case.binders.iter().map(|b| lowercase_first(b)).collect();
        let (pat_binders, env_binders) = if is_pcon && !lc_binders.is_empty() {
            let (rest, _last) = lc_binders.split_at(lc_binders.len() - 1);
            (rest.to_vec(), rest.to_vec())
        } else {
            (lc_binders.clone(), lc_binders.clone())
        };

        let py_con = ctx
            .constructors
            .get(&case.con)
            .cloned()
            .unwrap_or_else(|| sanitize_py_ident(&capitalize_ident(&case.con)));

        let mut env2 = env_binders.clone();
        env2.reverse();
        env2.extend_from_slice(env);
        let body_str = emit_term(&case.body, &env2, ctx);

        if pat_binders.is_empty() {
            arms.push(format!("{} if _v[0] == \"{}\" else ", body_str, py_con));
        } else {
            let indices: Vec<String> = (0..pat_binders.len())
                .map(|i| format!("_v[{}]", i + 1))
                .collect();
            let binders_str = pat_binders.join(", ");
            arms.push(format!(
                "(lambda {}: {})({}) if _v[0] == \"{}\" else ",
                binders_str,
                body_str,
                indices.join(", "),
                py_con
            ));
        }
    }

    arms.push("None".to_string());
    format!("(lambda _v: {})({})", arms.concat(), scrut_str)
}

fn try_emit_let(term: &Term, env: &[Name], ctx: &mut PythonModuleCtx) -> Option<String> {
    if let Term::TApp(f, value) = term
        && let Term::TAbs(binder, body) = f.as_ref() {
            let sanitized = lowercase_first(binder);
            let mut env2 = vec![sanitized.clone()];
            env2.extend_from_slice(env);
            return Some(format!(
                "(lambda {}: {})({})",
                sanitized,
                emit_term(body, &env2, ctx),
                emit_term(value, env, ctx)
            ));
        }
    None
}

fn emit_interval(i: &I, env: &[Name]) -> String {
    match i {
        I::Var(n) => env
            .get(*n as usize)
            .cloned()
            .unwrap_or_else(|| format!("i{}", n)),
        _ => "None".to_string(),
    }
}

fn is_py_keyword(name: &str) -> bool {
    matches!(
        name,
        "False" | "None" | "True"
            | "and" | "as" | "assert" | "async" | "await"
            | "break" | "class" | "continue" | "def" | "del"
            | "elif" | "else" | "except" | "finally" | "for"
            | "from" | "global" | "if" | "import" | "in"
            | "is" | "lambda" | "nonlocal" | "not" | "or"
            | "pass" | "raise" | "return" | "try" | "while"
            | "with" | "yield"
    )
}

fn sanitize_py_ident(name: &str) -> String {
    let s = name.replace('\'', "_prime");
    if is_py_keyword(&s) {
        format!("{}_", s)
    } else {
        s
    }
}

pub fn lowercase_first(name: &str) -> String {
    let name = sanitize_py_ident(name);
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

pub fn capitalize_ident(name: &str) -> String {
    let name = sanitize_py_ident(name);
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Python module name for a `.pic` file (`Nat.pic` → `Nat`).
pub fn module_name_from_path(path: &std::path::Path) -> String {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(capitalize_ident)
        .unwrap_or_else(|| "MainLib".to_string());
    if name == "Main" {
        "MainLib".to_string()
    } else {
        name
    }
}

#[derive(Debug, Clone)]
pub struct DatatypeInfo {
    pub module_name: String,
    pub nullary_constructors: Vec<String>,
}

pub fn collect_datatype_info(decls: &[Decl], module_name: &str) -> HashMap<Name, DatatypeInfo> {
    let mut map = HashMap::new();
    for decl in decls {
        if let Decl::Data(dt) = decl {
            let nullary_constructors = dt
                .cons
                .iter()
                .filter(|c| c.arg_tys.is_empty())
                .map(|c| sanitize_py_ident(&capitalize_ident(&c.name)))
                .collect();
            map.insert(
                dt.name.clone(),
                DatatypeInfo {
                    module_name: module_name.to_string(),
                    nullary_constructors,
                },
            );
        }
    }
    map
}

/// Emit `main.py` with a `if __name__ == "__main__"` block that runs the user's `main` definition.
pub fn emit_main_driver(
    root_comment: &str,
    entry_module: &str,
    entry_name: &str,
    entry_ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
) -> String {
    let entry_mod_lower = entry_module.to_lowercase();
    let (call_expr, ret_type, extra_imports) =
        emit_entry_call(entry_module, entry_name, entry_ty, datatype_info);

    let mut imports: Vec<String> = extra_imports.iter().map(|s| s.to_lowercase()).collect();
    if entry_module != "Main" && entry_module != "MainLib" {
        imports.push(entry_mod_lower.clone());
    }
    imports.sort();
    imports.dedup();

    let mut out = String::new();
    out.push_str(&format!("# generated runner for {}\n", root_comment));
    for imp in &imports {
        out.push_str(&format!("import {}\n", imp));
    }
    if !imports.is_empty() {
        out.push('\n');
    }

    let is_nat = matches!(&ret_type, Term::TData(name) if name == "Nat");

    if is_nat {
        out.push_str("def _nat_to_int(n):\n");
        out.push_str("    if n[0] == \"Zero\":\n");
        out.push_str("        return 0\n");
        out.push_str("    if n[0] == \"Suc\":\n");
        out.push_str("        return 1 + _nat_to_int(n[1])\n");
        out.push_str("    return n\n");
        out.push('\n');
    }

    out.push_str("if __name__ == \"__main__\":\n");
    if is_nat {
        out.push_str(&format!("    print(_nat_to_int({}))\n", call_expr));
    } else {
        out.push_str(&format!("    print({})\n", call_expr));
    }
    out
}

fn emit_entry_call(
    module: &str,
    name: &str,
    ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
) -> (String, Term, Vec<String>) {
    let mut args = Vec::new();
    let mut imports = Vec::new();
    let mut cur: &Term = ty;
    while let Term::TPi(_, domain, codomain) = cur {
        let (arg, arg_imports) = demo_value(domain, datatype_info);
        args.push(arg);
        imports.extend(arg_imports);
        cur = codomain;
    }

    let mod_lower = module.to_lowercase();
    let mut expr = format!("{}.{}", mod_lower, name);
    for arg in args {
        expr = format!("{}({})", expr, arg);
    }
    imports.sort();
    imports.dedup();
    (expr, cur.clone(), imports)
}

fn demo_value(ty: &Term, datatype_info: &HashMap<Name, DatatypeInfo>) -> (String, Vec<String>) {
    if let Term::TData(name) = ty {
        if name == "Nat" {
            let mod_name = datatype_info
                .get(name)
                .map(|i| i.module_name.to_lowercase())
                .unwrap_or_else(|| "nat".to_string());
            return (
                format!("{}.Suc({}.Suc({}.Zero))", mod_name, mod_name, mod_name),
                vec![mod_name],
            );
        }
        if let Some(info) = datatype_info.get(name)
            && let Some(con) = info.nullary_constructors.first() {
                let mod_name = info.module_name.to_lowercase();
                return (format!("{}.{}", mod_name, con), vec![mod_name]);
            }
    }
    ("None".to_string(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::parser::parse_program;

    #[test]
    fn capitalizes_constructor_names() {
        assert_eq!(capitalize_ident("zero"), "Zero");
        assert_eq!(capitalize_ident("suc"), "Suc");
    }

    #[test]
    fn emits_nat_datatype() {
        let src = "data Nat = | zero : Nat | suc : Nat -> Nat\n";
        let decls = parse_program(src).unwrap();
        let out = emit_module(
            &mut PythonModuleCtx::from_decls("Nat".to_string(), &decls),
            &decls,
            "Nat.pic",
        );
        assert!(out.contains("Zero = (\"Zero\",)"));
        assert!(out.contains("Suc = lambda a0: (\"Suc\", a0)"));
    }
}
