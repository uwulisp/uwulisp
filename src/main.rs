mod builtins;
mod env;
mod eval;
mod expr;
mod macros;
mod reader;
mod cubical;

use env::Env;
use eval::eval;
use reader::parse_all;
use std::fs;  // 파일을 읽기 위해 추가
use std::process;

/// 소스 코드 전체를 받아서 파싱한 뒤, 각 탑레벨 표현식을 차례대로 평가합니다.
fn run(src: &str, env: &Env) {
    match parse_all(src) {
        Ok(exprs) => {
            for e in exprs {
                match eval(&e, env) {
                    Ok(result) => println!("=> {:?}", result), // 출력 형식을 조금 깔끔하게 다듬었습니다.
                    Err(err) => println!("Evaluation Error: {}", err),
                }
            }
        }
        Err(err) => println!("Parse error: {}", err),
    }
}

fn main() {
    // 1. 명령행 인자 처리 (프로그램 이름 제외하고 파일 경로가 들어왔는지 확인)
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("사용법: cargo run -- <파일경로>");
        process::exit(1);
    }
    
    let file_path = &args[1];

    // 2. 파일 읽기
    let src = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(err) => {
            eprintln!("파일을 읽는 중 오류가 발생했습니다 '{}': {}", file_path, err);
            process::exit(1);
        }
    };

    // 3. 글로벌 환경 초기화 및 실행
    let env = builtins::global_env();
    run(&src, &env);
}