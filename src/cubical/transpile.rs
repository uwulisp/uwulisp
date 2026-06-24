//! Transpile `.uwuc` cubical surface programs to type-erased Haskell.

mod haskell;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use self::haskell::{
    HaskellModuleCtx, collect_datatype_info, emit_main_driver, emit_module, hs_path_from_uwuc_path,
    module_name_from_path,
};
use crate::cubical::env::Env;
use crate::cubical::parser::{Decl, ParseError, ProgramParser};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmittedModule {
    pub path: PathBuf,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileOutput {
    pub modules: Vec<EmittedModule>,
    pub prelude: Option<EmittedModule>,
}

#[derive(Debug)]
pub enum TranspileError {
    Io(std::io::Error),
    Parse(ParseError),
    Import(String),
}

impl fmt::Display for TranspileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TranspileError::Io(err) => write!(f, "I/O error: {}", err),
            TranspileError::Parse(err) => write!(f, "parse error: {}", err),
            TranspileError::Import(msg) => write!(f, "import error: {}", msg),
        }
    }
}

impl std::error::Error for TranspileError {}

impl From<std::io::Error> for TranspileError {
    fn from(err: std::io::Error) -> Self {
        TranspileError::Io(err)
    }
}

impl From<ParseError> for TranspileError {
    fn from(err: ParseError) -> Self {
        TranspileError::Parse(err)
    }
}

struct ParsedFile {
    uwuc_path: PathBuf,
    decls: Vec<Decl>,
}

/// Read and transpile a `.uwuc` file and its import closure.
pub fn transpile(path: impl AsRef<Path>) -> Result<TranspileOutput, TranspileError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)?;
    transpile_source(path, &source)
}

/// Transpile source rooted at `root_path` (used for relative imports).
pub fn transpile_source(root_path: &Path, source: &str) -> Result<TranspileOutput, TranspileError> {
    let import_base = root_path.parent().unwrap_or_else(|| Path::new("."));
    let mut collector = ImportCollector::new(import_base);
    collector.collect_file(root_path, source)?;

    let mut modules = Vec::new();
    let mut datatype_info = HashMap::new();

    for file in &collector.files {
        let module_name = module_name_from_path(&file.uwuc_path);
        for (name, info) in collect_datatype_info(&file.decls, &module_name) {
            datatype_info.insert(name, info);
        }
        let mut ctx = HaskellModuleCtx::from_decls(module_name.clone(), &file.decls);
        let source_comment = file
            .uwuc_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("input.uwuc");
        let hs_source = emit_module(&mut ctx, &file.decls, source_comment);
        modules.push(EmittedModule {
            path: hs_path_from_uwuc_path(&file.uwuc_path),
            source: hs_source,
        });
    }

    let root_canonical = canonical_import_path(root_path);
    if let Some(root_file) = collector
        .files
        .iter()
        .find(|f| canonical_import_path(&f.uwuc_path) == root_canonical)
    {
        if let Some((entry_name, entry_ty)) =
            root_file.decls.iter().find_map(|decl| match decl {
                Decl::Def { name, ty, .. } if name == "main" => Some((name.as_str(), ty)),
                _ => None,
            })
        {
            let entry_module = module_name_from_path(&root_file.uwuc_path);
            let root_comment = root_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("input.uwuc");
            let driver = emit_main_driver(
                root_comment,
                &entry_module,
                entry_name,
                entry_ty,
                &datatype_info,
            );
            modules.push(EmittedModule {
                path: PathBuf::from("Main.hs"),
                source: driver,
            });
        }
    }

    Ok(TranspileOutput {
        modules,
        prelude: None,
    })
}

/// Write all emitted modules (and optional prelude) under `out_dir`.
pub fn write_output(output: &TranspileOutput, out_dir: &Path) -> Result<(), TranspileError> {
    std::fs::create_dir_all(out_dir)?;
    for module in &output.modules {
        let dest = out_dir.join(
            module
                .path
                .file_name()
                .ok_or_else(|| std::io::Error::other("invalid module path"))?,
        );
        std::fs::write(&dest, &module.source)?;
    }
    if let Some(prelude) = &output.prelude {
        let dest = out_dir.join(&prelude.path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &prelude.source)?;
    }
    Ok(())
}

struct ImportCollector {
    import_base: PathBuf,
    loaded: HashSet<PathBuf>,
    loading: HashSet<PathBuf>,
    files: Vec<ParsedFile>,
    file_index: HashMap<PathBuf, usize>,
}

impl ImportCollector {
    fn new(import_base: &Path) -> Self {
        Self {
            import_base: import_base.to_path_buf(),
            loaded: HashSet::new(),
            loading: HashSet::new(),
            files: Vec::new(),
            file_index: HashMap::new(),
        }
    }

    fn collect_file(&mut self, uwuc_path: &Path, source: &str) -> Result<(), TranspileError> {
        let canonical = canonical_import_path(uwuc_path);
        if self.loaded.contains(&canonical) {
            return Ok(());
        }
        if !self.loading.insert(canonical.clone()) {
            return Err(TranspileError::Import(format!(
                "circular import involving '{}'",
                uwuc_path.display()
            )));
        }

        let mut env = Env::new();
        let mut decls = Vec::new();
        let mut parser = ProgramParser::new(source)?;
        let nested_base = uwuc_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.import_base.clone());

        while let Some(decl) = parser.next_decl()? {
            match &decl {
                Decl::Import { path } => {
                    let resolved = resolve_import_path(&nested_base, path);
                    let import_source = std::fs::read_to_string(&resolved).map_err(|err| {
                        TranspileError::Import(format!(
                            "cannot read '{}': {}",
                            resolved.display(),
                            err
                        ))
                    })?;
                    self.collect_file(&resolved, &import_source)?;
                    self.merge_imported_env(&mut env, &resolved);
                    parser.sync_from_env(&env);
                    decls.push(decl);
                }
                Decl::Data(dt) => {
                    env.declare_datatype(dt.clone());
                    decls.push(decl);
                }
                Decl::Def { name, ty, val } => {
                    env.define(name.clone(), ty.clone(), val.clone());
                    decls.push(decl);
                }
            }
        }

        self.loading.remove(&canonical);
        self.loaded.insert(canonical.clone());

        let parsed = ParsedFile {
            uwuc_path: uwuc_path.to_path_buf(),
            decls,
        };
        self.file_index.insert(canonical, self.files.len());
        self.files.push(parsed);
        Ok(())
    }

    fn merge_imported_env(&mut self, env: &mut Env, imported_path: &Path) {
        let canonical = canonical_import_path(imported_path);
        let Some(&idx) = self.file_index.get(&canonical) else {
            return;
        };
        for decl in &self.files[idx].decls {
            match decl {
                Decl::Data(dt) => env.declare_datatype(dt.clone()),
                Decl::Def { name, ty, val } => env.define(name.clone(), ty.clone(), val.clone()),
                Decl::Import { .. } => {}
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn transpiles_nat_module() {
        let out = transpile("Nat.uwuc").expect("Nat.uwuc should transpile");
        assert_eq!(out.modules.len(), 1);
        let nat = &out.modules[0].source;
        assert!(nat.contains("module Nat where"));
        assert!(nat.contains("Zero"));
        assert!(nat.contains("Suc Nat"));
        assert!(out.prelude.is_none());
        assert!(
            !out.modules
                .iter()
                .any(|m| m.path.file_stem().and_then(|s| s.to_str()) == Some("Main"))
        );
    }

    #[test]
    fn transpiles_hello_with_import() {
        let out = transpile("hello.uwuc").expect("hello.uwuc should transpile");
        assert_eq!(out.modules.len(), 3);
        let hello = out
            .modules
            .iter()
            .find(|m| m.path.file_stem().and_then(|s| s.to_str()) == Some("Hello"))
            .expect("hello module");
        assert!(hello.source.contains("module Hello where"));
        assert!(hello.source.contains("import Nat"));
        assert!(hello.source.contains("main :: Nat -> Nat"));
        assert!(hello.source.contains("Suc Zero"));
        assert!(hello.source.contains("case"));
        let driver = out
            .modules
            .iter()
            .find(|m| m.path.file_stem().and_then(|s| s.to_str()) == Some("Main"))
            .expect("Main driver");
        assert!(driver.source.contains("module Main where"));
        assert!(driver.source.contains("main :: IO ()"));
        assert!(driver.source.contains("Hello.main (Suc (Suc Zero))"));
        assert!(out.prelude.is_none());
    }

    #[test]
    fn transpile_reports_circular_import() {
        let dir =
            std::env::temp_dir().join(format!("cubical_transpile_cycle_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();

        let a_path = dir.join("a.uwuc");
        let b_path = dir.join("b.uwuc");

        let mut a_file = fs::File::create(&a_path).unwrap();
        writeln!(a_file, "import \"b.uwuc\"").unwrap();
        writeln!(a_file, "def a : U0 = U0").unwrap();

        let mut b_file = fs::File::create(&b_path).unwrap();
        writeln!(b_file, "import \"a.uwuc\"").unwrap();
        writeln!(b_file, "def b : U0 = U0").unwrap();

        let err = transpile(&a_path).unwrap_err();
        assert!(matches!(err, TranspileError::Import(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_output_creates_files() {
        let dir =
            std::env::temp_dir().join(format!("cubical_transpile_out_{}", std::process::id()));
        let out = transpile("hello.uwuc").unwrap();
        write_output(&out, &dir).unwrap();
        assert!(dir.join("Nat.hs").exists());
        assert!(dir.join("Hello.hs").exists());
        assert!(dir.join("Main.hs").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn transpile_erases_cubical_terms_to_plain_haskell() {
        let src = "def x : I = i0\n";
        let out = transpile_source(Path::new("test.uwuc"), src).expect("should transpile");
        assert!(out.prelude.is_none());
        let test_mod = &out.modules[0].source;
        assert!(!test_mod.contains("Cubical.Prelude"));
        assert!(!test_mod.contains("i0"));
    }
}
