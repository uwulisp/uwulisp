i tried test the cubical with [test.uwuc](file;file:///home/jihoo/uwulisp/test.uwuc) using "cargo run -- --cubical test.uwuc" command and i got this error
Cubical error: type error:
  Expected a Π-type, but found:
    Σ(A:U0). Π(B:U0). Σ(B:U0). A
and it's parser and other backends are self contained in [cubical](file;file:///home/jihoo/uwulisp/src/cubical) thus you don't need see other directory 
can you fix the error?