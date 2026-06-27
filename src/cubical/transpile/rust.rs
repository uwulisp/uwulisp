use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cubical::interval::I;
use crate::cubical::parser::Decl;
use crate::cubical::syntax::{ConSig, Datatype, ElimCase, Name, PConSig, Term};

pub struct RustModuleCtx {
    #[allow(dead_code)]
    pub module_name: String,
    pub imports: Vec<String>,
    pub constructors: HashMap<Name, Name>,
    #[allow(dead_code)]
    pub datatypes: HashSet<Name>,
    pub pconstructors: HashSet<Name>,
    pub def_names: HashSet<Name>,
}

impl RustModuleCtx {
    pub fn from_decls(module_name: String, decls: &[Decl]) -> Self {
        let mut imports = Vec::new();
        let mut constructors = HashMap::new();
        let mut datatypes = HashSet::new();
        let mut pconstructors = HashSet::new();
        let mut def_names = HashSet::new();

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
                Decl::Def { name, .. } => {
                    def_names.insert(sanitize_rust_ident(name));
                }
            }
        }

        Self {
            module_name,
            imports,
            constructors,
            datatypes,
            pconstructors,
            def_names,
        }
    }
}

pub const PRELUDE: &str = r#"pub use std::rc::Rc;

#[derive(Clone)]
pub enum Val {
    Con(&'static str, Vec<Val>),
    Fun(Rc<dyn Fn(Val) -> Val>),
}

pub fn con(name: &'static str, args: Vec<Val>) -> Val {
    Val::Con(name, args)
}

pub fn fun(f: impl Fn(Val) -> Val + 'static) -> Val {
    Val::Fun(Rc::new(f))
}

pub fn app(f: Val, x: Val) -> Val {
    match f {
        Val::Fun(g) => g(x),
        _ => panic!("not a function"),
    }
}

impl std::fmt::Debug for Val {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Val::Con(name, args) => {
                if args.is_empty() {
                    write!(f, "{}", name)
                } else {
                    write!(f, "{}(", name)?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{:?}", arg)?;
                    }
                    write!(f, ")")
                }
            }
            Val::Fun(_) => write!(f, "<fun>"),
        }
    }
}
"#;

pub fn emit_prelude_file() -> String {
    PRELUDE.to_string()
}

pub fn emit_module(ctx: &mut RustModuleCtx, decls: &[Decl], source_comment: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("// generated from {}\n", source_comment));
    out.push_str("#![allow(unused_variables, non_snake_case, unused_imports, dead_code)]\n\n");

    out.push_str("use crate::prelude::*;\n");
    for imp in &ctx.imports {
        out.push_str(&format!("use crate::{}::*;\n", imp));
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
                let rs_name = sanitize_rust_ident(name);
                name_env.insert(0, rs_name.clone());
                out.push_str(&format!(
                    "pub fn {}() -> Val {{\n    {}\n}}\n",
                    rs_name,
                    emit_term(val, &name_env, ctx)
                ));
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
        constructors.insert(
            con.name.clone(),
            sanitize_rust_ident(&capitalize_ident(&con.name)),
        );
    }
    for pcon in &dt.pcons {
        constructors.insert(
            pcon.name.clone(),
            sanitize_rust_ident(&capitalize_ident(&pcon.name)),
        );
        pconstructors.insert(pcon.name.clone());
    }
}

fn emit_datatype(dt: &Datatype, constructors: &HashMap<Name, Name>) -> String {
    let mut lines = Vec::new();
    let dt_name = sanitize_rust_ident(&dt.name);
    lines.push(format!("// data {}", dt_name));
    for con in &dt.cons {
        lines.push(emit_constructor_def(con, constructors));
    }
    for pcon in &dt.pcons {
        lines.push(emit_path_constructor_def(pcon, constructors));
    }
    lines.join("\n")
}

fn emit_constructor_def(con: &ConSig, constructors: &HashMap<Name, Name>) -> String {
    let rs_name = constructors
        .get(&con.name)
        .cloned()
        .unwrap_or_else(|| sanitize_rust_ident(&capitalize_ident(&con.name)));
    if con.arg_tys.is_empty() {
        format!(
            "pub fn {}() -> Val {{ con(\"{}\", vec![]) }}",
            rs_name, rs_name
        )
    } else {
        let args: Vec<String> = (0..con.arg_tys.len())
            .map(|i| format!("a{}", i))
            .collect();
        format!(
            "pub fn {}({}) -> Val {{ con(\"{}\", vec![{}]) }}",
            rs_name,
            args.iter()
                .map(|a| format!("{}: Val", a))
                .collect::<Vec<_>>()
                .join(", "),
            rs_name,
            args.join(", ")
        )
    }
}

fn emit_path_constructor_def(pcon: &PConSig, constructors: &HashMap<Name, Name>) -> String {
    let rs_name = constructors
        .get(&pcon.name)
        .cloned()
        .unwrap_or_else(|| sanitize_rust_ident(&capitalize_ident(&pcon.name)));
    if pcon.arg_tys.is_empty() {
        format!(
            "pub fn {}() -> Val {{ con(\"{}\", vec![]) }}",
            rs_name, rs_name
        )
    } else {
        let args: Vec<String> = (0..pcon.arg_tys.len())
            .map(|i| format!("a{}", i))
            .collect();
        format!(
            "pub fn {}({}) -> Val {{ con(\"{}\", vec![{}]) }}",
            rs_name,
            args.iter()
                .map(|a| format!("{}: Val", a))
                .collect::<Vec<_>>()
                .join(", "),
            rs_name,
            args.join(", ")
        )
    }
}

pub fn emit_term(term: &Term, env: &[Name], ctx: &mut RustModuleCtx) -> String {
    if let Some(let_expr) = try_emit_let(term, env, ctx) {
        return let_expr;
    }

    match term {
        Term::TVar(i) => {
            let name = env
                .get(*i as usize)
                .cloned()
                .unwrap_or_else(|| format!("v{}", i));
            if ctx.def_names.contains(&name) {
                format!("{}()", name)
            } else {
                format!("{}.clone()", name)
            }
        }
        Term::TAbs(x, body) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc.clone()];
            env2.extend_from_slice(env);
            format!(
                "fun(move |{}| {{ {} }})",
                lc,
                emit_term(body, &env2, ctx)
            )
        }
        Term::TApp(f, a) => format!(
            "app({}, {})",
            emit_term(f, env, ctx),
            emit_term(a, env, ctx)
        ),
        Term::TUniv(_) => "con(\"U\", vec![])".to_string(),
        Term::TIntervalTy => "con(\"I\", vec![])".to_string(),
        Term::TPi(_, a, b) => {
            let _ = emit_term(a, env, ctx);
            format!(
                "fun(move |_| {{ {} }})",
                emit_term(b, env, ctx)
            )
        }
        Term::TInterval(i) => emit_interval(i, env),
        Term::TCube(_) => "con(\"cube\", vec![])".to_string(),
        Term::TPath(a, _, _) => emit_term(a, env, ctx),
        Term::PLam(x, body) => {
            let lc = lowercase_first(x);
            let mut env2 = vec![lc];
            env2.extend_from_slice(env);
            emit_term(body, &env2, ctx)
        }
        Term::PApp(p, _) => emit_term(p, env, ctx),
        Term::THComp(_, _, _, u0) => emit_term(u0, env, ctx),
        Term::TEquiv(_, _) => "fun(|x| x)".to_string(),
        Term::TMkEquiv(_, _, f, _, _, _) => emit_term(f, env, ctx),
        Term::TEquivFwd(e, x) => format!(
            "app({}, {})",
            emit_term(e, env, ctx),
            emit_term(x, env, ctx)
        ),
        Term::TUa(_) => "fun(|x| x)".to_string(),
        Term::TTransport(_, x) => emit_term(x, env, ctx),
        Term::TGlue(a, _, _) => emit_term(a, env, ctx),
        Term::TGlueElem(_, t, _) => emit_term(t, env, ctx),
        Term::TUnglue(_, _, g) => emit_term(g, env, ctx),
        Term::TSigma(_, a, b) => {
            format!(
                "con(\",\", vec![{}, {}])",
                emit_term(a, env, ctx),
                emit_term(b, env, ctx)
            )
        }
        Term::TPair(a, b) => format!(
            "con(\",\", vec![{}, {}])",
            emit_term(a, env, ctx),
            emit_term(b, env, ctx)
        ),
        Term::TFst(p) => format!(
            "match {} {{ Val::Con(_, args) => args[0].clone(), _ => panic!(\"not a pair\") }}",
            emit_term(p, env, ctx)
        ),
        Term::TSnd(p) => format!(
            "match {} {{ Val::Con(_, args) => args[1].clone(), _ => panic!(\"not a pair\") }}",
            emit_term(p, env, ctx)
        ),
        Term::TData(name) => name.clone(),
        Term::TCon(_, con, args) => {
            let rs_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| sanitize_rust_ident(&capitalize_ident(con)));
            if args.is_empty() {
                format!("{}()", rs_con)
            } else {
                let arg_strs: Vec<String> =
                    args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("{}({})", rs_con, arg_strs.join(", "))
            }
        }
        Term::TPCon(_, con, args, _) => {
            let rs_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| sanitize_rust_ident(&capitalize_ident(con)));
            if args.is_empty() {
                format!("{}()", rs_con)
            } else {
                let arg_strs: Vec<String> =
                    args.iter().map(|a| emit_term(a, env, ctx)).collect();
                format!("{}({})", rs_con, arg_strs.join(", "))
            }
        }
        Term::TElim(_motive, cases, scrut) => emit_elim(cases, scrut, env, ctx),
    }
}

fn emit_elim(cases: &[ElimCase], scrut: &Term, env: &[Name], ctx: &mut RustModuleCtx) -> String {
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

        let rs_con = ctx
            .constructors
            .get(&case.con)
            .cloned()
            .unwrap_or_else(|| sanitize_rust_ident(&capitalize_ident(&case.con)));

        let mut env2 = env_binders.clone();
        env2.reverse();
        env2.extend_from_slice(env);
        let body_str = emit_term(&case.body, &env2, ctx);

        if pat_binders.is_empty() {
            arms.push(format!("\"{}\" => {{ {} }},", rs_con, body_str));
        } else {
            let let_bindings: Vec<String> = pat_binders
                .iter()
                .enumerate()
                .map(|(i, b)| format!("let {} = args[{}].clone();", b, i))
                .collect();
            arms.push(format!(
                "\"{}\" => {{\n{}\n{}}},",
                rs_con,
                let_bindings.join("\n"),
                body_str
            ));
        }
    }

    format!(
        "{{\nlet _v = {};\nmatch _v {{\nVal::Con(name, args) => match name {{\n{}\n_ => panic!(\"unexpected constructor\"),\n}},\n_ => panic!(\"not a constructor\"),\n}}\n}}",
        scrut_str,
        arms.join("\n")
    )
}

fn try_emit_let(term: &Term, env: &[Name], ctx: &mut RustModuleCtx) -> Option<String> {
    if let Term::TApp(f, value) = term
        && let Term::TAbs(binder, body) = f.as_ref()
    {
        let sanitized = lowercase_first(binder);
        let mut env2 = vec![sanitized.clone()];
        env2.extend_from_slice(env);
        return Some(format!(
            "{{ let {} = {}; {} }}",
            sanitized,
            emit_term(value, env, ctx),
            emit_term(body, &env2, ctx)
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
        _ => "con(\"i\", vec![])".to_string(),
    }
}

fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break" | "const" | "continue" | "crate" | "else"
            | "enum" | "extern" | "false" | "fn" | "for" | "if"
            | "impl" | "in" | "let" | "loop" | "match" | "mod"
            | "move" | "mut" | "pub" | "ref" | "return" | "self"
            | "Self" | "static" | "struct" | "super" | "trait"
            | "true" | "type" | "union" | "unsafe" | "use" | "where"
            | "while" | "async" | "await" | "dyn" | "abstract"
            | "become" | "box" | "do" | "final" | "macro" | "override"
            | "priv" | "try" | "typeof" | "unsized" | "virtual"
            | "yield"
    )
}

fn sanitize_rust_ident(name: &str) -> String {
    let s = name.replace('\'', "_prime");
    if is_rust_keyword(&s) {
        format!("{}_", s)
    } else {
        s
    }
}

pub fn lowercase_first(name: &str) -> String {
    let name = sanitize_rust_ident(name);
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
    }
}

pub fn capitalize_ident(name: &str) -> String {
    let name = sanitize_rust_ident(name);
    let mut chars = name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn module_name_from_uwuc_path(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())?;
    Some(capitalize_ident(stem))
}

pub fn rs_path_for_module(module_name: &str) -> PathBuf {
    PathBuf::from(format!("{}.rs", module_name.to_lowercase()))
}

pub fn rs_path_from_uwuc_path(path: &Path) -> PathBuf {
    rs_path_for_module(&module_name_from_path(path))
}

/// Module name for a `.pic` file path.
pub fn module_name_from_path(path: &Path) -> String {
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
                .map(|c| sanitize_rust_ident(&capitalize_ident(&c.name)))
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

/// Emit `main.rs` with `mod` declarations and a `main()` entry point.
pub fn emit_main_driver(
    root_comment: &str,
    entry_module: &str,
    entry_name: &str,
    entry_ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
) -> String {
    let entry_mod_lower = entry_module.to_lowercase();
    let (call_expr, _ret_type, extra_imports) =
        emit_entry_call(entry_module, entry_name, entry_ty, datatype_info);

    let mut modules: Vec<String> = extra_imports.iter().map(|s| s.to_lowercase()).collect();
    if entry_module != "Main" && entry_module != "MainLib" {
        modules.push(entry_mod_lower.clone());
    }
    modules.sort();
    modules.dedup();

    let mut out = String::new();
    out.push_str(&format!("// generated runner for {}\n", root_comment));
    out.push_str("#![allow(dead_code, unused_imports, non_snake_case)]\n\n");
    out.push_str("mod prelude;\n");
    for m in &modules {
        out.push_str(&format!("mod {};\n", m));
    }
    out.push('\n');
    // Only import prelude items - the Debug impl for Val is found through the
    // return type, so this is only needed for `app` when the entry is a function.
    if call_expr.contains("app(") {
        out.push_str("use prelude::*;\n");
    }
    out.push('\n');

    out.push_str("fn main() {\n");
    out.push_str(&format!("    println!(\"{{:?}}\", {});\n", call_expr));
    out.push_str("}\n");
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
    let mut expr = format!("{}::{}()", mod_lower, name);
    for arg in args {
        expr = format!("app({}, {})", expr, arg);
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
                format!(
                    "{}::Suc({}::Suc({}::Zero()))",
                    mod_name, mod_name, mod_name
                ),
                vec![mod_name],
            );
        }
        if let Some(info) = datatype_info.get(name)
            && let Some(con) = info.nullary_constructors.first()
        {
            let mod_name = info.module_name.to_lowercase();
            return (format!("{}::{}()", mod_name, con), vec![mod_name]);
        }
    }
    ("con(\"None\", vec![])".to_string(), Vec::new())
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
            &mut RustModuleCtx::from_decls("Nat".to_string(), &decls),
            &decls,
            "Nat.pic",
        );
        assert!(out.contains("pub fn Zero()"));
        assert!(out.contains("pub fn Suc(a0: Val)"));
    }

    #[test]
    fn emits_def_function() {
        let src = "data Nat = | zero : Nat | suc : Nat -> Nat\n\
                   def id : Nat -> Nat = \\n. n\n";
        let decls = parse_program(src).unwrap();
        let mut ctx = RustModuleCtx::from_decls("Nat".to_string(), &decls);
        let out = emit_module(&mut ctx, &decls, "Nat.pic");
        assert!(out.contains("pub fn id()"));
    }

    #[test]
    fn prelude_is_valid_rust() {
        let prelude = emit_prelude_file();
        assert!(prelude.contains("pub enum Val"));
        assert!(prelude.contains("pub fn con"));
        assert!(prelude.contains("pub fn fun"));
        assert!(prelude.contains("pub fn app"));
    }

    #[test]
    fn driver_has_mod_main_and_prelude() {
        let src = "data Nat = | zero : Nat | suc : Nat -> Nat\n\
                   def main : Nat = zero\n";
        let decls = parse_program(src).unwrap();
        let info = collect_datatype_info(&decls, "Nat");
        let driver = emit_main_driver(
            "test.pic",
            "Nat",
            "main",
            &Term::TData("Nat".into()),
            &info,
        );
        assert!(driver.contains("mod prelude;"));
        assert!(driver.contains("mod nat;"));
        assert!(driver.contains("fn main()"));
        assert!(driver.contains("println!"));
    }
}
