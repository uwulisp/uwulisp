# pi-lisp

A lightweight, experimental Lisp interpreter written in Rust. Beyond standard Lisp features (macros, lexical scoping, and arithmetic), this project features a unique **cubical type theory flavor**, including **Interval types, Path applications, Dependent Function types ($\Pi$-types), and Dependent Pair types ($\Sigma$-types)**.

## Quick Start

Make sure you have [Rust and Cargo installed](https://www.rust-lang.org/tools/install). Clone the repository and execute the test harness containing the sample expressions:

```bash
cargo run --release
#if you use nix
nix run github:pi-lisp/pi-lisp
nix shell github:pi-lisp/pi-lisp
```

## Testing

Run the full test suite (79 unit tests + 6 integration tests):

```bash
cargo test
```

Run example files individually:

```bash
cargo run --release hello.pi
cargo run --release hello.pic
cargo run --release test.pi
cargo run --release test.pic
cargo run --release examples.pic
```

## document
[document](https://pi-lisp.github.io)
