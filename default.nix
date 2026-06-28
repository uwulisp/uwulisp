{ pkgs ? import <nixpkgs> {} }:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "pi-lisp";
  version = "2.1.4";

  # The source directory of the project
  src = ./.;

  # Specifying Cargo.lock lets Nix build dependencies hermetically
  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  # Native dependencies (needed only at build time, e.g. pkg-config, cmake)
  nativeBuildInputs = [ ];

  # Dependencies needed at runtime (e.g. openssl, system libraries)
  buildInputs = [ ];

  meta = with pkgs.lib; {
    description = "An experimental Lisp interpreter written in Rust";
    license = licenses.asl20;
    mainProgram = "pilisp";
    platforms = platforms.all;
  };
}
