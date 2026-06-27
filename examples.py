# generated from examples.pic
# data Nat
Nat = None
Zero = ("Zero",)
Suc = lambda a0: ("Suc", a0)
# data Bool
Bool = None
True_ = ("True_",)
False_ = ("False_",)
id = lambda a: lambda x: x

not_ = lambda b: (lambda _v: False_ if _v[0] == "True_" else True_ if _v[0] == "False_" else None)(b)

add = lambda m: lambda n: (lambda _v: n if _v[0] == "Zero" else (lambda k: Suc(add(k)(n)))(_v[1]) if _v[0] == "Suc" else None)(m)

mul = lambda m: lambda n: (lambda _v: Zero if _v[0] == "Zero" else (lambda k: add(n)(mul(k)(n)))(_v[1]) if _v[0] == "Suc" else None)(m)

fac = lambda n: (lambda _v: Suc(Zero) if _v[0] == "Zero" else (lambda k: mul(Suc(k))(fac(k)))(_v[1]) if _v[0] == "Suc" else None)(n)

fib = lambda n: (lambda _v: Zero if _v[0] == "Zero" else (lambda k: (lambda _v: Suc(Zero) if _v[0] == "Zero" else (lambda k2: add(fib(k))(fib(k2)))(_v[1]) if _v[0] == "Suc" else None)(k))(_v[1]) if _v[0] == "Suc" else None)(n)

apply = lambda f: lambda x: f(x)

twice = lambda f: lambda x: f(f(x))

# data S1
S1 = None
Base = ("Base",)
Loop = ("Loop",)
reflBase = Base

loopPath = Loop

loopAtStart = Loop

loopAtEnd = Loop

intervalDemo = lambda i: lambda j: None

intervalDemoUnicode = lambda i: lambda j: None

s1ToBool = lambda x: (lambda _v: True_ if _v[0] == "Base" else True_ if _v[0] == "Loop" else None)(x)

natPair = (Suc(Zero), Suc(Suc(Zero)))

fstOfPair = natPair[0]

depPair = (True_, Zero)

main = add(mul(Suc(Suc(Zero)))(fac(Suc(Suc(Suc(Suc(Suc(Zero))))))))(fib(Suc(Suc(Suc(Suc(Suc(Suc(Suc(Zero)))))))))

