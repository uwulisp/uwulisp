use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};

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

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Symbol(s) => write!(f, "{}", s),
            Expr::Index(i) => write!(f, "#{}", i),
            Expr::Number(n) => write!(f, "{}", n),
            Expr::List(l) => {
                write!(f, "(")?;
                for (i, e) in l.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{:?}", e)?;
                }
                write!(f, ")")
            }
            Expr::Func(_) => write!(f, "<builtin>"),
            Expr::Lambda(arity, _, _) => write!(f, "<lambda/{}>", arity),
            Expr::Macro(..) => write!(f, "<macro>"),
            Expr::Path(..) => write!(f, "<path>"),
            Expr::Pi(dom, cod, _) => write!(f, "(Π {:?} {:?})", dom, cod),
            Expr::Sigma(dom, cod, _) => write!(f, "(Σ {:?} {:?})", dom, cod),
            Expr::GlueType(base, equiv) => write!(f, "(GlueType {:?} {:?})", base, equiv),
            Expr::Glue(val, equiv) => write!(f, "(glue {:?} {:?})", val, equiv),
        }
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

/// Shared, mutable lexical environment (strong reference).
pub type Env = Rc<std::cell::RefCell<EnvData>>;



pub struct EnvData {
    pub vars: HashMap<String, Expr>,
}

pub fn new_env() -> Env {
    Rc::new(std::cell::RefCell::new(EnvData {
        vars: HashMap::new(),
    }))
}

pub fn env_get(env: &Env, name: &str) -> Result<Expr, String> {
    if let Some(v) = env.borrow().vars.get(name) {
        return Ok(v.clone());
    }
    Err(format!("undefined symbol: {}", name))
}

pub fn env_set(env: &Env, name: String, val: Expr) {
    env.borrow_mut().vars.insert(name, val);
}