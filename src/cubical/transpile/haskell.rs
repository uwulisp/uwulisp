//! Emit type-erased Haskell from cubical AST nodes.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::cubical::interval::I;
use crate::cubical::parser::Decl;
use crate::cubical::syntax::{ConSig, Datatype, ElimCase, Name, PConSig, Term};

/// Context for emitting a single `.uwuc` module.
pub struct HaskellModuleCtx {
    pub module_name: String,
    pub imports: Vec<String>,
    pub constructors: HashMap<Name, Name>,
    pub datatypes: HashSet<Name>,
    pub pconstructors: HashSet<Name>,
}

impl HaskellModuleCtx {
    pub fn from_decls(module_name: String, decls: &[Decl]) -> Self {
        let mut imports = Vec::new();
        let mut constructors = HashMap::new();
        let mut datatypes = HashSet::new();
        let mut pconstructors = HashSet::new();

        for decl in decls {
            match decl {
                Decl::Import { path } => {
                    if let Some(name) = module_name_from_uwuc_path(path) {
                        imports.push(name);
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

pub fn hs_path_for_module(module_name: &str) -> PathBuf {
    PathBuf::from(format!("{}.hs", module_name))
}

pub fn hs_path_from_uwuc_path(path: &PathBuf) -> PathBuf {
    hs_path_for_module(&module_name_from_path(path))
}

pub fn emit_module(ctx: &mut HaskellModuleCtx, decls: &[Decl], source_comment: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("-- generated from {}\n", source_comment));
    out.push_str(&format!("module {} where\n\n", ctx.module_name));

    out.push_str("import Data.Kind (Type)\n");
    out.push_str("import Unsafe.Coerce (unsafeCoerce)\n");

    let mut import_lines: Vec<String> = ctx
        .imports
        .iter()
        .map(|m| format!("import {}", m))
        .collect();
    import_lines.sort();
    import_lines.dedup();
    for line in import_lines {
        out.push_str(&line);
        out.push('\n');
    }
    if !ctx.imports.is_empty() {
        out.push('\n');
    } else {
        out.push('\n');
    }

    let mut name_env: Vec<Name> = Vec::new();

    for decl in decls {
        match decl {
            Decl::Import { .. } => {}
            Decl::Data(dt) => {
                out.push_str(&emit_datatype(dt, &ctx.constructors));
                out.push_str("\n\n");
            }
            Decl::Def { name, ty, val } => {
                name_env.insert(0, name.clone());
                let erased_ty = emit_type_erased(ty, &name_env, ctx);
                out.push_str(&format!("{} :: {}\n", name, erased_ty));
                out.push_str(&format!("{} = {}\n", name, emit_term(val, &name_env, ctx)));
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
        constructors.insert(con.name.clone(), capitalize_ident(&con.name));
    }
    for pcon in &dt.pcons {
        constructors.insert(pcon.name.clone(), capitalize_ident(&pcon.name));
        pconstructors.insert(pcon.name.clone());
    }
}

fn emit_datatype(dt: &Datatype, constructors: &HashMap<Name, Name>) -> String {
    let mut parts = Vec::new();
    for con in &dt.cons {
        parts.push(emit_constructor_decl(con, constructors));
    }
    for pcon in &dt.pcons {
        parts.push(emit_path_constructor_decl(pcon, constructors));
    }
    format!("data {} = {} deriving Show", dt.name, parts.join("\n  | "))
}

fn emit_constructor_decl(con: &ConSig, constructors: &HashMap<Name, Name>) -> String {
    let hs_name = constructors
        .get(&con.name)
        .cloned()
        .unwrap_or_else(|| capitalize_ident(&con.name));
    if con.arg_tys.is_empty() {
        hs_name
    } else {
        let args: Vec<String> = con
            .arg_tys
            .iter()
            .map(|ty| emit_type_erased(ty, &[], &mut bare_ctx()))
            .collect();
        format!("{} {}", hs_name, args.join(" "))
    }
}

fn emit_path_constructor_decl(pcon: &PConSig, constructors: &HashMap<Name, Name>) -> String {
    let hs_name = constructors
        .get(&pcon.name)
        .cloned()
        .unwrap_or_else(|| capitalize_ident(&pcon.name));
    if pcon.arg_tys.is_empty() {
        hs_name
    } else {
        let args: Vec<String> = pcon
            .arg_tys
            .iter()
            .map(|ty| emit_type_erased(ty, &[], &mut bare_ctx()))
            .collect();
        format!("{} {}", hs_name, args.join(" "))
    }
}

fn bare_ctx() -> HaskellModuleCtx {
    HaskellModuleCtx {
        module_name: String::new(),
        imports: Vec::new(),
        constructors: HashMap::new(),
        datatypes: HashSet::new(),
        pconstructors: HashSet::new(),
    }
}

pub fn emit_type_erased(ty: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> String {
    match ty {
        Term::TUniv(_) => "Type".to_string(),
        Term::TIntervalTy => "()".to_string(),
        Term::TPi(x, a, b) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            let a_str = emit_type_erased(a, env, ctx);
            let a_str = if is_function_type(a) {
                format!("({})", a_str)
            } else {
                a_str
            };
            let b_str = emit_type_erased(b, &env2, ctx);
            if term_mentions_var(b, 0) {
                format!("{} -> {}  -- ERASED: dependent Pi", a_str, b_str)
            } else {
                format!("{} -> {}", a_str, b_str)
            }
        }
        Term::TSigma(x, a, b) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            let a_str = emit_type_erased(a, env, ctx);
            let a_str = if is_function_type(a) {
                format!("({})", a_str)
            } else {
                a_str
            };
            let b_str = emit_type_erased(b, &env2, ctx);
            if term_mentions_var(b, 0) {
                format!("({}, {})  -- ERASED: dependent Sigma", a_str, b_str)
            } else {
                format!("({}, {})", a_str, b_str)
            }
        }
        Term::TData(name) => name.clone(),
        Term::TPath(a, _, _) => emit_type_erased(a, env, ctx),
        Term::TEquiv(a, b) => {
            let a_str = emit_type_erased(a, env, ctx);
            let a_str = if is_function_type(a) {
                format!("({})", a_str)
            } else {
                a_str
            };
            format!("{} -> {}", a_str, emit_type_erased(b, env, ctx))
        },
        Term::TGlue(a, _, _) => emit_type_erased(a, env, ctx),
        Term::TApp(f, a) => format!(
            "{} {}",
            emit_type_erased(f, env, ctx),
            emit_term(a, env, ctx)
        ),
        Term::TVar(i) => env
            .get(*i as usize)
            .cloned()
            .unwrap_or_else(|| format!("t{}", i)),
        other => format!("({})", emit_term(other, env, ctx)),
    }
}

pub fn emit_term(term: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> String {
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
            format!("\\{} -> {}", lc, emit_term(body, &env2, ctx))
        }
        Term::TApp(f, a) => format!("({} {})", emit_term(f, env, ctx), emit_term(a, env, ctx)),
        Term::TUniv(n) => format!("u{}", n),
        Term::TIntervalTy => "()".to_string(),
        Term::TPi(x, a, b) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            let a_str = emit_type_erased(a, env, ctx);
            let a_str = if is_function_type(a) {
                format!("({})", a_str)
            } else {
                a_str
            };
            format!(
                "({} -> {})",
                a_str,
                emit_type_erased(b, &env2, ctx)
            )
        }
        Term::TInterval(i) => emit_interval(i, env),
        Term::TCube(_) => "()".to_string(),
        Term::TPath(a, _, _) => emit_type_erased(a, env, ctx),
        Term::PLam(x, body) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            emit_term(body, &env2, ctx)
        }
        Term::PApp(p, _) => emit_term(p, env, ctx),
        Term::THComp(_, _, _, u0) => emit_term(u0, env, ctx),
        Term::TEquiv(a, b) => {
            let a_str = emit_type_erased(a, env, ctx);
            let a_str = if is_function_type(a) {
                format!("({})", a_str)
            } else {
                a_str
            };
            format!("{} -> {}", a_str, emit_type_erased(b, env, ctx))
        },
        Term::TMkEquiv(_, _, f, _, _, _) => emit_term(f, env, ctx),
        Term::TEquivFwd(e, x) => format!("({} {})", emit_term(e, env, ctx), emit_term(x, env, ctx)),
        Term::TUa(_) => "undefined".to_string(),
        Term::TTransport(_, x) => format!("unsafeCoerce ({})", emit_term(x, env, ctx)),
        Term::TGlue(a, _, _) => emit_type_erased(a, env, ctx),
        Term::TGlueElem(_, t, _) => format!("unsafeCoerce ({})", emit_term(t, env, ctx)),
        Term::TUnglue(_, _, g) => emit_term(g, env, ctx),
        Term::TSigma(x, a, b) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            format!(
                "({}, {})",
                emit_type_erased(a, env, ctx),
                emit_type_erased(b, &env2, ctx)
            )
        }
        Term::TPair(a, b) => format!("({}, {})", emit_term(a, env, ctx), emit_term(b, env, ctx)),
        Term::TFst(p) => format!("fst {}", emit_term(p, env, ctx)),
        Term::TSnd(p) => format!("snd {}", emit_term(p, env, ctx)),
        Term::TData(name) => name.clone(),
        Term::TCon(_, con, args) => {
            let hs_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| capitalize_ident(con));
            if args.is_empty() {
                hs_con
            } else {
                let arg_strs: Vec<String> = args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("({} {})", hs_con, arg_strs.join(" "))
            }
        }
        Term::TPCon(_, con, args, _) => {
            let hs_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| capitalize_ident(con));
            if args.is_empty() {
                hs_con
            } else {
                let arg_strs: Vec<String> = args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("({} {})", hs_con, arg_strs.join(" "))
            }
        }
        Term::TElim(_motive, cases, scrut) => emit_elim(cases, scrut, env, ctx),
    }
}

fn emit_elim(cases: &[ElimCase], scrut: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> String {
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
        let pat = emit_case_pattern(&case.con, &pat_binders, ctx);
        let mut env2 = env_binders;
        env2.reverse();
        env2.extend_from_slice(env);
        arms.push(format!("{} -> {}", pat, emit_term(&case.body, &env2, ctx)));
    }
    format!(
        "(case {} of\n  {})",
        emit_term(scrut, env, ctx),
        arms.join("\n  ")
    )
}

fn emit_case_pattern(con: &Name, binders: &[Name], ctx: &HaskellModuleCtx) -> String {
    let hs_con = ctx
        .constructors
        .get(con)
        .cloned()
        .unwrap_or_else(|| capitalize_ident(con));
    if binders.is_empty() {
        hs_con
    } else {
        format!("{} {}", hs_con, binders.join(" "))
    }
}

fn try_emit_let(term: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> Option<String> {
    if let Term::TApp(f, value) = term {
        if let Term::TAbs(binder, body) = f.as_ref() {
            let mut env2 = vec![binder.clone()];
            env2.extend_from_slice(env);
            return Some(format!(
                "(let {} = {} in {})",
                binder,
                emit_term(value, env, ctx),
                emit_term(body, &env2, ctx)
            ));
        }
    }
    None
}

fn emit_interval(i: &I, env: &[Name]) -> String {
    match i {
        I::IVar(n) => env
            .get(*n as usize)
            .cloned()
            .unwrap_or_else(|| format!("i{}", n)),
        _ => "()".to_string(),
    }
}

fn term_mentions_var(term: &Term, var: i32) -> bool {
    match term {
        Term::TVar(i) => *i == var,
        Term::TApp(f, a) => term_mentions_var(f, var) || term_mentions_var(a, var),
        Term::TAbs(_, b) => term_mentions_var(b, var.saturating_sub(1)),
        Term::TPi(_, a, b) => {
            term_mentions_var(a, var) || term_mentions_var(b, var.saturating_sub(1))
        }
        Term::TSigma(_, a, b) => {
            term_mentions_var(a, var) || term_mentions_var(b, var.saturating_sub(1))
        }
        Term::TPath(a, u, v) => {
            term_mentions_var(a, var) || term_mentions_var(u, var) || term_mentions_var(v, var)
        }
        Term::PLam(_, b) => term_mentions_var(b, var.saturating_sub(1)),
        Term::PApp(p, r) => term_mentions_var(p, var) || term_mentions_var(r, var),
        Term::THComp(a, phi, u, u0) => {
            term_mentions_var(a, var)
                || term_mentions_var(phi, var)
                || term_mentions_var(u, var)
                || term_mentions_var(u0, var)
        }
        Term::TEquiv(a, b) => term_mentions_var(a, var) || term_mentions_var(b, var),
        Term::TMkEquiv(a, b, f, g, eta, eps) => {
            term_mentions_var(a, var)
                || term_mentions_var(b, var)
                || term_mentions_var(f, var)
                || term_mentions_var(g, var)
                || term_mentions_var(eta, var)
                || term_mentions_var(eps, var)
        }
        Term::TEquivFwd(e, x) => term_mentions_var(e, var) || term_mentions_var(x, var),
        Term::TUa(e) => term_mentions_var(e, var),
        Term::TTransport(p, x) => term_mentions_var(p, var) || term_mentions_var(x, var),
        Term::TGlue(a, phi, te) => {
            term_mentions_var(a, var) || term_mentions_var(phi, var) || term_mentions_var(te, var)
        }
        Term::TGlueElem(phi, t, a) => {
            term_mentions_var(phi, var) || term_mentions_var(t, var) || term_mentions_var(a, var)
        }
        Term::TUnglue(phi, te, g) => {
            term_mentions_var(phi, var) || term_mentions_var(te, var) || term_mentions_var(g, var)
        }
        Term::TPair(a, b) => term_mentions_var(a, var) || term_mentions_var(b, var),
        Term::TFst(p) | Term::TSnd(p) => term_mentions_var(p, var),
        Term::TCon(_, _, args) => args.iter().any(|a| term_mentions_var(a, var)),
        Term::TPCon(_, _, args, r) => {
            args.iter().any(|a| term_mentions_var(a, var)) || term_mentions_var(r, var)
        }
        Term::TElim(m, cases, s) => {
            term_mentions_var(m, var)
                || term_mentions_var(s, var)
                || cases.iter().any(|c| term_mentions_var(&c.body, var))
        }
        Term::TUniv(_)
        | Term::TIntervalTy
        | Term::TInterval(_)
        | Term::TCube(_)
        | Term::TData(_) => false,
    }
}

pub fn capitalize_ident(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn is_function_type(ty: &Term) -> bool {
    matches!(ty, Term::TPi(..) | Term::TSigma(..) | Term::TEquiv(..))
}

fn lowercase_first(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

/// Haskell module name for a `.uwuc` file (`Nat.uwuc` → `Nat`).
/// Avoids clashing with the generated executable driver module `Main`.
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

/// Metadata for generating demo values at the executable entry point.
#[derive(Debug, Clone)]
pub struct DatatypeInfo {
    pub module_name: String,
    pub nullary_constructors: Vec<String>,
}

/// Build a map from datatype name to the module that defines it.
pub fn collect_datatype_info(decls: &[Decl], module_name: &str) -> HashMap<Name, DatatypeInfo> {
    let mut map = HashMap::new();
    for decl in decls {
        if let Decl::Data(dt) = decl {
            let nullary_constructors = dt
                .cons
                .iter()
                .filter(|c| c.arg_tys.is_empty())
                .map(|c| capitalize_ident(&c.name))
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

/// Emit `Main.hs` with `main :: IO ()` that runs the user's `main` definition.
pub fn emit_main_driver(
    root_comment: &str,
    entry_module: &str,
    entry_name: &str,
    entry_ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
) -> String {
    let (call_expr, extra_imports) =
        emit_entry_call(entry_module, entry_name, entry_ty, datatype_info);

    let mut imports: Vec<String> = extra_imports;
    if entry_module != "Main" && entry_module != "MainLib" {
        imports.push(entry_module.to_string());
    }
    imports.sort();
    imports.dedup();

    let mut out = String::new();
    out.push_str(&format!("-- generated runner for {}\n", root_comment));
    out.push_str("module Main where\n\n");
    out.push_str("import Data.Kind (Type)\n");
    for imp in &imports {
        out.push_str(&format!("import {}\n", imp));
    }
    if !imports.is_empty() {
        out.push('\n');
    } else {
        out.push('\n');
    }
    out.push_str("main :: IO ()\n");
    out.push_str(&format!("main = print ({})\n", call_expr));
    out
}

fn emit_entry_call(
    module: &str,
    name: &str,
    ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
) -> (String, Vec<String>) {
    let mut args = Vec::new();
    let mut imports = Vec::new();
    let mut cur = ty;
    while let Term::TPi(_, domain, codomain) = cur {
        let (arg, arg_imports) = demo_value(domain, datatype_info);
        args.push(arg);
        imports.extend(arg_imports);
        cur = codomain;
    }

    let mut expr = format!("{}.{}", module, name);
    for arg in args {
        expr = format!("({} {})", expr, arg);
    }
    imports.sort();
    imports.dedup();
    (expr, imports)
}

fn demo_value(ty: &Term, datatype_info: &HashMap<Name, DatatypeInfo>) -> (String, Vec<String>) {
    if let Term::TData(name) = ty {
        if name == "Nat" {
            return (
                "(Suc (Suc Zero))".to_string(),
                vec![
                    datatype_info
                        .get(name)
                        .map(|i| i.module_name.clone())
                        .unwrap_or_else(|| "Nat".to_string()),
                ],
            );
        }
        if let Some(info) = datatype_info.get(name) {
            if let Some(con) = info.nullary_constructors.first() {
                return (con.clone(), vec![info.module_name.clone()]);
            }
        }
    }
    ("undefined".to_string(), Vec::new())
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
        let ctx = HaskellModuleCtx::from_decls("Nat".to_string(), &decls);
        let out = emit_module(
            &mut HaskellModuleCtx::from_decls("Nat".to_string(), &decls),
            &decls,
            "Nat.uwuc",
        );
        assert!(out.contains("data Nat = Zero"));
        assert!(out.contains("Suc Nat"));
        let _ = ctx;
    }
}
