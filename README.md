# uwulisp

A lightweight, experimental Lisp interpreter written in Rust. Beyond standard Lisp features (macros, lexical scoping, and arithmetic), this project features a unique **cubical type theory flavor**, including **Interval types, Path applications, Dependent Function types ($\Pi$-types), and Dependent Pair types ($\Sigma$-types)**.

## Quick Start

Make sure you have [Rust and Cargo installed](https://www.rust-lang.org/tools/install). Clone the repository and execute the test harness containing the sample expressions:

```bash
cargo run --no-default-features --features vm --release hello1.uwu
cargo run --release hello1.uwu
```

## document
[document](https://uwulisp.github.io)