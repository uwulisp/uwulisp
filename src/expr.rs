use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Core value/expression type for the Lisp evaluator.
#[derive(Clone)]
pub enum Expr {
    Symbol(String),
    Index(usize), // De Bruijn index for local variables
    Number(f64),
    List(Vec<Expr>),
    Func(Rc<dyn Fn(&[Expr]) -> Result<Expr, String>>),
    // Lambda takes the number of arguments (arity), body, and lexical environment
    Lambda(usize, Box<Expr>, Rc<LexEnv>),
    Macro(Vec<String>, Box<Expr>), // Macros operate on S-expressions (surface syntax)
    // Path, Pi, and Sigma each bind exactly 1 variable, so no arity needed.
    Path(Box<Expr>, Rc<LexEnv>),
    Pi(Box<Expr>, Box<Expr>, Rc<LexEnv>),
    Sigma(Box<Expr>, Box<Expr>, Rc<LexEnv>),
    /// `(glue-type base equiv)` — the Glue type former.
    ///
    /// `base` is the base type A; `equiv` is an equivalence e : B ≃ A,
    /// represented here as a function B → A (the forward/coercion direction).
    /// A term of type `GlueType(A, e)` is a value in B that is "glued" to A
    /// via e, i.e. there is a canonical element `e(b) : A` for each `b : B`.
    GlueType(Box<Expr>, Box<Expr>),
    /// `(glue val equiv)` — the Glue introduction form.
    ///
    /// `val` is the B-side value and `equiv` is the forward function B → A.
    /// The pair records both so that `unglue` can extract the A-side image.
    Glue(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug)]
pub enum LexEnv {
    Empty,
    Node(Expr, Rc<LexEnv>),
}

impl LexEnv {
    pub fn get(&self, index: usize) -> Option<Expr> {
        let mut curr = self;
        let mut i = index;
        loop {
            match curr {
                LexEnv::Empty => return None,
                LexEnv::Node(val, next) => {
                    if i == 0 {
                        return Some(val.clone());
                    }
                    curr = next;
                    i -= 1;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Display — user-facing printer (what the REPL shows)
//
// Prints values as readable S-expressions.  Internal details like De Bruijn
// indices, arities, and captured environments are hidden; the user sees only
// the value's observable shape.
// ---------------------------------------------------------------------------
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Symbol(s)    => write!(f, "{}", s),
            Expr::Index(i)     => write!(f, "#{}", i),   // should not appear in fully-evaluated output
            Expr::Number(n)    => {
                // Print integers without a decimal point for readability.
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Expr::List(l) => {
                write!(f, "(")?;
                for (i, e) in l.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            Expr::Func(_)              => write!(f, "<builtin>"),
            Expr::Lambda(arity, _, _)  => write!(f, "<lambda/{}>", arity),
            Expr::Macro(..)            => write!(f, "<macro>"),
            Expr::Path(..)             => write!(f, "<path>"),
            Expr::Pi(dom, cod, _)      => write!(f, "(Π {} {})", dom, cod),
            Expr::Sigma(dom, cod, _)   => write!(f, "(Σ {} {})", dom, cod),
            Expr::GlueType(base, eq)   => write!(f, "(GlueType {} {})", base, eq),
            Expr::Glue(val, eq)        => write!(f, "(glue {} {})", val, eq),
        }
    }
}

// ---------------------------------------------------------------------------
// Debug — internal printer (used in error messages and {:?} formatting)
//
// Identical structure to Display but uses {:?} recursively so it is always
// available without the Display bound, and can diverge later if we want more
// internal detail in debug builds.
// ---------------------------------------------------------------------------
impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Delegate to Display for now; add more internal detail here if needed.
        write!(f, "{}", self)
    }
}

/// Returns whether an Expr should be treated as "true" in conditionals.
/// Number 0 and empty lists are falsy; everything else is truthy.
pub fn is_truthy(e: &Expr) -> bool {
    match e {
        Expr::Number(n) => *n != 0.0,
        Expr::List(l) => !l.is_empty(),
        _ => true,
    }
}

/// Returns whether a symbol name is an internal type system sentinel.
pub fn is_sentinel_symbol(s: &str) -> bool {
    s == "__Num__"
        || s == "__Type__"
        || s == "__Any__"
        || s == "__GlueType__"
        || s == "__Path__"
        || s == "__Glue__"
}

/// Shared, mutable global environment (strong reference).
pub type Env = Rc<std::cell::RefCell<EnvData>>;

pub struct EnvData {
    pub vars: HashMap<String, Expr>,
}

pub fn new_env() -> Env {
    Rc::new(std::cell::RefCell::new(EnvData {
        vars: HashMap::new(),
    }))
}

/// Look up a name in the global environment, returning an error if absent.
pub fn env_get(env: &Env, name: &str) -> Result<Expr, String> {
    env_get_opt(env, name).ok_or_else(|| format!("undefined symbol: {}", name))
}

/// Look up a name in the global environment, returning `None` if absent.
///
/// Prefer this over `env_get` when absence is not an error (e.g. optional
/// macro dispatch, feature detection).
pub fn env_get_opt(env: &Env, name: &str) -> Option<Expr> {
    env.borrow().vars.get(name).cloned()
}

pub fn env_set(env: &Env, name: String, val: Expr) {
    env.borrow_mut().vars.insert(name, val);
}