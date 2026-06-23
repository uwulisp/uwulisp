#![allow(dead_code)]

use crate::cubical::interval::{DNF, I, dnf_bot, dnf_top, eval_interval};
use crate::cubical::syntax::{ElimCase, Level, Name, Term};

pub type Env = Vec<Value>;

#[derive(Debug, Clone)]
pub enum Value {
    VNeutral(Neutral),
    VLam(Name, Closure),
    VApp(Box<Value>, Box<Value>),
    VPi(Name, Box<Value>, Closure),
    VSigma(Name, Box<Value>, Closure),
    VPair(Box<Value>, Box<Value>),
    VPath(Box<Value>, Box<Value>, Box<Value>),
    VPLam(Name, IClosure),
    VPApp(Box<Value>, Box<Value>),
    VUniv(Level),
    VIntervalTy,
    VInterval(I),
    VIntervalVar(usize),
    VCube(DNF),
    VData(Name),
    VCon(Name, Name, Vec<Value>),
    VPCon(Name, Name, Vec<Value>, Box<Value>),
    VElim(Box<Value>, Vec<ElimCase>, Box<Value>),
    VGlue(Box<Value>, DNF, Box<Value>),
    VGlueElem(DNF, Box<Value>, Box<Value>),
    VUnglue(DNF, Box<Value>, Box<Value>),
    VEquiv(Box<Value>, Box<Value>),
    VMkEquiv(
        Box<Value>,
        Box<Value>,
        Box<Value>,
        Box<Value>,
        Box<Value>,
        Box<Value>,
    ),
    VEquivFwd(Box<Value>, Box<Value>),
    VUa(Box<Value>),
    VTransport(Box<Value>, Box<Value>),
    VHComp(Box<Value>, DNF, Box<Value>, Box<Value>),
    VFst(Box<Value>),
    VSnd(Box<Value>),
}

#[derive(Debug, Clone)]
pub struct Closure {
    pub env: Env,
    pub body: Term,
}

#[derive(Debug, Clone)]
pub struct IClosure {
    pub env: Env,
    pub body: Term,
}

#[derive(Debug, Clone)]
pub enum Neutral {
    NVar(usize),
    NApp(Box<Neutral>, Box<Value>),
    NPApp(Box<Neutral>, Box<Value>),
    NFst(Box<Neutral>),
    NSnd(Box<Neutral>),
    NElim(Box<Value>, Vec<ElimCase>, Box<Neutral>),
    NTransport(Box<Value>, Box<Value>),
    NHComp(Box<Value>, DNF, Box<Value>, Box<Value>),
}

impl Closure {
    pub fn apply(&self, v: Value) -> Value {
        let mut env = vec![v];
        env.extend_from_slice(&self.env);
        eval_nbe(&env, &self.body)
    }
}

impl IClosure {
    pub fn apply_i(&self, i: I) -> Value {
        self.apply_interval_value(Value::VInterval(i))
    }

    fn apply_i_var(&self, level: usize) -> Value {
        self.apply_interval_value(Value::VIntervalVar(level))
    }

    fn apply_interval_value(&self, v: Value) -> Value {
        let mut env = vec![v];
        env.extend_from_slice(&self.env);
        eval_nbe(&env, &self.body)
    }
}

pub fn eval_nbe(env: &[Value], t: &Term) -> Value {
    match t {
        Term::TVar(i) => {
            let i = *i as usize;
            env.get(i)
                .cloned()
                .unwrap_or_else(|| Value::VNeutral(Neutral::NVar(i - env.len())))
        }
        Term::TApp(f, a) => do_apply(eval_nbe(env, f), eval_nbe(env, a)),
        Term::TAbs(x, b) => Value::VLam(
            x.clone(),
            Closure {
                env: env.to_vec(),
                body: (**b).clone(),
            },
        ),
        Term::TUniv(n) => Value::VUniv(*n),
        Term::TIntervalTy => Value::VIntervalTy,
        Term::TPi(x, a, b) => Value::VPi(
            x.clone(),
            Box::new(eval_nbe(env, a)),
            Closure {
                env: env.to_vec(),
                body: (**b).clone(),
            },
        ),
        Term::TInterval(i) => Value::VInterval(i.clone()),
        Term::TCube(c) => Value::VCube(c.clone()),
        Term::TPath(a, u, v) => Value::VPath(
            Box::new(eval_nbe(env, a)),
            Box::new(eval_nbe(env, u)),
            Box::new(eval_nbe(env, v)),
        ),
        Term::PLam(x, b) => Value::VPLam(
            x.clone(),
            IClosure {
                env: env.to_vec(),
                body: (**b).clone(),
            },
        ),
        Term::PApp(p, r) => do_papp(eval_nbe(env, p), eval_nbe(env, r)),
        Term::THComp(a, phi, tube, base) => do_hcomp(
            eval_nbe(env, a),
            value_to_dnf(eval_nbe(env, phi)),
            eval_nbe(env, tube),
            eval_nbe(env, base),
        ),
        Term::TEquiv(a, b) => Value::VEquiv(Box::new(eval_nbe(env, a)), Box::new(eval_nbe(env, b))),
        Term::TMkEquiv(a, b, f, g, eta, eps) => Value::VMkEquiv(
            Box::new(eval_nbe(env, a)),
            Box::new(eval_nbe(env, b)),
            Box::new(eval_nbe(env, f)),
            Box::new(eval_nbe(env, g)),
            Box::new(eval_nbe(env, eta)),
            Box::new(eval_nbe(env, eps)),
        ),
        Term::TEquivFwd(e, x) => do_equiv_fwd(eval_nbe(env, e), eval_nbe(env, x)),
        Term::TUa(e) => Value::VUa(Box::new(eval_nbe(env, e))),
        Term::TTransport(p, x) => do_transport(eval_nbe(env, p), eval_nbe(env, x)),
        Term::TGlue(a, phi, te) => {
            let phi = value_to_dnf(eval_nbe(env, phi));
            let te = eval_nbe(env, te);
            if phi == dnf_top() {
                match te {
                    Value::VLam(_, clos) => {
                        let body = clos.apply(Value::VInterval(I::I1));
                        equiv_dom_value(body)
                    }
                    other => equiv_dom_value(other),
                }
            } else if phi == dnf_bot() {
                eval_nbe(env, a)
            } else {
                Value::VGlue(Box::new(eval_nbe(env, a)), phi, Box::new(te))
            }
        }
        Term::TGlueElem(phi, t, a) => {
            let phi = value_to_dnf(eval_nbe(env, phi));
            if phi == dnf_top() {
                eval_nbe(env, t)
            } else if phi == dnf_bot() {
                eval_nbe(env, a)
            } else {
                Value::VGlueElem(phi, Box::new(eval_nbe(env, t)), Box::new(eval_nbe(env, a)))
            }
        }
        Term::TUnglue(phi, te, g) => {
            let phi = value_to_dnf(eval_nbe(env, phi));
            let te = eval_nbe(env, te);
            let g = eval_nbe(env, g);
            if phi == dnf_top() {
                do_equiv_fwd(te, g)
            } else if phi == dnf_bot() {
                g
            } else {
                Value::VUnglue(phi, Box::new(te), Box::new(g))
            }
        }
        Term::TSigma(x, a, b) => Value::VSigma(
            x.clone(),
            Box::new(eval_nbe(env, a)),
            Closure {
                env: env.to_vec(),
                body: (**b).clone(),
            },
        ),
        Term::TPair(a, b) => Value::VPair(Box::new(eval_nbe(env, a)), Box::new(eval_nbe(env, b))),
        Term::TFst(p) => do_fst(eval_nbe(env, p)),
        Term::TSnd(p) => do_snd(eval_nbe(env, p)),
        Term::TData(d) => Value::VData(d.clone()),
        Term::TCon(data, con, args) => Value::VCon(
            data.clone(),
            con.clone(),
            args.iter().map(|a| eval_nbe(env, a)).collect(),
        ),
        Term::TPCon(data, con, args, r) => Value::VPCon(
            data.clone(),
            con.clone(),
            args.iter().map(|a| eval_nbe(env, a)).collect(),
            Box::new(eval_nbe(env, r)),
        ),
        Term::TElim(motive, cases, scrut) => {
            do_elim(eval_nbe(env, motive), cases, eval_nbe(env, scrut), env)
        }
    }
}

pub fn do_apply(f: Value, a: Value) -> Value {
    match f {
        Value::VLam(_, clos) => clos.apply(a),
        Value::VNeutral(n) => Value::VNeutral(Neutral::NApp(Box::new(n), Box::new(a))),
        other => Value::VApp(Box::new(other), Box::new(a)),
    }
}

pub fn do_papp(p: Value, r: Value) -> Value {
    if let Some(i) = value_to_endpoint(&r) {
        if let Value::VPLam(_, clos) = p {
            return clos.apply_i(i);
        }
    }

    match p {
        Value::VPLam(_, clos) => match r {
            Value::VInterval(i) => clos.apply_i(i),
            Value::VIntervalVar(level) => clos.apply_i_var(level),
            other => Value::VPApp(
                Box::new(Value::VPLam("_".to_string(), clos)),
                Box::new(other),
            ),
        },
        Value::VNeutral(n) => Value::VNeutral(Neutral::NPApp(Box::new(n), Box::new(r))),
        other => Value::VPApp(Box::new(other), Box::new(r)),
    }
}

pub fn do_fst(p: Value) -> Value {
    match p {
        Value::VPair(a, _) => *a,
        Value::VNeutral(n) => Value::VNeutral(Neutral::NFst(Box::new(n))),
        other => Value::VFst(Box::new(other)),
    }
}

pub fn do_snd(p: Value) -> Value {
    match p {
        Value::VPair(_, b) => *b,
        Value::VNeutral(n) => Value::VNeutral(Neutral::NSnd(Box::new(n))),
        other => Value::VSnd(Box::new(other)),
    }
}

pub fn do_elim(motive: Value, cases: &[ElimCase], scrut: Value, env: &[Value]) -> Value {
    match scrut {
        Value::VCon(_, con, args) => match cases.iter().find(|case| case.con == con) {
            Some(case) => {
                let mut env2: Env = args.into_iter().rev().collect();
                env2.extend_from_slice(env);
                eval_nbe(&env2, &case.body)
            }
            None => Value::VElim(
                Box::new(motive),
                cases.to_vec(),
                Box::new(Value::VCon("".into(), con, args)),
            ),
        },
        Value::VPCon(_, con, args, r) => match cases.iter().find(|case| case.con == con) {
            Some(case) => {
                let mut env2: Env = args.into_iter().rev().collect();
                env2.extend_from_slice(env);
                let body = eval_nbe(&env2, &case.body);
                do_papp(body, *r)
            }
            None => Value::VElim(
                Box::new(motive),
                cases.to_vec(),
                Box::new(Value::VPCon("".into(), con, args, r)),
            ),
        },
        Value::VNeutral(n) => stuck_elim(motive, cases, n),
        other => Value::VElim(Box::new(motive), cases.to_vec(), Box::new(other)),
    }
}

pub fn do_transport(p: Value, x: Value) -> Value {
    match p {
        Value::VUa(e) => do_equiv_fwd(*e, x),
        Value::VPLam(_, clos) => {
            let b0 = clos.apply_i(I::I0);
            let b1 = clos.apply_i(I::I1);
            if quote(0, b0.clone()) == quote(0, b1.clone()) {
                return x;
            }
            match (&b0, &b1) {
                (Value::VUniv(_), Value::VUniv(_)) => x,
                _ => Value::VTransport(Box::new(Value::VPLam("_".to_string(), clos)), Box::new(x)),
            }
        }
        other => Value::VNeutral(Neutral::NTransport(Box::new(other), Box::new(x))),
    }
}

pub fn do_hcomp(a_ty: Value, phi: DNF, tube: Value, base: Value) -> Value {
    if phi == dnf_top() {
        do_papp(tube, Value::VInterval(I::I1))
    } else if phi == dnf_bot() {
        base
    } else {
        Value::VHComp(Box::new(a_ty), phi, Box::new(tube), Box::new(base))
    }
}

pub fn quote(size: usize, v: Value) -> Term {
    match v {
        Value::VNeutral(n) => quote_neutral(size, n),
        Value::VLam(x, clos) => Term::TAbs(
            x,
            Box::new(quote(
                size + 1,
                clos.apply(Value::VNeutral(Neutral::NVar(size))),
            )),
        ),
        Value::VApp(f, a) => Term::TApp(Box::new(quote(size, *f)), Box::new(quote(size, *a))),
        Value::VPi(x, a, b) => Term::TPi(
            x,
            Box::new(quote(size, *a)),
            Box::new(quote(
                size + 1,
                b.apply(Value::VNeutral(Neutral::NVar(size))),
            )),
        ),
        Value::VSigma(x, a, b) => Term::TSigma(
            x,
            Box::new(quote(size, *a)),
            Box::new(quote(
                size + 1,
                b.apply(Value::VNeutral(Neutral::NVar(size))),
            )),
        ),
        Value::VPair(a, b) => Term::TPair(Box::new(quote(size, *a)), Box::new(quote(size, *b))),
        Value::VFst(p) => Term::TFst(Box::new(quote(size, *p))),
        Value::VSnd(p) => Term::TSnd(Box::new(quote(size, *p))),
        Value::VPath(a, u, v) => Term::TPath(
            Box::new(quote(size, *a)),
            Box::new(quote(size, *u)),
            Box::new(quote(size, *v)),
        ),
        Value::VPLam(x, clos) => Term::PLam(x, Box::new(quote(size + 1, clos.apply_i_var(size)))),
        Value::VPApp(p, r) => Term::PApp(Box::new(quote(size, *p)), Box::new(quote(size, *r))),
        Value::VUniv(n) => Term::TUniv(n),
        Value::VIntervalTy => Term::TIntervalTy,
        Value::VInterval(i) => Term::TInterval(i),
        Value::VIntervalVar(level) => level_to_var(size, level),
        Value::VCube(c) => Term::TCube(c),
        Value::VData(d) => Term::TData(d),
        Value::VCon(d, c, args) => {
            Term::TCon(d, c, args.into_iter().map(|a| quote(size, a)).collect())
        }
        Value::VPCon(d, c, args, r) => Term::TPCon(
            d,
            c,
            args.into_iter().map(|a| quote(size, a)).collect(),
            Box::new(quote(size, *r)),
        ),
        Value::VElim(motive, cases, scrut) => Term::TElim(
            Box::new(quote(size, *motive)),
            quote_cases(size, cases),
            Box::new(quote(size, *scrut)),
        ),
        Value::VGlue(a, phi, te) => Term::TGlue(
            Box::new(quote(size, *a)),
            Box::new(Term::TCube(phi)),
            Box::new(quote(size, *te)),
        ),
        Value::VGlueElem(phi, t, a) => Term::TGlueElem(
            Box::new(Term::TCube(phi)),
            Box::new(quote(size, *t)),
            Box::new(quote(size, *a)),
        ),
        Value::VUnglue(phi, te, g) => Term::TUnglue(
            Box::new(Term::TCube(phi)),
            Box::new(quote(size, *te)),
            Box::new(quote(size, *g)),
        ),
        Value::VEquiv(a, b) => Term::TEquiv(Box::new(quote(size, *a)), Box::new(quote(size, *b))),
        Value::VMkEquiv(a, b, f, g, eta, eps) => Term::TMkEquiv(
            Box::new(quote(size, *a)),
            Box::new(quote(size, *b)),
            Box::new(quote(size, *f)),
            Box::new(quote(size, *g)),
            Box::new(quote(size, *eta)),
            Box::new(quote(size, *eps)),
        ),
        Value::VEquivFwd(e, x) => {
            Term::TEquivFwd(Box::new(quote(size, *e)), Box::new(quote(size, *x)))
        }
        Value::VUa(e) => Term::TUa(Box::new(quote(size, *e))),
        Value::VTransport(p, x) => {
            Term::TTransport(Box::new(quote(size, *p)), Box::new(quote(size, *x)))
        }
        Value::VHComp(a, phi, tube, base) => Term::THComp(
            Box::new(quote(size, *a)),
            Box::new(Term::TCube(phi)),
            Box::new(quote(size, *tube)),
            Box::new(quote(size, *base)),
        ),
    }
}

fn quote_neutral(size: usize, n: Neutral) -> Term {
    match n {
        Neutral::NVar(level) => level_to_var(size, level),
        Neutral::NApp(f, a) => {
            Term::TApp(Box::new(quote_neutral(size, *f)), Box::new(quote(size, *a)))
        }
        Neutral::NPApp(p, r) => {
            Term::PApp(Box::new(quote_neutral(size, *p)), Box::new(quote(size, *r)))
        }
        Neutral::NFst(p) => Term::TFst(Box::new(quote_neutral(size, *p))),
        Neutral::NSnd(p) => Term::TSnd(Box::new(quote_neutral(size, *p))),
        Neutral::NElim(motive, cases, scrut) => Term::TElim(
            Box::new(quote(size, *motive)),
            quote_cases(size, cases),
            Box::new(quote_neutral(size, *scrut)),
        ),
        Neutral::NTransport(p, x) => {
            Term::TTransport(Box::new(quote(size, *p)), Box::new(quote(size, *x)))
        }
        Neutral::NHComp(a, phi, tube, base) => Term::THComp(
            Box::new(quote(size, *a)),
            Box::new(Term::TCube(phi)),
            Box::new(quote(size, *tube)),
            Box::new(quote(size, *base)),
        ),
    }
}

fn quote_cases(size: usize, cases: Vec<ElimCase>) -> Vec<ElimCase> {
    cases
        .into_iter()
        .map(|case| ElimCase {
            con: case.con,
            binders: case.binders.clone(),
            body: Box::new(normalize_under_binders(
                size,
                case.binders.len(),
                &case.body,
            )),
        })
        .collect()
}

fn normalize_under_binders(size: usize, binders: usize, body: &Term) -> Term {
    let mut env = Vec::new();
    for level in (size..size + binders).rev() {
        env.push(Value::VNeutral(Neutral::NVar(level)));
    }
    quote(size + binders, eval_nbe(&env, body))
}

pub fn normalize(env: &[Value], t: &Term) -> Term {
    quote(env.len(), eval_nbe(env, t))
}

fn max_var(t: &Term) -> i32 {
    match t {
        Term::TVar(i) => *i,
        Term::TApp(f, a) => max_var(f).max(max_var(a)),
        Term::TAbs(_, b) => (max_var(b) - 1).max(-1),
        Term::TUniv(_) => -1,
        Term::TIntervalTy => -1,
        Term::TPi(_, a, b) => max_var(a).max(max_var(b) - 1).max(-1),
        Term::TInterval(_) => -1,
        Term::TCube(_) => -1,
        Term::TPath(a, u, v) => max_var(a).max(max_var(u)).max(max_var(v)),
        Term::PLam(_, b) => (max_var(b) - 1).max(-1),
        Term::PApp(p, r) => max_var(p).max(max_var(r)),
        Term::THComp(a, phi, u, u0) => max_var(a).max(max_var(phi)).max(max_var(u)).max(max_var(u0)),
        Term::TEquiv(a, b) => max_var(a).max(max_var(b)),
        Term::TMkEquiv(a, b, f, g, eta, eps) => max_var(a)
            .max(max_var(b))
            .max(max_var(f))
            .max(max_var(g))
            .max(max_var(eta))
            .max(max_var(eps)),
        Term::TEquivFwd(e, x) => max_var(e).max(max_var(x)),
        Term::TUa(e) => max_var(e),
        Term::TTransport(p, x) => max_var(p).max(max_var(x)),
        Term::TGlue(a, phi, te) => max_var(a).max(max_var(phi)).max(max_var(te)),
        Term::TGlueElem(phi, t, a) => max_var(phi).max(max_var(t)).max(max_var(a)),
        Term::TUnglue(phi, te, g) => max_var(phi).max(max_var(te)).max(max_var(g)),
        Term::TSigma(_, a, b) => max_var(a).max(max_var(b) - 1).max(-1),
        Term::TPair(a, b) => max_var(a).max(max_var(b)),
        Term::TFst(p) => max_var(p),
        Term::TSnd(p) => max_var(p),
        Term::TData(_) => -1,
        Term::TCon(_, _, args) => args.iter().map(max_var).fold(-1, |m, x| m.max(x)),
        Term::TPCon(_, _, args, r) => args.iter().map(max_var).fold(-1, |m, x| m.max(x)).max(max_var(r)),
        Term::TElim(motive, cases, scrut) => {
            let mut m = max_var(motive).max(max_var(scrut));
            for case in cases {
                let n = case.binders.len() as i32;
                m = m.max(max_var(&case.body) - n);
            }
            m.max(-1)
        }
    }
}

pub fn nbe_eval(t: &Term) -> Term {
    let mv = max_var(t);
    if mv < 0 {
        normalize(&[], t)
    } else {
        let size = (mv + 1) as usize;
        let mut env = Vec::with_capacity(size);
        for level in (0..size).rev() {
            env.push(Value::VNeutral(Neutral::NVar(level)));
        }
        normalize(&env, t)
    }
}

fn do_equiv_fwd(e: Value, x: Value) -> Value {
    match e {
        Value::VMkEquiv(_, _, f, _, _, _) => do_apply(*f, x),
        other => Value::VEquivFwd(Box::new(other), Box::new(x)),
    }
}

fn equiv_dom_value(v: Value) -> Value {
    match v {
        Value::VMkEquiv(a, _, _, _, _, _) | Value::VEquiv(a, _) => *a,
        Value::VPair(a, _) => *a,
        other => other,
    }
}

fn stuck_elim(motive: Value, cases: &[ElimCase], n: Neutral) -> Value {
    Value::VNeutral(Neutral::NElim(
        Box::new(motive),
        cases.to_vec(),
        Box::new(n),
    ))
}

fn value_to_dnf(v: Value) -> DNF {
    match v {
        Value::VCube(d) => d,
        Value::VInterval(i) => eval_interval(&i),
        Value::VIntervalVar(level) => eval_interval(&I::IVar(level as i32)),
        other => match quote(0, other) {
            Term::TCube(d) => d,
            Term::TInterval(i) => eval_interval(&i),
            _ => dnf_bot(),
        },
    }
}

fn value_to_endpoint(v: &Value) -> Option<I> {
    match v {
        Value::VInterval(i) => {
            let d = eval_interval(i);
            if d == dnf_bot() {
                Some(I::I0)
            } else if d == dnf_top() {
                Some(I::I1)
            } else {
                None
            }
        }
        Value::VCube(d) if *d == dnf_bot() => Some(I::I0),
        Value::VCube(d) if *d == dnf_top() => Some(I::I1),
        _ => None,
    }
}

fn level_to_var(size: usize, level: usize) -> Term {
    if level < size {
        Term::TVar((size - level - 1) as i32)
    } else {
        Term::TVar(level.saturating_sub(size) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(t: Term) -> Box<Term> {
        Box::new(t)
    }

    #[test]
    fn identity_function_normalizes_to_itself() {
        let id = Term::TAbs("x".to_string(), b(Term::TVar(0)));
        assert_eq!(nbe_eval(&id), id);
    }

    #[test]
    fn beta_reduces_identity_application() {
        let term = Term::TApp(
            b(Term::TAbs("x".to_string(), b(Term::TVar(0)))),
            b(Term::TUniv(0)),
        );
        assert_eq!(nbe_eval(&term), Term::TUniv(0));
    }

    #[test]
    fn fst_of_pair_reduces() {
        let term = Term::TFst(b(Term::TPair(b(Term::TUniv(0)), b(Term::TUniv(1)))));
        assert_eq!(nbe_eval(&term), Term::TUniv(0));
    }

    #[test]
    fn transport_over_constant_family_is_identity() {
        let family = Term::PLam("i".to_string(), b(Term::TUniv(0)));
        let term = Term::TTransport(b(family), b(Term::TUniv(1)));
        assert_eq!(nbe_eval(&term), Term::TUniv(1));
    }
}
