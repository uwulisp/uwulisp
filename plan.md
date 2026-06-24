remove cubical type when transpile to haskell source code
the transpiler is selfcontained on src/cubical thus you don't need see other directorys
you can test with cargo run -- --cubical-transpile asm.uwuc -o dist
