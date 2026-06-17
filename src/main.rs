mod builtins;
mod env;
mod eval;
mod expr;
mod gc;
mod macros;
mod reader;
mod cubical;
mod tinyasm;
mod helper;

use eval::eval;
use gc::{Heap, GcHandle};
use reader::parse_all;
use std::fs;
use std::io::{self, Write};
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
                    Err(err)   => println!("Evaluation Error: {}", err),
                }
            }
        }
        Err(err) => println!("Parse error: {}", err),
    }
}

fn repl(global_env: GcHandle, heap: &mut Heap) {
    println!("uwulisp REPL — Ctrl+D to exit");
    let mut input = String::new();

    loop {
        let prompt = if input.is_empty() { "uwu> " } else { "...  " };
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
    let mut in_string  = false;
    let mut escape     = false;

    for ch in src.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape = true,
            '"'               => in_string = !in_string,
            '(' | '[' if !in_string => depth += 1,
            ')' | ']' if !in_string => depth -= 1,
            _ => {}
        }
    }

    depth <= 0
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Create the single GC heap that owns all environment frames for the
    // lifetime of this interpreter process.
    let mut heap = Heap::new();

    // global_env is the permanent root: it must be passed to every GC
    // collection so the mark phase never frees the top-level frame.
    let global_env = builtins::global_env(&mut heap);

    if args.len() < 2 {
        repl(global_env, &mut heap);
    } else {
        for file_path in &args[1..] {
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
            run(&src, global_env, &mut heap);
        }
    }
}