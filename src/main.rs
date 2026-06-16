mod builtins;
mod env;
mod eval;
mod expr;
mod macros;
mod reader;
mod cubical;
mod tinyasm;

use env::Env;
use eval::eval;
use reader::parse_all;
use std::fs;
use std::io::{self, BufRead, Write};
use std::process;

fn run(src: &str, env: &Env) {
    match parse_all(src) {
        Ok(exprs) => {
            for e in exprs {
                match eval(&e, env) {
                    Ok(result) => println!("=> {:?}", result),
                    Err(err) => println!("Evaluation Error: {}", err),
                }
            }
        }
        Err(err) => println!("Parse error: {}", err),
    }
}

fn repl(env: &Env) {
    println!("uwulisp REPL — Ctrl+D to exit");
    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        // Show a continuation prompt when brackets are unbalanced
        let prompt = if input.is_empty() { "uwu> " } else { "...  " };
        print!("{}", prompt);
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                // EOF (Ctrl+D)
                println!();
                break;
            }
            Ok(_) => {
                input.push_str(&line);

                // Only evaluate once parentheses are balanced
                if is_balanced(&input) {
                    let src = input.trim().to_string();
                    if !src.is_empty() {
                        run(&src, env);
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

    let env = builtins::global_env();

    if args.len() < 2 {
        // No file given — drop into REPL
        repl(&env);
    } else {
        let file_path = &args[1];
        let src = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(err) => {
                eprintln!("An error occurred while reading the file '{}': {}", file_path, err);
                process::exit(1);
            }
        };
        run(&src, &env);
    }
}