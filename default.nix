{ pkgs ? import <nixpkgs> {}
, features ? [ "vm" "jit" ]
}:

pkgs.rustPlatform.buildRustPackage rec {
  pname = "uwulisp";
  version = "1.3.0";

  # The source directory of the project
  src = ./.;

  # Specifying Cargo.lock lets Nix build dependencies hermetically
  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  buildNoDefaultFeatures = true;
  buildFeatures = features;

  # Native dependencies (needed only at build time, e.g. pkg-config, cmake)
  nativeBuildInputs = [ ];

  # Dependencies needed at runtime (e.g. openssl, system libraries)
  buildInputs = [ ];

  meta = with pkgs.lib; {
    description = "An experimental Lisp interpreter written in Rust";
    license = licenses.asl20;
    mainProgram = "uwulisp";
    platforms = platforms.all;
  };
}
