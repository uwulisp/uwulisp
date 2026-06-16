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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("uwulisp -- file");
        process::exit(1);
    }
    
    let file_path = &args[1];

    let src = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("An error occurred while reading the file. '{}': {}", file_path, err);
            process::exit(1);
        }
    };

    let env = builtins::global_env();
    run(&src, &env);
}