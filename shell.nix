{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
    gcc
    python3
  ];

  shellHook = ''
    export RUST_SRC_PATH=${pkgs.rustPlatform.rustLibSrc}
    echo "🦀 Welcome to the Rust development environment! 🦀"
    rustc --version
  '';
}
