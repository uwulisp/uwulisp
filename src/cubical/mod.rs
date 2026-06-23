pub mod env;
pub mod equality;
pub mod eval;
pub mod interval;
pub mod nbe;
pub mod parser;
pub mod syntax;
pub mod transpile;
pub mod typechecker;

pub use transpile::{
    EmittedModule, TranspileError, TranspileOutput, transpile, transpile_source, write_output,
};

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use self::env::{Env, apply_globals, check_with_full_env, infer_with_full_env};
use self::nbe::nbe_eval;
use self::parser::{Decl, ParseError, ProgramParser};
use self::syntax::{Name, Term};
use self::typechecker::{TypeError, check_closed_dt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    pub name: Name,
    pub ty: Term,
    pub value: Term,
}

#[derive(Debug)]
pub enum RunError {
    Io(std::io::Error),
    Parse(ParseError),
    Type(TypeError),
    Import(String),
    NoEntryPoint,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Io(err) => write!(f, "I/O error: {}", err),
            RunError::Parse(err) => write!(f, "parse error: {}", err),
            RunError::Type(err) => write!(f, "type error:\n{}", err),
            RunError::Import(msg) => write!(f, "import error: {}", msg),
            RunError::NoEntryPoint => write!(f, "program has no definition to run"),
        }
    }
}

impl std::error::Error for RunError {}

impl From<std::io::Error> for RunError {
    fn from(err: std::io::Error) -> Self {
        RunError::Io(err)
    }
}

impl From<ParseError> for RunError {
    fn from(err: ParseError) -> Self {
        RunError::Parse(err)
    }
}

impl From<TypeError> for RunError {
    fn from(err: TypeError) -> Self {
        RunError::Type(err)
    }
}

/// Read, typecheck, and evaluate a cubical source file.
///
/// Top-level declarations are processed in order. Datatypes are registered in
/// the environment, definitions are checked against their annotations, and the
/// most recent definition is normalized and returned as the program result.
pub fn run(path: impl AsRef<Path>) -> Result<RunOutput, RunError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)?;
    run_source(path, &source)
}

fn run_source(root_path: &Path, source: &str) -> Result<RunOutput, RunError> {
    let mut env = Env::new();
    let mut loaded = HashSet::new();
    let import_base = root_path.parent().unwrap_or_else(|| Path::new("."));
    let mut last_def = None;

    process_file_source(
        source,
        import_base,
        &mut env,
        &mut loaded,
        &mut HashSet::new(),
        &mut last_def,
    )?;

    last_def.ok_or(RunError::NoEntryPoint)
}

fn resolve_import_path(base: &Path, path: &str) -> PathBuf {
    let requested = Path::new(path);
    if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        base.join(requested)
    }
}

fn canonical_import_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn process_file_source(
    source: &str,
    import_base: &Path,
    env: &mut Env,
    loaded: &mut HashSet<PathBuf>,
    loading: &mut HashSet<PathBuf>,
    last_def: &mut Option<RunOutput>,
) -> Result<(), RunError> {
    let mut parser = ProgramParser::new(source)?;
    while let Some(decl) = parser.next_decl()? {
        match decl {
            Decl::Import { path } => {
                load_import(&path, env, loaded, loading, import_base, last_def)?;
                parser.sync_from_env(env);
            }
            Decl::Data(dt) => {
                process_data(&dt, env)?;
            }
            Decl::Def { name, ty, val } => {
                *last_def = Some(process_def(&name, &ty, &val, env)?);
            }
        }
    }
    Ok(())
}

fn load_import(
    path: &str,
    env: &mut Env,
    loaded: &mut HashSet<PathBuf>,
    loading: &mut HashSet<PathBuf>,
    import_base: &Path,
    last_def: &mut Option<RunOutput>,
) -> Result<(), RunError> {
    let resolved = resolve_import_path(import_base, path);
    let canonical = canonical_import_path(&resolved);

    if loaded.contains(&canonical) {
        return Ok(());
    }
    if !loading.insert(canonical.clone()) {
        return Err(RunError::Import(format!(
            "circular import involving '{}'",
            resolved.display()
        )));
    }

    let source = std::fs::read_to_string(&resolved).map_err(|err| {
        RunError::Import(format!("cannot read '{}': {}", resolved.display(), err))
    })?;

    let nested_base = resolved.parent().unwrap_or(import_base);
    process_file_source(&source, nested_base, env, loaded, loading, last_def)?;

    loading.remove(&canonical);
    loaded.insert(canonical);
    Ok(())
}

fn process_data(dt: &crate::cubical::syntax::Datatype, env: &mut Env) -> Result<(), RunError> {
    env.declare_datatype(dt.clone());
    for con in &dt.cons {
        for arg_ty in &con.arg_tys {
            check_closed_dt(&env.datatypes, arg_ty, &Term::TUniv(0)).map_err(RunError::Type)?;
        }
    }
    Ok(())
}

fn process_def(name: &Name, ty: &Term, val: &Term, env: &mut Env) -> Result<RunOutput, RunError> {
    println!("Checking definition: {}", name);
    let closed_ty_globals = apply_globals(&env.defs, ty);
    let closed_val = val.clone();

    // Normalize only for the universe-level check; keep the original
    // structure (e.g., Glue types) intact for body checking.
    let closed_ty_nf = nbe_eval(&closed_ty_globals);
    match nbe_eval(&infer_with_full_env(env, &closed_ty_nf)?) {
        Term::TUniv(_) => {}
        other => return Err(TypeError::ExpectedUniverse(other).into()),
    }
    // Register before checking the body so recursive calls resolve.
    env.define(name.clone(), closed_ty_globals.clone(), closed_val.clone());
    check_with_full_env(env, &closed_val, &closed_ty_globals)?;
    let output = RunOutput {
        name: name.clone(),
        ty: closed_ty_globals.clone(),
        value: nbe_eval(&closed_val),
    };

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn run_with_import_merges_declarations() {
        let dir = std::env::temp_dir().join(format!("cubical_import_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let nat_path = dir.join("nat.uwuc");
        let main_path = dir.join("main.uwuc");

        fs::write(&nat_path, "data Nat = | zero : Nat | suc : Nat -> Nat\n").unwrap();
        fs::write(
            &main_path,
            "import \"nat.uwuc\"\n\ndef main : Nat -> Nat = \\n. n\n",
        )
        .unwrap();

        let output = run(&main_path).expect("imported program should run");
        assert_eq!(output.name, "main");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_reports_circular_import() {
        let dir = std::env::temp_dir().join(format!("cubical_cycle_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let a_path = dir.join("a.uwuc");
        let b_path = dir.join("b.uwuc");

        let mut a_file = fs::File::create(&a_path).unwrap();
        writeln!(a_file, "import \"b.uwuc\"").unwrap();
        writeln!(a_file, "def a : U0 = U0").unwrap();

        let mut b_file = fs::File::create(&b_path).unwrap();
        writeln!(b_file, "import \"a.uwuc\"").unwrap();
        writeln!(b_file, "def b : U0 = U0").unwrap();

        let err = run(&a_path).unwrap_err();
        assert!(matches!(err, RunError::Import(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_plus_on_nat() {
        let src = "data Nat = | zero : Nat | suc : Nat -> Nat\n\
                   def plus : Nat -> Nat -> Nat = \\m n. elim (\\_. Nat) \
                   { | zero => n | suc m' => suc (plus m' n) } m\n\
                   def four : Nat = plus (suc (suc zero)) (suc (suc zero))";
        let dir = std::env::temp_dir().join(format!("cubical_plus_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.uwuc");
        fs::write(&path, src).unwrap();
        let output = run(&path).expect("plus should typecheck");
        assert_eq!(output.name, "four");
        let _ = fs::remove_dir_all(&dir);
    }
}
