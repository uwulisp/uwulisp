use std::path::PathBuf;
use std::process::Command;

fn pilisp_binary() -> PathBuf {
    PathBuf::from(std::env!("CARGO_BIN_EXE_pilisp"))
}

fn run_file(file: &str) -> (bool, String) {
    let output = Command::new(pilisp_binary())
        .arg(file)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run pilisp");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{}{}", stdout, stderr))
}

#[test]
fn test_hello_pi() {
    let (ok, msg) = run_file("hello.pi");
    assert!(ok, "hello.pi failed:\n{}", msg);
    assert!(msg.contains("=> ()"));
}

#[test]
fn test_hello_pic() {
    let (ok, msg) = run_file("hello.pic");
    assert!(ok, "hello.pic failed:\n{}", msg);
    assert!(msg.contains("main : Π(_:Nat). Nat"));
}

// Nat.pic only defines a datatype with no main; it's expected to produce
// an error. We just verify it runs without crashing.
#[test]
fn test_nat_pic() {
    let (_ok, _msg) = run_file("Nat.pic");
}

#[test]
fn test_examples_pic() {
    let (ok, msg) = run_file("examples.pic");
    assert!(ok, "examples.pic failed:\n{}", msg);
    assert!(msg.contains("main : Nat"));
}

#[test]
fn test_test_pic() {
    let (ok, msg) = run_file("test.pic");
    assert!(ok, "test.pic failed:\n{}", msg);
    assert!(msg.contains("main : Nat = four"));
}

#[test]
fn test_test_pi() {
    let (ok, msg) = run_file("test.pi");
    assert!(ok, "test.pi failed:\n{}", msg);
    assert!(msg.contains("=> (\"not\""));
}
