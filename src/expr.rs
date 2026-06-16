use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};

/// Core value/expression type for the Lisp evaluator.
#[derive(Clone)]
pub enum Expr {
    Symbol(String),
    Number(f64),
    List(Vec<Expr>),
    Func(Rc<dyn Fn(&[Expr]) -> Result<Expr, String>>),
    /// Captures a *weak* reference to the defining environment so that
    /// recursive bindings (`(define f (lambda ...))`) do not form strong
    /// Rc cycles and leak memory.
    Lambda(Vec<String>, Box<Expr>, WeakEnv),
    Macro(Vec<String>, Box<Expr>),
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expr::Symbol(s) => write!(f, "{}", s),
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
            Expr::Lambda(..) => write!(f, "<lambda>"),
            Expr::Macro(..) => write!(f, "<macro>"),
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

/// Non-owning reference to an environment, used inside closures to avoid
/// forming `Rc` cycles when a recursive binding stores a lambda that
/// captures the same environment it is stored in.
pub type WeakEnv = Weak<std::cell::RefCell<EnvData>>;

pub struct EnvData {
    pub vars: HashMap<String, Expr>,
    /// Parent scope. Child→parent links are acyclic (environments form a
    /// tree), so a strong `Rc` is correct and safe here.
    pub parent: Option<Env>,
}

pub fn new_env(parent: Option<Env>) -> Env {
    Rc::new(std::cell::RefCell::new(EnvData {
        vars: HashMap::new(),
        parent,
    }))
}

/// Downgrade a strong `Env` to a `WeakEnv` for storage inside closures.
pub fn downgrade(env: &Env) -> WeakEnv {
    Rc::downgrade(env)
}

/// Upgrade a `WeakEnv` back to a strong `Env` when a closure is called.
/// If the environment has already been freed (e.g. a `refl` path whose
/// dummy env was a temporary), a fresh empty env is returned instead.
/// This is safe because such paths never look up any variables in their
/// closure environment (e.g. the body is a `quote` form).
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