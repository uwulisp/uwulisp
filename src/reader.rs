use crate::expr::Expr;

/// A single lexical token. Replaces the old "everything is a String" token
/// stream so that string literals can be told apart from bare symbols/
/// numbers that happen to contain the same characters (e.g. the symbol `x`
/// vs. the string "x").
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    LParen,
    RParen,
    Quote,
    Str(String),
    Atom(String),
    Backtick,
    Unquote,
    UnquoteSplice,
}

/// Scans source text into a token stream.
///
/// This is a real character-by-character scanner (not a global
/// find-and-replace) because that's what's required to support:
///   - line comments (`;` ... to end of line) — we need to know where a
///     line *ends*, which a whitespace-split approach has no concept of.
///   - string literals (`"..."`) — parens, quotes, semicolons, and
///     newlines inside a string must NOT be treated as syntax; a string
///     can also legitimately span multiple lines.
///   - multi-line input in general — whitespace (including newlines) is
///     simply skipped between tokens, so a single top-level expression,
///     or a single string, can freely span as many lines as it likes.
pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        match c {
            // Whitespace of any kind (spaces, tabs, newlines, carriage
            // returns) just separates tokens — including across lines.
            c if c.is_whitespace() => {
                i += 1;
            }
            // Line comment: skip from `;` through end of line (or EOF).
            ';' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '\'' => {
                tokens.push(Token::Quote);
                i += 1;
            }
            '"' => {
                i += 1; // consume opening quote
                let mut s = String::new();
                loop {
                    if i >= chars.len() {
                        return Err("unterminated string literal".into());
                    }
                    match chars[i] {
                        '"' => {
                            i += 1; // consume closing quote
                            break;
                        }
                        '\\' => {
                            i += 1;
                            let escaped = *chars.get(i).ok_or("unterminated string literal")?;
                            s.push(match escaped {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                '"' => '"',
                                '\\' => '\\',
                                other => other, // unknown escape: keep as-is
                            });
                            i += 1;
                        }
                        ch => {
                            // Newlines fall through here too, so a string
                            // literal may span multiple lines.
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                tokens.push(Token::Str(s));
            }
            '`' => {
                tokens.push(Token::Backtick);
                i += 1;
            }
            ',' => {
                i += 1;
                if chars.get(i) == Some(&'@') {
                    tokens.push(Token::UnquoteSplice);
                    i += 1;
                } else {
                    tokens.push(Token::Unquote);
                }
            }
            _ => {
                // Bare atom (symbol or number): read until the next
                // delimiter.
                let mut s = String::new();
                while i < chars.len() {
                    let ch = chars[i];
                    if ch.is_whitespace()
                        || ch == '('
                        || ch == ')'
                        || ch == '\''
                        || ch == '"'
                        || ch == ';'
                    {
                        break;
                    }
                    s.push(ch);
                    i += 1;
                }
                tokens.push(Token::Atom(s));
            }
        }
    }

    Ok(tokens)
}

/// Parses a single expression starting at `*pos`, advancing `*pos` past it.
pub fn parse(tokens: &[Token], pos: &mut usize) -> Result<Expr, String> {
    let tok = tokens.get(*pos).ok_or("unexpected EOF")?.clone();
    *pos += 1;
    match tok {
        Token::LParen => {
            let mut list = Vec::new();
            loop {
                match tokens.get(*pos) {
                    Some(Token::RParen) => {
                        *pos += 1;
                        break;
                    }
                    None => return Err("unexpected EOF in list".into()),
                    _ => list.push(parse(tokens, pos)?),
                }
            }
            Ok(Expr::List(list))
        }
        Token::RParen => Err("unexpected )".into()),
        Token::Quote => {
            // 'expr  =>  (quote expr)
            let inner = parse(tokens, pos)?;
            Ok(Expr::List(vec![Expr::Symbol("quote".into()), inner]))
        }
        Token::Str(s) => Ok(Expr::Str(s)),
        Token::Atom(s) => {
            if s == "#t" {
                Ok(Expr::Bool(true))
            } else if s == "#f" {
                Ok(Expr::Bool(false))
            } else if let Ok(n) = s.parse::<i64>() {
                Ok(Expr::Int(n))
            } else if let Ok(n) = s.parse::<f64>() {
                Ok(Expr::Float(n))
            } else {
                Ok(Expr::Symbol(s))
            }
        }
        Token::Backtick => {
            let inner = parse(tokens, pos)?;
            Ok(Expr::List(vec![Expr::Symbol("quasiquote".into()), inner]))
        }
        Token::Unquote => {
            let inner = parse(tokens, pos)?;
            Ok(Expr::List(vec![Expr::Symbol("unquote".into()), inner]))
        }
        Token::UnquoteSplice => {
            let inner = parse(tokens, pos)?;
            Ok(Expr::List(vec![
                Expr::Symbol("unquote-splicing".into()),
                inner,
            ]))
        }
    }
}

/// Parses an entire source string into a sequence of top-level expressions.
/// Top-level forms, strings, and comments may all freely span multiple
/// lines — only the surrounding parens (or a closing quote, for strings)
/// determine where a form ends.
pub fn parse_all(src: &str) -> Result<Vec<Expr>, String> {
    let tokens = tokenize(src)?;
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < tokens.len() {
        exprs.push(parse(&tokens, &mut pos)?);
    }
    Ok(exprs)
}

/// Convenience helper: parses params list `(a b c)` into Vec<String>.
pub fn parse_params(e: &Expr) -> Result<Vec<String>, String> {
    if let Expr::List(p) = e {
        p.iter()
            .map(|e| match e {
                Expr::Symbol(s) => Ok(s.clone()),
                other => Err(format!("parameter must be a symbol, got: {:?}", other)),
            })
            .collect()
    } else {
        Err(format!("expected a parameter list, got: {:?}", e))
    }
}
