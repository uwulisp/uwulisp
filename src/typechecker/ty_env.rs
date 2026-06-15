//! Type environments for the bidirectional type checker.
//!
//! - [`TyEnv`]: a linked-list of local-variable types, parallel to `LexEnv`.
//! - [`TyGlobal`]: a map from global names to their types.

use std::collections::HashMap;
use std::rc::Rc;

use crate::expr::Expr;

/// Linked-list of local-variable *types*, parallel to `LexEnv`.
#[derive(Clone, Debug)]
pub enum TyEnv {
    Empty,
    Node(Expr, Rc<TyEnv>),
}

impl TyEnv {
    pub fn get(&self, index: usize) -> Option<Expr> {
        let mut curr = self;
        let mut i = index;
        loop {
            match curr {
                TyEnv::Empty => return None,
                TyEnv::Node(ty, next) => {
                    if i == 0 {
                        return Some(ty.clone());
                    }
                    curr = next;
                    i -= 1;
                }
            }
        }
    }
}

/// Global name → type map (separate from the value environment).
pub type TyGlobal = HashMap<String, Expr>;