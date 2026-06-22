//! Emit type-erased Haskell from cubical AST nodes.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::cubical::interval::{DNF, I, Literal};
use crate::cubical::parser::Decl;
use crate::cubical::syntax::{ConSig, Datatype, ElimCase, Name, PConSig, Term};

/// Context for emitting a single `.uwuc` module.
pub struct HaskellModuleCtx {
    pub module_name: String,
    pub imports: Vec<String>,
    pub constructors: HashMap<Name, Name>,
    pub datatypes: HashSet<Name>,
    pub uses_cubical: bool,
}

impl HaskellModuleCtx {
    pub fn from_decls(module_name: String, decls: &[Decl]) -> Self {
        let mut imports = Vec::new();
        let mut constructors = HashMap::new();
        let mut datatypes = HashSet::new();

        for decl in decls {
            match decl {
                Decl::Import { path } => {
                    if let Some(name) = module_name_from_uwuc_path(path) {
                        imports.push(name);
                    }
                }
                Decl::Data(dt) => {
                    register_datatype(dt, &mut constructors, &mut datatypes);
                }
                Decl::Def { .. } => {}
            }
        }

        Self {
            module_name,
            imports,
            constructors,
            datatypes,
            uses_cubical: false,
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
    prescan_cubical_usage(ctx, decls);

    let mut out = String::new();
    out.push_str(&format!("-- generated from {}\n", source_comment));
    out.push_str(&format!("module {} where\n\n", ctx.module_name));

    if ctx.uses_cubical {
        out.push_str("import Cubical.Prelude\n");
    }

    let mut import_lines: Vec<String> = ctx.imports.iter().map(|m| format!("import {}", m)).collect();
    import_lines.sort();
    import_lines.dedup();
    for line in import_lines {
        out.push_str(&line);
        out.push('\n');
    }
    if !ctx.imports.is_empty() || ctx.uses_cubical {
        out.push('\n');
    }

    for decl in decls {
        match decl {
            Decl::Import { .. } => {}
            Decl::Data(dt) => {
                out.push_str(&emit_datatype(dt, &ctx.constructors));
                out.push_str("\n\n");
            }
            Decl::Def { name, ty, val } => {
                let erased_ty = emit_type_erased(ty, &[], ctx);
                out.push_str(&format!("{} :: {}\n", name, erased_ty));
                out.push_str(&format!("{} = {}\n", name, emit_term(val, &[], ctx)));
                out.push('\n');
            }
        }
    }

    out
}

fn prescan_cubical_usage(ctx: &mut HaskellModuleCtx, decls: &[Decl]) {
    for decl in decls {
        match decl {
            Decl::Import { .. } => {}
            Decl::Data(dt) => {
                for con in &dt.cons {
                    for ty in &con.arg_tys {
                        prescan_type(ty, ctx);
                    }
                }
                for pcon in &dt.pcons {
                    for ty in &pcon.arg_tys {
                        prescan_type(ty, ctx);
                    }
                    prescan_term(&pcon.face0, ctx);
                    prescan_term(&pcon.face1, ctx);
                }
            }
            Decl::Def { ty, val, .. } => {
                prescan_type(ty, ctx);
                prescan_term(val, ctx);
            }
        }
    }
}

fn prescan_type(ty: &Term, ctx: &mut HaskellModuleCtx) {
    if is_cubical_type(ty) {
        ctx.uses_cubical = true;
    }
    walk_type(ty, ctx);
}

fn prescan_term(term: &Term, ctx: &mut HaskellModuleCtx) {
    if is_cubical_term(term) {
        ctx.uses_cubical = true;
    }
    walk_term(term, ctx);
}

fn is_cubical_type(ty: &Term) -> bool {
    matches!(
        ty,
        Term::TIntervalTy
            | Term::TPath(_, _, _)
            | Term::TEquiv(_, _)
            | Term::TGlue(_, _, _)
            | Term::TCube(_)
    )
}

fn is_cubical_term(term: &Term) -> bool {
    matches!(
        term,
        Term::TIntervalTy
            | Term::TInterval(_)
            | Term::TCube(_)
            | Term::TPath(_, _, _)
            | Term::PLam(_, _)
            | Term::PApp(_, _)
            | Term::THComp(_, _, _, _)
            | Term::TEquiv(_, _)
            | Term::TMkEquiv(_, _, _, _, _, _)
            | Term::TEquivFwd(_, _)
            | Term::TUa(_)
            | Term::TTransport(_, _)
            | Term::TGlue(_, _, _)
            | Term::TGlueElem(_, _, _)
            | Term::TUnglue(_, _, _)
            | Term::TPCon(_, _, _, _)
    )
}

fn walk_type(ty: &Term, ctx: &mut HaskellModuleCtx) {
    match ty {
        Term::TPi(_, a, b) | Term::TSigma(_, a, b) => {
            prescan_type(a, ctx);
            prescan_type(b, ctx);
        }
        Term::TPath(a, u, v) => {
            prescan_type(a, ctx);
            prescan_term(u, ctx);
            prescan_term(v, ctx);
        }
        Term::TEquiv(a, b) => {
            prescan_type(a, ctx);
            prescan_type(b, ctx);
        }
        Term::TGlue(a, phi, te) => {
            prescan_type(a, ctx);
            prescan_term(phi, ctx);
            prescan_term(te, ctx);
        }
        Term::TApp(f, a) => {
            prescan_type(f, ctx);
            prescan_term(a, ctx);
        }
        _ => {}
    }
}

fn walk_term(term: &Term, ctx: &mut HaskellModuleCtx) {
    match term {
        Term::TApp(f, a) => {
            prescan_term(f, ctx);
            prescan_term(a, ctx);
        }
        Term::TAbs(_, b) | Term::PLam(_, b) => prescan_term(b, ctx),
        Term::TPi(_, a, b) | Term::TSigma(_, a, b) => {
            prescan_type(a, ctx);
            prescan_type(b, ctx);
        }
        Term::TPath(a, u, v) => {
            prescan_type(a, ctx);
            prescan_term(u, ctx);
            prescan_term(v, ctx);
        }
        Term::PApp(p, r) => {
            prescan_term(p, ctx);
            prescan_term(r, ctx);
        }
        Term::THComp(a, phi, u, u0) => {
            prescan_type(a, ctx);
            prescan_term(phi, ctx);
            prescan_term(u, ctx);
            prescan_term(u0, ctx);
        }
        Term::TMkEquiv(a, b, f, g, eta, eps) => {
            for t in [a.as_ref(), b.as_ref(), f.as_ref(), g.as_ref(), eta.as_ref(), eps.as_ref()] {
                prescan_term(t, ctx);
            }
        }
        Term::TEquiv(a, b) => {
            prescan_type(a, ctx);
            prescan_type(b, ctx);
        }
        Term::TEquivFwd(e, x) | Term::TTransport(e, x) => {
            prescan_term(e, ctx);
            prescan_term(x, ctx);
        }
        Term::TUa(e) => prescan_term(e, ctx),
        Term::TGlue(a, phi, te) => {
            prescan_type(a, ctx);
            prescan_term(phi, ctx);
            prescan_term(te, ctx);
        }
        Term::TGlueElem(phi, t, a) => {
            prescan_term(phi, ctx);
            prescan_term(t, ctx);
            prescan_term(a, ctx);
        }
        Term::TUnglue(phi, te, g) => {
            prescan_term(phi, ctx);
            prescan_term(te, ctx);
            prescan_term(g, ctx);
        }
        Term::TPair(a, b) => {
            prescan_term(a, ctx);
            prescan_term(b, ctx);
        }
        Term::TFst(p) | Term::TSnd(p) => prescan_term(p, ctx),
        Term::TCon(_, _, args) => {
            for a in args {
                prescan_term(a, ctx);
            }
        }
        Term::TPCon(_, _, args, r) => {
            for a in args {
                prescan_term(a, ctx);
            }
            prescan_term(r, ctx);
        }
        Term::TElim(m, cases, s) => {
            prescan_term(m, ctx);
            prescan_term(s, ctx);
            for case in cases {
                prescan_term(&case.body, ctx);
            }
        }
        _ => {}
    }
}

fn register_datatype(
    dt: &Datatype,
    constructors: &mut HashMap<Name, Name>,
    datatypes: &mut HashSet<Name>,
) {
    datatypes.insert(dt.name.clone());
    for con in &dt.cons {
        constructors.insert(con.name.clone(), capitalize_ident(&con.name));
    }
    for pcon in &dt.pcons {
        constructors.insert(pcon.name.clone(), capitalize_ident(&pcon.name));
    }
}

fn emit_datatype(dt: &Datatype, constructors: &HashMap<Name, Name>) -> String {
    let mut parts = Vec::new();
    for con in &dt.cons {
        parts.push(emit_constructor_decl(con, constructors));
    }
    for pcon in &dt.pcons {
        parts.push(format!(
            "-- path constructor {} (endpoints stubbed; see Cubical.Prelude)\n  {}",
            pcon.name,
            emit_path_constructor_decl(pcon, constructors)
        ));
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
        uses_cubical: false,
    }
}

pub fn emit_type_erased(ty: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> String {
    match ty {
        Term::TUniv(_) => "Type".to_string(),
        Term::TIntervalTy => {
            ctx.uses_cubical = true;
            "I".to_string()
        }
        Term::TPi(x, a, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            let a_str = emit_type_erased(a, env, ctx);
            let b_str = emit_type_erased(b, &env2, ctx);
            if term_mentions_var(b, 0) {
                format!("{} -> {}  -- ERASED: dependent Pi", a_str, b_str)
            } else {
                format!("{} -> {}", a_str, b_str)
            }
        }
        Term::TSigma(x, a, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            let a_str = emit_type_erased(a, env, ctx);
            let b_str = emit_type_erased(b, &env2, ctx);
            if term_mentions_var(b, 0) {
                format!("({}, {})  -- ERASED: dependent Sigma", a_str, b_str)
            } else {
                format!("({}, {})", a_str, b_str)
            }
        }
        Term::TData(name) => name.clone(),
        Term::TPath(a, u, v) => {
            ctx.uses_cubical = true;
            format!(
                "Path {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_term(u, env, ctx),
                emit_term(v, env, ctx)
            )
        }
        Term::TEquiv(a, b) => {
            ctx.uses_cubical = true;
            format!(
                "Equiv {} {}",
                emit_type_erased(a, env, ctx),
                emit_type_erased(b, env, ctx)
            )
        }
        Term::TGlue(a, phi, te) => {
            ctx.uses_cubical = true;
            format!(
                "Glue {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_term(phi, env, ctx),
                emit_term(te, env, ctx)
            )
        }
        Term::TApp(f, a) => format!("{} {}", emit_type_erased(f, env, ctx), emit_term(a, env, ctx)),
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
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!("\\{} -> {}", x, emit_term(body, &env2, ctx))
        }
        Term::TApp(f, a) => format!("({} {})", emit_term(f, env, ctx), emit_term(a, env, ctx)),
        Term::TUniv(n) => format!("u{}", n),
        Term::TIntervalTy => {
            ctx.uses_cubical = true;
            "iType".to_string()
        }
        Term::TPi(x, a, b) => {
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!(
                "({} -> {})",
                emit_type_erased(a, env, ctx),
                emit_type_erased(b, &env2, ctx)
            )
        }
        Term::TInterval(i) => {
            ctx.uses_cubical = true;
            emit_interval(i, env)
        }
        Term::TCube(dnf) => {
            ctx.uses_cubical = true;
            emit_dnf(dnf)
        }
        Term::TPath(a, u, v) => {
            ctx.uses_cubical = true;
            format!(
                "path {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_term(u, env, ctx),
                emit_term(v, env, ctx)
            )
        }
        Term::PLam(x, body) => {
            ctx.uses_cubical = true;
            let mut env2 = vec![x.clone()];
            env2.extend_from_slice(env);
            format!("plam (\\{} -> {})", x, emit_term(body, &env2, ctx))
        }
        Term::PApp(p, r) => {
            ctx.uses_cubical = true;
            format!("papp {} {}", emit_term(p, env, ctx), emit_term(r, env, ctx))
        }
        Term::THComp(a, phi, u, u0) => {
            ctx.uses_cubical = true;
            format!(
                "hcomp {} {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_term(phi, env, ctx),
                emit_term(u, env, ctx),
                emit_term(u0, env, ctx)
            )
        }
        Term::TEquiv(a, b) => {
            ctx.uses_cubical = true;
            format!(
                "equivType {} {}",
                emit_type_erased(a, env, ctx),
                emit_type_erased(b, env, ctx)
            )
        }
        Term::TMkEquiv(a, b, f, g, eta, eps) => {
            ctx.uses_cubical = true;
            format!(
                "mkEquiv {} {} {} {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_type_erased(b, env, ctx),
                emit_term(f, env, ctx),
                emit_term(g, env, ctx),
                emit_term(eta, env, ctx),
                emit_term(eps, env, ctx)
            )
        }
        Term::TEquivFwd(e, x) => {
            ctx.uses_cubical = true;
            format!("equivFwd {} {}", emit_term(e, env, ctx), emit_term(x, env, ctx))
        }
        Term::TUa(e) => {
            ctx.uses_cubical = true;
            format!("ua {}", emit_term(e, env, ctx))
        }
        Term::TTransport(p, x) => {
            ctx.uses_cubical = true;
            format!("transport {} {}", emit_term(p, env, ctx), emit_term(x, env, ctx))
        }
        Term::TGlue(a, phi, te) => {
            ctx.uses_cubical = true;
            format!(
                "glueType {} {} {}",
                emit_type_erased(a, env, ctx),
                emit_term(phi, env, ctx),
                emit_term(te, env, ctx)
            )
        }
        Term::TGlueElem(phi, t, a) => {
            ctx.uses_cubical = true;
            format!(
                "glueElem {} {} {}",
                emit_term(phi, env, ctx),
                emit_term(t, env, ctx),
                emit_term(a, env, ctx)
            )
        }
        Term::TUnglue(phi, te, g) => {
            ctx.uses_cubical = true;
            format!(
                "unglue {} {} {}",
                emit_term(phi, env, ctx),
                emit_term(te, env, ctx),
                emit_term(g, env, ctx)
            )
        }
        Term::TSigma(x, a, b) => {
            let mut env2 = vec![x.clone()];
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
        Term::TPCon(_, con, args, r) => {
            ctx.uses_cubical = true;
            let hs_con = ctx
                .constructors
                .get(con)
                .cloned()
                .unwrap_or_else(|| capitalize_ident(con));
            let mut arg_strs: Vec<String> = args.iter().map(|a| emit_term(a, env, ctx)).collect();
            arg_strs.push(format!("@ {}", emit_term(r, env, ctx)));
            format!("({} {})", hs_con, arg_strs.join(" "))
        }
        Term::TElim(_motive, cases, scrut) => emit_elim(cases, scrut, env, ctx),
    }
}

fn emit_elim(cases: &[ElimCase], scrut: &Term, env: &[Name], ctx: &mut HaskellModuleCtx) -> String {
    let mut arms = Vec::new();
    for case in cases {
        let pat = emit_case_pattern(&case.con, &case.binders, ctx);
        let mut env2 = case.binders.clone();
        env2.reverse();
        env2.extend_from_slice(env);
        arms.push(format!(
            "{} -> {}",
            pat,
            emit_term(&case.body, &env2, ctx)
        ));
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
        I::I0 => "i0".to_string(),
        I::I1 => "i1".to_string(),
        I::IVar(n) => env
            .get(*n as usize)
            .cloned()
            .unwrap_or_else(|| format!("i{}", n)),
        I::Meet(a, b) => format!("({} /\\ {})", emit_interval(a, env), emit_interval(b, env)),
        I::Join(a, b) => format!("({} \\/ {})", emit_interval(a, env), emit_interval(b, env)),
        I::Neg(a) => format!("(~ {})", emit_interval(a, env)),
    }
}

fn emit_dnf(dnf: &DNF) -> String {
    if dnf.cubes.is_empty() {
        return "i0".to_string();
    }
    if dnf.cubes.len() == 1 && dnf.cubes.iter().next().unwrap().is_empty() {
        return "i1".to_string();
    }
    let parts: Vec<String> = dnf.cubes.iter().map(emit_cube).collect();
    format!("({})", parts.join(" \\/ "))
}

fn emit_cube(cube: &std::collections::BTreeSet<Literal>) -> String {
    if cube.is_empty() {
        "i1".to_string()
    } else {
        let lits: Vec<String> = cube.iter().map(|l| match l {
            Literal::Pos(n) => format!("i{}", n),
            Literal::NegVar(n) => format!("~i{}", n),
        }).collect();
        format!("({})", lits.join(" /\\ "))
    }
}

fn term_mentions_var(term: &Term, var: i32) -> bool {
    match term {
        Term::TVar(i) => *i == var,
        Term::TApp(f, a) => term_mentions_var(f, var) || term_mentions_var(a, var),
        Term::TAbs(_, b) => term_mentions_var(b, var.saturating_sub(1)),
        Term::TPi(_, a, b) => term_mentions_var(a, var) || term_mentions_var(b, var.saturating_sub(1)),
        Term::TSigma(_, a, b) => term_mentions_var(a, var) || term_mentions_var(b, var.saturating_sub(1)),
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

/// Emit `Main.hs` with `main :: IO ()` that runs the root file's last definition.
pub fn emit_main_driver(
    root_comment: &str,
    entry_module: &str,
    entry_name: &str,
    entry_ty: &Term,
    datatype_info: &HashMap<Name, DatatypeInfo>,
    uses_cubical: bool,
) -> String {
    let (call_expr, extra_imports) = emit_entry_call(entry_module, entry_name, entry_ty, datatype_info);

    let mut imports: Vec<String> = extra_imports;
    if entry_module != "Main" && entry_module != "MainLib" {
        imports.push(entry_module.to_string());
    }
    imports.sort();
    imports.dedup();

    let mut out = String::new();
    out.push_str(&format!("-- generated runner for {}\n", root_comment));
    out.push_str("module Main where\n\n");
    if uses_cubical {
        out.push_str("import Cubical.Prelude\n");
    }
    let has_imports = !imports.is_empty();
    for imp in &imports {
        out.push_str(&format!("import {}\n", imp));
    }
    if uses_cubical || has_imports {
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
                vec![datatype_info
                    .get(name)
                    .map(|i| i.module_name.clone())
                    .unwrap_or_else(|| "Nat".to_string())],
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
        let out = emit_module(&mut HaskellModuleCtx::from_decls("Nat".to_string(), &decls), &decls, "Nat.uwuc");
        assert!(out.contains("data Nat = Zero"));
        assert!(out.contains("Suc Nat"));
        let _ = ctx;
    }
}
