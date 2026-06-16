use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};

/// Core value/expression type for the Lisp evaluator.
#[derive(Clone)]
pub enum Expr {
    Symbol(String),
    Number(f64),
    /// A string literal, e.g. "hello world". Self-evaluating, like numbers.
    Str(String),
    List(Vec<Expr>),
    Func(Rc<dyn Fn(&[Expr]) -> Result<Expr, String>>),
    Lambda(Vec<String>, Box<Expr>, WeakEnv),
    Macro(Vec<String>, Box<Expr>),
    /// A fully opaque cubical type theory term, injected by the cubical
    /// builtins and consumed by `ctt-eval`, `ctt-infer`, and `ctt-check`.
    CubicalTerm(Box<crate::cubical::syntax::Term>),
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Symbol(s)       => write!(f, "{}", s),
            Expr::Number(n)       => write!(f, "{}", n),
            Expr::Str(s)          => write!(f, "{:?}", s),
            Expr::List(l) => {
                write!(f, "(")?;
                for (i, e) in l.iter().enumerate() {
                    if i > 0 { write!(f, " ")?; }
                    write!(f, "{:?}", e)?;
                }
                write!(f, ")")
            }
            Expr::Func(_)         => write!(f, "<builtin>"),
            Expr::Lambda(..)      => write!(f, "<lambda>"),
            Expr::Macro(..)       => write!(f, "<macro>"),
            // Delegate to the Term's own Display impl (show_term with empty env).
            Expr::CubicalTerm(t)  => write!(f, "<ctt:{}>", t),
        }
    }
}

pub fn is_truthy(e: &Expr) -> bool {
    match e {
        Expr::Number(n)      => *n != 0.0,
        Expr::Str(s)         => !s.is_empty(),
        Expr::List(l)        => !l.is_empty(),
        Expr::CubicalTerm(_) => true, // every well-formed term is truthy
        _                    => true,
    }
}

pub type Env     = Rc<std::cell::RefCell<EnvData>>;
pub type WeakEnv = Weak<std::cell::RefCell<EnvData>>;

pub struct EnvData {
    pub vars:   std::collections::HashMap<String, Expr>,
    pub parent: Option<Env>,
}

pub fn new_env(parent: Option<Env>) -> Env {
    Rc::new(std::cell::RefCell::new(EnvData {
        vars: HashMap::new(),
        parent,
    }))
}

pub fn downgrade(env: &Env) -> WeakEnv { Rc::downgrade(env) }

pub fn upgrade(w: &WeakEnv) -> Result<Env, String> {
    Ok(w.upgrade().unwrap_or_else(|| new_env(None)))
}

pub fn env_get(env: &Env, name: &str) -> Result<Expr, String> {
    if let Some(v) = env.borrow().vars.get(name) {
        return Ok(v.clone());
    }
    if let Some(parent) = &env.borrow().parent {
        return env_get(parent, name);
    }
    Err(format!("undefined symbol: {}", name))
}

pub fn env_set(env: &Env, name: String, val: Expr) {
    env.borrow_mut().vars.insert(name, val);
}