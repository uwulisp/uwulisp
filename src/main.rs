mod builtins;
mod cubical;
mod env;
mod eval;
mod expr;
mod gc;
mod helper;
mod macros;
mod reader;
mod tinyasm;
mod vm;

use eval::{eval, with_import_base};
use gc::{GcHandle, Heap};
use reader::parse_all;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use crate::helper::shared_read_line;

/// Parses and evaluates all expressions in `src`, printing each result.
///
/// `global_env` is passed as the GC root on every `eval` call so that the
/// mark phase never frees the global frame, even when a cycle fires mid-run.
fn run(src: &str, global_env: GcHandle, heap: &mut Heap) {
    match parse_all(src) {
        Ok(exprs) => {
            for e in exprs {
                match eval(&e, global_env, heap) {
                    Ok(result) => println!("=> {:?}", result),
                    Err(err) => println!("Evaluation Error: {}", err),
                }
            }
        }
        Err(err) => println!("Parse error: {}", err),
    }
}

fn repl(global_env: GcHandle, heap: &mut Heap) {
    println!("pilisp REPL — Ctrl+D to exit");
    let mut input = String::new();

    loop {
        let prompt = if input.is_empty() { "π> " } else { "...  " };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        match shared_read_line() {
            Ok(None) => {
                // EOF (Ctrl+D)
                println!();
                break;
            }
            Ok(Some(line)) => {
                input.push_str(&line);
                input.push('\n');

                if is_balanced(&input) {
                    let src = input.trim().to_string();
                    if !src.is_empty() {
                        run(&src, global_env, heap);
                    }
                    input.clear();
                }
            }
            Err(err) => {
                eprintln!("Input error: {}", err);
                break;
            }
        }
    }
}

/// Returns true when open parens are fully closed and input is non-empty.
fn is_balanced(src: &str) -> bool {
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;

    for ch in src.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"' => in_string = !in_string,
            '(' | '[' if !in_string => depth += 1,
            ')' | ']' if !in_string => depth -= 1,
            _ => {}
        }
    }

    depth <= 0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut heap = Heap::new();
    let global_env = builtins::global_env(&mut heap);

    if args.len() < 2 {
        repl(global_env, &mut heap);
    } else {
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--cubical" {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --cubical requires a filename argument");
                    process::exit(1);
                }
                match cubical::run(&args[i]) {
                    Ok(output) => println!("=> {:?}", output),
                    Err(err) => eprintln!("Cubical error: {}", err),
                }
            } else if args[i] == "--cubical-transpile" {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --cubical-transpile requires a filename argument");
                    process::exit(1);
                }
                let input_path = Path::new(&args[i]);
                let mut out_dir = input_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
                i += 1;
                if i < args.len() && args[i] == "-o" {
                    i += 1;
                    if i >= args.len() {
                        eprintln!("Error: -o requires an output directory argument");
                        process::exit(1);
                    }
                    out_dir = PathBuf::from(&args[i]);
                }
                match cubical::transpile(input_path) {
                    Ok(output) => {
                        if let Err(err) = cubical::write_output(&output, &out_dir) {
                            eprintln!("Transpile write error: {}", err);
                            process::exit(1);
                        }
                        for module in &output.modules {
                            println!(
                                "wrote {}",
                                out_dir
                                    .join(module.path.file_name().unwrap_or_default())
                                    .display()
                            );
                        }
                        if output
                            .modules
                            .iter()
                            .any(|m| m.path.file_stem().and_then(|s| s.to_str()) == Some("Main"))
                        {
                            println!(
                                "run: cd {} && ghc -o app Main.hs && ./app",
                                out_dir.display()
                            );
                        }
                    }
                    Err(err) => {
                        eprintln!("Transpile error: {}", err);
                        process::exit(1);
                    }
                }
            } else {
                let file_path = &args[i];
                let src = match fs::read_to_string(file_path) {
                    Ok(content) => content,
                    Err(err) => {
                        eprintln!(
                            "An error occurred while reading the file '{}': {}",
                            file_path, err
                        );
                        process::exit(1);
                    }
                };
                let base = Path::new(file_path).parent();
                with_import_base(base, || run(&src, global_env, &mut heap));

                #[cfg(feature = "vm")]
                {
                    let (chunks, compilable) = crate::vm::cache_stats();
                    eprintln!("[cache] chunks={} compilable={}", chunks, compilable);
                }
            }
            i += 1;
        }
    }
}
