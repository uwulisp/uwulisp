pub mod env;
pub mod equality;
pub mod eval;
pub mod interval;
pub mod nbe;
pub mod parser;
pub mod syntax;
pub mod typechecker;

use std::fmt;
use std::path::Path;

use self::env::{Env, apply_globals, check_with_full_env, infer_with_full_env};
use self::nbe::nbe_eval;
use self::parser::{Decl, ParseError, parse_program};
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
    NoEntryPoint,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Io(err) => write!(f, "I/O error: {}", err),
            RunError::Parse(err) => write!(f, "parse error: {}", err),
            RunError::Type(err) => write!(f, "type error:\n{}", err),
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
    let source = std::fs::read_to_string(path)?;
    let decls = parse_program(&source)?;
    let mut env = Env::new();
    let mut last_def = None;

    for decl in decls {
        match decl {
            Decl::Data(dt) => {
                env.declare_datatype(dt.clone());
                for con in &dt.cons {
                    for arg_ty in &con.arg_tys {
                        check_closed_dt(&env.datatypes, arg_ty, &Term::TUniv(0))
                            .map_err(RunError::Type)?;
                    }
                }
            }
            Decl::Def { name, ty, val } => {
                let closed_ty = nbe_eval(&apply_globals(&env.defs, &ty));
                let closed_val = nbe_eval(&apply_globals(&env.defs, &val));

                match nbe_eval(&infer_with_full_env(&env, &closed_ty)?) {
                    Term::TUniv(_) => {}
                    other => return Err(TypeError::ExpectedUniverse(other).into()),
                }
                check_with_full_env(&env, &closed_val, &closed_ty)?;
                let output = RunOutput {
                    name: name.clone(),
                    ty: closed_ty.clone(),
                    value: closed_val.clone(),
                };

                env.define(name, closed_ty, closed_val);
                last_def = Some(output);
            }
        }
    }

    last_def.ok_or(RunError::NoEntryPoint)
}
