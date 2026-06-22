//! Hand-written parser for the cubical surface language.
//!
//! The parser resolves ordinary variables and interval variables to de Bruijn
//! indices as it parses. Top-level definitions parsed earlier in a program are
//! available to later declarations as globals.

use crate::cubical::interval::I;
use crate::cubical::syntax::{ConSig, Datatype, ElimCase, Name, PConSig, Term};
use std::fmt;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.message, self.line, self.col)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    Def { name: Name, ty: Term, val: Term },
    Data(Datatype),
}

pub fn parse_term(src: &str) -> Result<Term, ParseError> {
    let tokens = Lexer::new(src).lex()?;
    let mut parser = Parser::new(tokens);
    let term = parser.parse_term()?;
    parser.expect(TokenKind::Eof, "expected end of input")?;
    Ok(term)
}

pub fn parse_program(src: &str) -> Result<Vec<Decl>, ParseError> {
    let tokens = Lexer::new(src).lex()?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

/// Parse and typecheck a complete program.
///
/// Declarations are processed in order. Each `data` declaration is added to
/// the datatype environment before the next declaration is checked, so a `def`
/// can refer to any datatype declared above it — exactly the behaviour the
/// user expects.
///
/// Returns the list of successfully checked definitions (name, type, value)
/// together with the collected datatypes, or a human-readable error string.
pub fn typecheck_program(
    src: &str,
) -> Result<(Vec<crate::cubical::syntax::Datatype>, Vec<(String, crate::cubical::syntax::Term, crate::cubical::syntax::Term)>), String> {
    use crate::cubical::typechecker::check_closed_dt;
    use crate::cubical::syntax::Datatype;

    let decls = parse_program(src).map_err(|e| e.to_string())?;

    let mut dts: Vec<Datatype> = Vec::new();
    let mut defs: Vec<(String, crate::cubical::syntax::Term, crate::cubical::syntax::Term)> = Vec::new();

    for decl in decls {
        match decl {
            Decl::Data(dt) => {
                // Make the datatype available to all subsequent declarations.
                dts.push(dt);
            }
            Decl::Def { name, ty, val } => {
                // Check the definition body against its declared type, with
                // all datatypes declared so far in scope.
                check_closed_dt(&dts, &val, &ty)
                    .map_err(|e| format!("type error in '{}': {}", name, e))?;
                defs.push((name, ty, val));
            }
        }
    }

    Ok((dts, defs))
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Int(i32),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LAngle,
    RAngle,
    Colon,
    Comma,
    Dot,
    Arrow,
    FatArrow,
    Pipe,
    At,
    Backslash,
    Star,
    Slash,
    AndSym,
    OrSym,
    Tilde,
    LBracket,
    RBracket,
    Equals,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    line: usize,
    col: usize,
}

struct Lexer<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars(),
            peeked: None,
            line: 1,
            col: 1,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            let line = self.line;
            let col = self.col;
            match ch {
                c if c.is_whitespace() => {
                    self.bump();
                }
                '-' => {
                    self.bump();
                    if self.peek() == Some('-') {
                        while let Some(c) = self.peek() {
                            self.bump();
                            if c == '\n' {
                                break;
                            }
                        }
                    } else if self.peek() == Some('>') {
                        self.bump();
                        tokens.push(tok(TokenKind::Arrow, line, col));
                    } else {
                        return Err(err("unexpected '-'", line, col));
                    }
                }
                '=' => {
                    self.bump();
                    if self.peek() == Some('>') {
                        self.bump();
                        tokens.push(tok(TokenKind::FatArrow, line, col));
                    } else {
                        tokens.push(tok(TokenKind::Equals, line, col));
                    }
                }
                '/' => {
                    self.bump();
                    if self.peek() == Some('\\') {
                        self.bump();
                        tokens.push(tok(TokenKind::AndSym, line, col));
                    } else {
                        tokens.push(tok(TokenKind::Slash, line, col));
                    }
                }
                '\\' => {
                    self.bump();
                    if self.peek() == Some('/') {
                        self.bump();
                        tokens.push(tok(TokenKind::OrSym, line, col));
                    } else {
                        tokens.push(tok(TokenKind::Backslash, line, col));
                    }
                }
                '(' => {
                    self.bump();
                    tokens.push(tok(TokenKind::LParen, line, col));
                }
                ')' => {
                    self.bump();
                    tokens.push(tok(TokenKind::RParen, line, col));
                }
                '{' => {
                    self.bump();
                    tokens.push(tok(TokenKind::LBrace, line, col));
                }
                '}' => {
                    self.bump();
                    tokens.push(tok(TokenKind::RBrace, line, col));
                }
                '<' | '⟨' => {
                    self.bump();
                    tokens.push(tok(TokenKind::LAngle, line, col));
                }
                '>' | '⟩' => {
                    self.bump();
                    tokens.push(tok(TokenKind::RAngle, line, col));
                }
                ':' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Colon, line, col));
                }
                ',' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Comma, line, col));
                }
                '.' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Dot, line, col));
                }
                '|' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Pipe, line, col));
                }
                '@' => {
                    self.bump();
                    tokens.push(tok(TokenKind::At, line, col));
                }
                '*' | '×' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Star, line, col));
                }
                '[' => {
                    self.bump();
                    tokens.push(tok(TokenKind::LBracket, line, col));
                }
                ']' => {
                    self.bump();
                    tokens.push(tok(TokenKind::RBracket, line, col));
                }
                '~' | '¬' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Tilde, line, col));
                }
                '∧' => {
                    self.bump();
                    tokens.push(tok(TokenKind::AndSym, line, col));
                }
                '∨' => {
                    self.bump();
                    tokens.push(tok(TokenKind::OrSym, line, col));
                }
                'λ' => {
                    self.bump();
                    tokens.push(tok(TokenKind::Backslash, line, col));
                }
                c if c.is_ascii_digit() => tokens.push(self.lex_int(line, col)?),
                c if is_ident_start(c) => tokens.push(self.lex_ident(line, col)),
                other => return Err(err(format!("unexpected character '{}'", other), line, col)),
            }
        }
        tokens.push(tok(TokenKind::Eof, self.line, self.col));
        Ok(tokens)
    }

    fn lex_int(&mut self, line: usize, col: usize) -> Result<Token, ParseError> {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        match text.parse::<i32>() {
            Ok(n) => Ok(tok(TokenKind::Int(n), line, col)),
            Err(_) => Err(err("integer literal is too large", line, col)),
        }
    }

    fn lex_ident(&mut self, line: usize, col: usize) -> Token {
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if is_ident_continue(c) {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        tok(TokenKind::Ident(text), line, col)
    }

    fn peek(&mut self) -> Option<char> {
        if self.peeked.is_none() {
            self.peeked = self.chars.next();
        }
        self.peeked
    }

    fn bump(&mut self) -> Option<char> {
        let ch = match self.peeked.take() {
            Some(c) => Some(c),
            None => self.chars.next(),
        }?;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c == '\'' || c == '?' || c == '!' || c == '-' || c.is_alphanumeric()
}

fn tok(kind: TokenKind, line: usize, col: usize) -> Token {
    Token { kind, line, col }
}

fn err(message: impl Into<String>, line: usize, col: usize) -> ParseError {
    ParseError {
        message: message.into(),
        line,
        col,
    }
}

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    term_env: Vec<Name>,
    ivar_env: Vec<Name>,
    global_env: Vec<Name>,
    datatypes: Vec<Datatype>,
    /// When true, `starts_atom` treats the keyword `with` as a stop token.
    stop_at_with: bool,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            term_env: Vec::new(),
            ivar_env: Vec::new(),
            global_env: Vec::new(),
            datatypes: Vec::new(),
            stop_at_with: false,
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Decl>, ParseError> {
        let mut decls = Vec::new();
        while !self.at(&TokenKind::Eof) {
            let decl = if self.consume_ident("def") {
                self.parse_def()?
            } else if self.consume_ident("data") {
                self.parse_data_decl()?
            } else {
                return Err(self.error_here("expected top-level declaration"));
            };
            match &decl {
                Decl::Def { name, .. } => self.global_env.insert(0, name.clone()),
                Decl::Data(dt) => self.datatypes.push(dt.clone()),
            }
            decls.push(decl);
        }
        Ok(decls)
    }

    fn parse_def(&mut self) -> Result<Decl, ParseError> {
        let name = self.expect_ident("expected definition name")?;
        self.expect(
            TokenKind::Colon,
            format!("expected ':' after definition name '{}'", name),
        )?;
        let ty = self.parse_term()?;
        self.expect(
            TokenKind::Equals,
            format!("expected '=' after type for definition '{}'", name),
        )?;
        let val = self.parse_term()?;
        Ok(Decl::Def { name, ty, val })
    }

    fn parse_data_decl(&mut self) -> Result<Decl, ParseError> {
        let name = self.expect_ident("expected datatype name")?;
        self.expect(
            TokenKind::Equals,
            format!("expected '=' after datatype name '{}'", name),
        )?;
        let mut cons = Vec::new();
        let mut pcons = Vec::new();
        let mut local_dt = Datatype {
            name: name.clone(),
            cons: Vec::new(),
            pcons: Vec::new(),
        };
        while self.consume(&TokenKind::Pipe) {
            let con_name = self.expect_ident("expected constructor name after '|'")?;
            self.expect(
                TokenKind::Colon,
                format!("expected ':' after constructor name '{}'", con_name),
            )?;
            let (arg_tys, result) = self.parse_constructor_type(&name, &local_dt)?;
            if result != Term::TData(name.clone()) {
                return Err(self.error_here(format!(
                    "constructor '{}' must return datatype '{}'",
                    con_name, name
                )));
            }
            if self.consume(&TokenKind::LBracket) {
                let face0 = self.parse_face_with_extra_datatype(&local_dt)?;
                self.expect(
                    TokenKind::Comma,
                    "expected ',' between path-constructor faces",
                )?;
                let face1 = self.parse_face_with_extra_datatype(&local_dt)?;
                self.expect(
                    TokenKind::RBracket,
                    "expected ']' after path-constructor faces",
                )?;
                let sig = PConSig {
                    name: con_name,
                    arg_tys,
                    face0,
                    face1,
                };
                local_dt.pcons.push(sig.clone());
                pcons.push(sig);
            } else {
                let sig = ConSig {
                    name: con_name,
                    arg_tys,
                };
                local_dt.cons.push(sig.clone());
                cons.push(sig);
            }
        }
        if cons.is_empty() && pcons.is_empty() {
            return Err(self.error_here(format!(
                "datatype '{}' must declare at least one constructor",
                name
            )));
        }
        Ok(Decl::Data(Datatype { name, cons, pcons }))
    }

    fn parse_constructor_type(
        &mut self,
        dt_name: &str,
        local_dt: &Datatype,
    ) -> Result<(Vec<Term>, Term), ParseError> {
        let old_dts_len = self.datatypes.len();
        self.datatypes.push(local_dt.clone());
        let ty = self.parse_term()?;
        self.datatypes.truncate(old_dts_len);
        let mut args = Vec::new();
        let mut cur = ty;
        loop {
            match cur {
                Term::TPi(_, a, b) => {
                    args.push(*a);
                    cur = *b;
                }
                Term::TData(ref n) if n == dt_name => return Ok((args, cur)),
                other => return Ok((args, other)),
            }
        }
    }

    fn parse_face_with_extra_datatype(&mut self, dt: &Datatype) -> Result<Term, ParseError> {
        let old_len = self.datatypes.len();
        self.datatypes.push(dt.clone());
        let term = self.parse_arrow_star();
        self.datatypes.truncate(old_len);
        term
    }

    fn parse_term(&mut self) -> Result<Term, ParseError> {
        self.parse_lambda()
    }

    fn parse_lambda(&mut self) -> Result<Term, ParseError> {
        if self.consume(&TokenKind::Backslash) {
            let binders = self.parse_one_or_more_idents("expected lambda binder after '\\'")?;
            self.expect(TokenKind::Dot, "expected '.' after lambda binder list")?;
            for binder in binders.iter().rev() {
                self.term_env.insert(0, binder.clone());
            }
            let body = self.parse_term()?;
            for _ in &binders {
                self.term_env.remove(0);
            }
            let mut term = body;
            for binder in binders.into_iter().rev() {
                term = Term::TAbs(binder, Box::new(term));
            }
            return Ok(term);
        }
        if self.consume_ident("fun") {
            let binders = self.parse_one_or_more_idents("expected binder after 'fun'")?;
            self.expect(
                TokenKind::FatArrow,
                "expected '=>' after function binder list",
            )?;
            for binder in binders.iter().rev() {
                self.term_env.insert(0, binder.clone());
            }
            let body = self.parse_term()?;
            for _ in &binders {
                self.term_env.remove(0);
            }
            let mut term = body;
            for binder in binders.into_iter().rev() {
                term = Term::TAbs(binder, Box::new(term));
            }
            return Ok(term);
        }
        if self.consume(&TokenKind::LAngle) {
            let binder = self.expect_ident("expected interval binder after '<'")?;
            self.expect(TokenKind::RAngle, "expected '>' after interval binder")?;
            self.ivar_env.insert(0, binder.clone());
            let body = self.parse_term()?;
            self.ivar_env.remove(0);
            return Ok(Term::PLam(binder, Box::new(body)));
        }
        if self.consume_ident("Π") || self.consume_ident("Pi") {
            let (binder, ty) = self.parse_parenthesized_binder("Pi")?;
            self.expect(TokenKind::Dot, "expected '.' after Pi binder")?;
            self.term_env.insert(0, binder.clone());
            let body = self.parse_term()?;
            self.term_env.remove(0);
            return Ok(Term::TPi(binder, Box::new(ty), Box::new(body)));
        }
        if self.consume_ident("Σ") || self.consume_ident("Sigma") {
            let (binder, ty) = self.parse_parenthesized_binder("Sigma")?;
            self.expect(TokenKind::Dot, "expected '.' after Sigma binder")?;
            self.term_env.insert(0, binder.clone());
            let body = self.parse_term()?;
            self.term_env.remove(0);
            return Ok(Term::TSigma(binder, Box::new(ty), Box::new(body)));
        }
        self.parse_pair()
    }

    fn parse_pair(&mut self) -> Result<Term, ParseError> {
        let left = self.parse_arrow_star()?;
        if self.consume(&TokenKind::Comma) {
            let right = self.parse_term()?;
            Ok(Term::TPair(Box::new(left), Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_arrow_star(&mut self) -> Result<Term, ParseError> {
        let left = self.parse_join()?;
        if self.consume(&TokenKind::Arrow) {
            let right = self.parse_arrow_star()?;
            Ok(Term::TPi("_".to_string(), Box::new(left), Box::new(right)))
        } else if self.consume(&TokenKind::Star) {
            let right = self.parse_arrow_star()?;
            Ok(Term::TSigma(
                "_".to_string(),
                Box::new(left),
                Box::new(right),
            ))
        } else {
            Ok(left)
        }
    }

    fn parse_join(&mut self) -> Result<Term, ParseError> {
        let mut term = self.parse_meet()?;
        while self.consume(&TokenKind::OrSym) {
            let rhs = self.parse_meet()?;
            term = interval_binary(term, rhs, |a, b| I::Join(Box::new(a), Box::new(b)), self)?;
        }
        Ok(term)
    }

    fn parse_meet(&mut self) -> Result<Term, ParseError> {
        let mut term = self.parse_tilde()?;
        while self.consume(&TokenKind::AndSym) {
            let rhs = self.parse_tilde()?;
            term = interval_binary(term, rhs, |a, b| I::Meet(Box::new(a), Box::new(b)), self)?;
        }
        Ok(term)
    }

    fn parse_tilde(&mut self) -> Result<Term, ParseError> {
        if self.consume(&TokenKind::Tilde) {
            let term = self.parse_tilde()?;
            let i = expect_interval(term, self)?;
            Ok(Term::TInterval(I::Neg(Box::new(i))))
        } else {
            self.parse_papp()
        }
    }

    fn parse_papp(&mut self) -> Result<Term, ParseError> {
        let mut term = self.parse_app()?;
        while self.consume(&TokenKind::At) {
            let rhs = self.parse_tilde()?;
            if let Term::TCon(dt, con, args) = term {
                if self.is_path_constructor(&dt, &con) {
                    term = Term::TPCon(dt, con, args, Box::new(rhs));
                } else {
                    term = Term::PApp(Box::new(Term::TCon(dt, con, args)), Box::new(rhs));
                }
            } else {
                term = Term::PApp(Box::new(term), Box::new(rhs));
            }
        }
        Ok(term)
    }

    fn parse_app(&mut self) -> Result<Term, ParseError> {
        let first = self.parse_prefix_or_atom()?;
        let mut args = Vec::new();
        while self.starts_atom() {
            args.push(self.parse_prefix_or_atom()?);
        }
        if let Term::TCon(dt, con, mut con_args) = first {
            con_args.extend(args);
            return Ok(Term::TCon(dt, con, con_args));
        }
        let mut term = first;
        for arg in args {
            term = Term::TApp(Box::new(term), Box::new(arg));
        }
        Ok(term)
    }

    fn parse_prefix_or_atom(&mut self) -> Result<Term, ParseError> {
        if self.consume_ident("fst") {
            return Ok(Term::TFst(Box::new(self.parse_prefix_or_atom()?)));
        }
        if self.consume_ident("snd") {
            return Ok(Term::TSnd(Box::new(self.parse_prefix_or_atom()?)));
        }
        if self.consume_ident("ua") {
            return Ok(Term::TUa(Box::new(self.parse_prefix_or_atom()?)));
        }
        if self.consume_ident("transport") {
            let p = self.parse_prefix_or_atom()?;
            let x = self.parse_prefix_or_atom()?;
            return Ok(Term::TTransport(Box::new(p), Box::new(x)));
        }
        if self.consume_ident("equivFwd") {
            let e = self.parse_prefix_or_atom()?;
            let x = self.parse_prefix_or_atom()?;
            return Ok(Term::TEquivFwd(Box::new(e), Box::new(x)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Term, ParseError> {
        if self.consume_ident("Path") {
            let a = self.parse_prefix_or_atom()?;
            let u = self.parse_prefix_or_atom()?;
            let v = self.parse_prefix_or_atom()?;
            return Ok(Term::TPath(Box::new(a), Box::new(u), Box::new(v)));
        }
        if self.consume_ident("hcomp") {
            let a = self.parse_prefix_or_atom()?;
            let phi = self.parse_prefix_or_atom()?;
            let u = self.parse_prefix_or_atom()?;
            let u0 = self.parse_prefix_or_atom()?;
            return Ok(Term::THComp(
                Box::new(a),
                Box::new(phi),
                Box::new(u),
                Box::new(u0),
            ));
        }
        if self.consume_ident("Equiv") {
            let a = self.parse_prefix_or_atom()?;
            let b = self.parse_prefix_or_atom()?;
            return Ok(Term::TEquiv(Box::new(a), Box::new(b)));
        }
        if self.consume_ident("mkEquiv") {
            let a = self.parse_prefix_or_atom()?;
            let b = self.parse_prefix_or_atom()?;
            let f = self.parse_prefix_or_atom()?;
            let g = self.parse_prefix_or_atom()?;
            let eta = self.parse_prefix_or_atom()?;
            let eps = self.parse_prefix_or_atom()?;
            return Ok(Term::TMkEquiv(
                Box::new(a),
                Box::new(b),
                Box::new(f),
                Box::new(g),
                Box::new(eta),
                Box::new(eps),
            ));
        }
        if self.consume_ident("Glue") {
            let a = self.parse_prefix_or_atom()?;
            let phi = self.parse_prefix_or_atom()?;
            let te = self.parse_prefix_or_atom()?;
            return Ok(Term::TGlue(Box::new(a), Box::new(phi), Box::new(te)));
        }
        if self.consume_ident("glueElem") || self.consume_ident("glue") {
            let phi = self.parse_prefix_or_atom()?;
            let t = self.parse_prefix_or_atom()?;
            let a = self.parse_prefix_or_atom()?;
            return Ok(Term::TGlueElem(Box::new(phi), Box::new(t), Box::new(a)));
        }
        if self.consume_ident("unglue") {
            let phi = self.parse_prefix_or_atom()?;
            let te = self.parse_prefix_or_atom()?;
            let g = self.parse_prefix_or_atom()?;
            return Ok(Term::TUnglue(Box::new(phi), Box::new(te), Box::new(g)));
        }
        if self.consume_ident("elim") {
            return self.parse_elim();
        }
        if self.consume_ident("elim[") {
            return self.parse_elim();
        }
        if self.consume_ident("match") {
            return self.parse_match();
        }

        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.pos += 1;
                self.resolve_ident(name)
            }
            TokenKind::Int(0) => {
                self.pos += 1;
                Ok(Term::TInterval(I::I0))
            }
            TokenKind::Int(1) => {
                self.pos += 1;
                Ok(Term::TInterval(I::I1))
            }
            TokenKind::LParen => self.parse_paren(),
            other => Err(self.error_here(format!("expected term, found {}", describe(&other)))),
        }
    }

    fn parse_paren(&mut self) -> Result<Term, ParseError> {
        self.expect(TokenKind::LParen, "expected '('")?;
        if let Some(name) = self.try_parse_binder_header()? {
            self.expect(TokenKind::RParen, "unmatched '('")?;
            if self.consume(&TokenKind::Arrow) {
                self.term_env.insert(0, name.0.clone());
                let body = self.parse_arrow_star()?;
                self.term_env.remove(0);
                return Ok(Term::TPi(name.0, Box::new(name.1), Box::new(body)));
            }
            if self.consume(&TokenKind::Star) {
                self.term_env.insert(0, name.0.clone());
                let body = self.parse_arrow_star()?;
                self.term_env.remove(0);
                return Ok(Term::TSigma(name.0, Box::new(name.1), Box::new(body)));
            }
            return self.resolve_ident(name.0);
        }
        let term = self.parse_term()?;
        if self.consume(&TokenKind::Colon) {
            let _ty = self.parse_term()?;
            self.expect(TokenKind::RParen, "unmatched '('")?;
            return Ok(term);
        }
        self.expect(TokenKind::RParen, "unmatched '('")?;
        Ok(term)
    }

    fn parse_parenthesized_binder(&mut self, form: &str) -> Result<(Name, Term), ParseError> {
        self.expect(
            TokenKind::LParen,
            format!("expected '(' after {} type former", form),
        )?;
        let binder = self.expect_ident(format!("expected binder name in {} type former", form))?;
        self.expect(
            TokenKind::Colon,
            format!("expected ':' after binder name '{}'", binder),
        )?;
        let ty = self.parse_term()?;
        self.expect(TokenKind::RParen, "unmatched '('")?;
        Ok((binder, ty))
    }

    fn try_parse_binder_header(&mut self) -> Result<Option<(Name, Term)>, ParseError> {
        let save = self.pos;
        let name = match self.peek().kind.clone() {
            TokenKind::Ident(n) => {
                self.pos += 1;
                n
            }
            _ => return Ok(None),
        };
        if !self.consume(&TokenKind::Colon) {
            self.pos = save;
            return Ok(None);
        }
        let ty = self.parse_term()?;
        Ok(Some((name, ty)))
    }

    fn parse_elim(&mut self) -> Result<Term, ParseError> {
        let bracketed = self.consume(&TokenKind::LBracket);
        let motive = self.parse_term()?;
        if bracketed {
            self.expect(TokenKind::RBracket, "expected ']' after eliminator motive")?;
        }
        let cases = self.parse_elim_cases(true)?;
        let scrutinee = self.parse_term()?;
        Ok(Term::TElim(Box::new(motive), cases, Box::new(scrutinee)))
    }

    fn parse_match(&mut self) -> Result<Term, ParseError> {
        let (scrutinee, binder) = if let TokenKind::Ident(name) = self.peek().kind.clone() {
            self.pos += 1;
            let scrut = self.resolve_ident(name.clone())?;
            (scrut, name)
        } else {
            (self.parse_term()?, "_match".to_string())
        };

        self.term_env.insert(0, binder.clone());
        self.expect_ident("return")?;
        self.stop_at_with = true;
        let return_type = self.parse_term()?;
        self.stop_at_with = false;
        self.term_env.remove(0);

        self.expect_ident("with")?;
        let cases = self.parse_elim_cases(false)?;
        let motive = Term::TAbs(binder, Box::new(return_type));
        Ok(Term::TElim(Box::new(motive), cases, Box::new(scrutinee)))
    }

    /// Parse eliminator/match case arms. When `require_brace` is true (`elim`), `{`
    /// is mandatory; when false (`match`), cases may start with `|` or `{ | ... }`.
    fn parse_elim_cases(&mut self, require_brace: bool) -> Result<Vec<ElimCase>, ParseError> {
        let braced = if require_brace {
            self.expect(TokenKind::LBrace, "expected '{' before eliminator cases")?;
            true
        } else {
            let braced = self.consume(&TokenKind::LBrace);
            if !braced && !self.at(&TokenKind::Pipe) {
                return Err(self.error_here("expected '{' or '|' before match cases"));
            }
            braced
        };

        let mut cases = Vec::new();
        self.consume(&TokenKind::Pipe);
        loop {
            if braced && self.at(&TokenKind::RBrace) {
                break;
            }

            let con = self.expect_ident("expected constructor name in eliminator case")?;
            let mut binders = Vec::new();
            while let TokenKind::Ident(name) = self.peek().kind.clone() {
                if name == "=>" {
                    break;
                }
                self.pos += 1;
                binders.push(name);
            }
            if self.consume(&TokenKind::FatArrow) || self.consume(&TokenKind::Arrow) {
                for binder in binders.iter().rev() {
                    self.term_env.insert(0, binder.clone());
                }
                let body = self.parse_term()?;
                for _ in &binders {
                    self.term_env.remove(0);
                }
                cases.push(ElimCase {
                    con,
                    binders,
                    body: Box::new(body),
                });
            } else {
                return Err(self.error_here("expected '=>' after eliminator case binders"));
            }
            if !self.consume(&TokenKind::Pipe) {
                break;
            }
        }

        if braced {
            self.expect(TokenKind::RBrace, "expected '}' after eliminator cases")?;
        }
        Ok(cases)
    }

    fn resolve_ident(&self, name: Name) -> Result<Term, ParseError> {
        if name == "Type" {
            return Ok(Term::TUniv(0));
        }
        if name == "I" || name == "𝕀" {
            return Ok(Term::TIntervalTy);
        }
        if name == "i0" {
            return Ok(Term::TInterval(I::I0));
        }
        if name == "i1" {
            return Ok(Term::TInterval(I::I1));
        }
        if let Some(level) = parse_universe(&name) {
            return Ok(Term::TUniv(level));
        }
        if let Some(idx) = self.term_env.iter().position(|n| n == &name) {
            return Ok(Term::TVar(idx as i32));
        }
        if let Some(idx) = self.global_env.iter().position(|n| n == &name) {
            return Ok(Term::TVar((self.term_env.len() + idx) as i32));
        }
        if let Some(idx) = self.ivar_env.iter().position(|n| n == &name) {
            return Ok(Term::TInterval(I::IVar(idx as i32)));
        }
        if let Some((dt, is_path)) = self.find_constructor(&name) {
            if is_path {
                return Ok(Term::TCon(dt, name, Vec::new()));
            }
            return Ok(Term::TCon(dt, name, Vec::new()));
        }
        if self.datatypes.iter().any(|dt| dt.name == name) {
            return Ok(Term::TData(name));
        }
        Err(self.error_here(format!("unknown name or constructor '{}'", name)))
    }

    fn find_constructor(&self, name: &str) -> Option<(Name, bool)> {
        for dt in self.datatypes.iter().rev() {
            if dt.cons.iter().any(|c| c.name == name) {
                return Some((dt.name.clone(), false));
            }
            if dt.pcons.iter().any(|c| c.name == name) {
                return Some((dt.name.clone(), true));
            }
        }
        None
    }

    fn is_path_constructor(&self, dt_name: &str, con_name: &str) -> bool {
        self.datatypes
            .iter()
            .rev()
            .find(|dt| dt.name == dt_name)
            .is_some_and(|dt| dt.pcons.iter().any(|c| c.name == con_name))
    }

    fn parse_one_or_more_idents(
        &mut self,
        message: impl Into<String>,
    ) -> Result<Vec<Name>, ParseError> {
        let first = self.expect_ident(message)?;
        let mut names = vec![first];
        while let TokenKind::Ident(name) = self.peek().kind.clone() {
            self.pos += 1;
            names.push(name);
        }
        Ok(names)
    }

    fn is_decl_start(&self) -> bool {
        matches!(
            &self.peek().kind,
            TokenKind::Ident(name) if name == "def" || name == "data"
        )
    }

    fn starts_atom(&self) -> bool {
        if self.is_decl_start() {
            return false;
        }
        if self.stop_at_with {
            if let TokenKind::Ident(name) = &self.peek().kind {
                if name == "with" {
                    return false;
                }
            }
        }
        matches!(
            &self.peek().kind,
            TokenKind::Ident(_) | TokenKind::Int(_) | TokenKind::LParen
        )
    }

    fn expect_ident(&mut self, message: impl Into<String>) -> Result<Name, ParseError> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => {
                self.pos += 1;
                Ok(name)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn consume_ident(&mut self, expected: &str) -> bool {
        match &self.peek().kind {
            TokenKind::Ident(name) if name == expected => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn consume(&mut self, expected: &TokenKind) -> bool {
        if self.at(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(
        &mut self,
        expected: TokenKind,
        message: impl Into<String>,
    ) -> Result<(), ParseError> {
        if self.consume(&expected) {
            Ok(())
        } else {
            Err(self.error_here(message))
        }
    }

    fn at(&self, expected: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(expected)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        let token = self.peek();
        err(message, token.line, token.col)
    }
}

fn parse_universe(name: &str) -> Option<i32> {
    let rest = name.strip_prefix('U')?;
    if rest.is_empty() {
        return None;
    }
    rest.parse::<i32>().ok()
}

fn expect_interval(term: Term, parser: &Parser) -> Result<I, ParseError> {
    match term {
        Term::TInterval(i) => Ok(i),
        other => Err(parser.error_here(format!("expected interval expression, got {:?}", other))),
    }
}

fn interval_binary(
    left: Term,
    right: Term,
    mk: fn(I, I) -> I,
    parser: &Parser,
) -> Result<Term, ParseError> {
    let l = expect_interval(left, parser)?;
    let r = expect_interval(right, parser)?;
    Ok(Term::TInterval(mk(l, r)))
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s) => format!("'{}'", s),
        TokenKind::Int(n) => n.to_string(),
        TokenKind::LParen => "'('".to_string(),
        TokenKind::RParen => "')'".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::LAngle => "'<'".to_string(),
        TokenKind::RAngle => "'>'".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::Dot => "'.'".to_string(),
        TokenKind::Arrow => "'->'".to_string(),
        TokenKind::FatArrow => "'=>'".to_string(),
        TokenKind::Pipe => "'|'".to_string(),
        TokenKind::At => "'@'".to_string(),
        TokenKind::Backslash => "'\\'".to_string(),
        TokenKind::Star => "'*'".to_string(),
        TokenKind::Slash => "'/'".to_string(),
        TokenKind::AndSym => "'/\\'".to_string(),
        TokenKind::OrSym => "'\\/'".to_string(),
        TokenKind::Tilde => "'~'".to_string(),
        TokenKind::LBracket => "'['".to_string(),
        TokenKind::RBracket => "']'".to_string(),
        TokenKind::Equals => "'='".to_string(),
        TokenKind::Eof => "end of input".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::syntax::show_term;

    #[test]
    fn parses_lambda_identity() {
        assert_eq!(
            parse_term("\\x. x").unwrap(),
            Term::TAbs("x".to_string(), Box::new(Term::TVar(0)))
        );
    }

    #[test]
    fn parses_dependent_pi() {
        assert_eq!(
            parse_term("(x : U0) -> x").unwrap(),
            Term::TPi(
                "x".to_string(),
                Box::new(Term::TUniv(0)),
                Box::new(Term::TVar(0))
            )
        );
    }

    #[test]
    fn parses_path_lambda() {
        assert_eq!(
            parse_term("<i> i0").unwrap(),
            Term::PLam("i".to_string(), Box::new(Term::TInterval(I::I0)))
        );
    }

    #[test]
    fn parses_path_application() {
        let mut parser = Parser::new(Lexer::new("p @ i0").lex().unwrap());
        parser.term_env.push("p".to_string());
        let term = parser.parse_term().unwrap();
        assert_eq!(
            term,
            Term::PApp(Box::new(Term::TVar(0)), Box::new(Term::TInterval(I::I0)))
        );
    }

    #[test]
    fn parses_nat_declaration() {
        let decls = parse_program("data Nat = | zero : Nat | suc : Nat -> Nat").unwrap();
        assert_eq!(decls.len(), 1);
        match &decls[0] {
            Decl::Data(dt) => {
                assert_eq!(dt.name, "Nat");
                assert_eq!(dt.cons.len(), 2);
                assert_eq!(dt.cons[0].name, "zero");
                assert_eq!(dt.cons[1].name, "suc");
                assert_eq!(dt.cons[1].arg_tys, vec![Term::TData("Nat".to_string())]);
            }
            _ => panic!("expected data declaration"),
        }
    }

    #[test]
    fn parses_def_then_data() {
        let src = "def main : U1 = U0\ndata Nat = | zero : Nat | suc : Nat -> Nat";
        let decls = parse_program(src).unwrap();
        assert_eq!(decls.len(), 2);
        match &decls[0] {
            Decl::Def { name, .. } => assert_eq!(name, "main"),
            _ => panic!("expected def declaration"),
        }
        match &decls[1] {
            Decl::Data(dt) => assert_eq!(dt.name, "Nat"),
            _ => panic!("expected data declaration"),
        }
    }

    #[test]
    fn parses_data_then_def() {
        let src = "data Nat = | zero : Nat | suc : Nat -> Nat\ndef main : U1 = U0";
        let decls = parse_program(src).unwrap();
        assert_eq!(decls.len(), 2);
        match &decls[0] {
            Decl::Data(dt) => assert_eq!(dt.name, "Nat"),
            _ => panic!("expected data declaration"),
        }
        match &decls[1] {
            Decl::Def { name, .. } => assert_eq!(name, "main"),
            _ => panic!("expected def declaration"),
        }
    }

    #[test]
    fn parses_two_defs() {
        let src = "def a : U0 = U0\ndef b : U0 = U0";
        let decls = parse_program(src).unwrap();
        assert_eq!(decls.len(), 2);
        match &decls[0] {
            Decl::Def { name, .. } => assert_eq!(name, "a"),
            _ => panic!("expected def declaration"),
        }
        match &decls[1] {
            Decl::Def { name, .. } => assert_eq!(name, "b"),
            _ => panic!("expected def declaration"),
        }
    }

    #[test]
    fn parses_eliminator() {
        let src = "elim motive { | zero => body0 | suc n => body1 } scrutinee";
        let mut parser = Parser::new(Lexer::new(src).lex().unwrap());
        parser.global_env = vec![
            "scrutinee".to_string(),
            "body1".to_string(),
            "body0".to_string(),
            "motive".to_string(),
        ];
        let term = parser.parse_term().unwrap();
        match term {
            Term::TElim(_, cases, _) => {
                assert_eq!(cases.len(), 2);
                assert_eq!(cases[0].con, "zero");
                assert_eq!(cases[1].con, "suc");
                assert_eq!(cases[1].binders, vec!["n".to_string()]);
            }
            _ => panic!("expected eliminator"),
        }
    }

    #[test]
    fn parses_match() {
        let src = "match n return Nat with | zero => z | suc m => s";
        let mut parser = Parser::new(Lexer::new(src).lex().unwrap());
        parser.global_env = vec![
            "s".to_string(),
            "z".to_string(),
            "Nat".to_string(),
            "n".to_string(),
        ];
        let term = parser.parse_term().unwrap();
        match term {
            Term::TElim(motive, cases, scrut) => {
                assert_eq!(*scrut, Term::TVar(3));
                assert_eq!(*motive, Term::TAbs("n".to_string(), Box::new(Term::TVar(3))));
                assert_eq!(cases.len(), 2);
                assert_eq!(cases[0].con, "zero");
                assert_eq!(cases[0].binders, Vec::<String>::new());
                assert_eq!(cases[1].con, "suc");
                assert_eq!(cases[1].binders, vec!["m".to_string()]);
            }
            _ => panic!("expected match to desugar to eliminator"),
        }
    }

    #[test]
    fn parses_match_with_braced_cases() {
        let src = "match n return Nat with { | zero => z | suc m => s }";
        let mut parser = Parser::new(Lexer::new(src).lex().unwrap());
        parser.global_env = vec![
            "s".to_string(),
            "z".to_string(),
            "Nat".to_string(),
            "n".to_string(),
        ];
        let term = parser.parse_term().unwrap();
        assert!(matches!(term, Term::TElim(_, _, _)));
    }

    #[test]
    fn parses_match_dependent_return_type() {
        let src = "match n return n with | zero => z | suc m => s";
        let mut parser = Parser::new(Lexer::new(src).lex().unwrap());
        parser.global_env = vec![
            "s".to_string(),
            "z".to_string(),
            "n".to_string(),
        ];
        let term = parser.parse_term().unwrap();
        match term {
            Term::TElim(motive, _, _) => {
                assert_eq!(
                    *motive,
                    Term::TAbs("n".to_string(), Box::new(Term::TVar(0)))
                );
            }
            _ => panic!("expected match to desugar to eliminator"),
        }
    }

    #[test]
    fn match_desugars_to_equivalent_elim() {
        let src = "match n return Nat with | zero => z | suc m => s";
        let mut parser = Parser::new(Lexer::new(src).lex().unwrap());
        parser.global_env = vec![
            "s".to_string(),
            "z".to_string(),
            "Nat".to_string(),
            "n".to_string(),
        ];
        let from_match = parser.parse_term().unwrap();

        let elim_src = "elim (\\n. Nat) { | zero => z | suc m => s } n";
        let mut elim_parser = Parser::new(Lexer::new(elim_src).lex().unwrap());
        elim_parser.global_env = vec![
            "s".to_string(),
            "z".to_string(),
            "Nat".to_string(),
            "n".to_string(),
        ];
        let from_elim = elim_parser.parse_term().unwrap();

        assert_eq!(from_match, from_elim);
    }

    #[test]
    fn parses_s1_declaration() {
        let decls = parse_program("data S1 = | base : S1 | loop : S1 [ base , base ]").unwrap();
        match &decls[0] {
            Decl::Data(dt) => {
                assert_eq!(dt.name, "S1");
                assert_eq!(dt.cons.len(), 1);
                assert_eq!(dt.pcons.len(), 1);
                assert_eq!(
                    dt.pcons[0].face0,
                    Term::TCon("S1".to_string(), "base".to_string(), vec![])
                );
            }
            _ => panic!("expected data declaration"),
        }
    }

    #[test]
    fn round_trip_with_show_term() {
        let term = parse_term("\\x. (x , x)").unwrap();
        let printed = show_term(&[], &term);
        let reparsed = parse_term(&printed).unwrap();
        assert_eq!(term, reparsed);
    }
}