//! A small, safe arithmetic expression evaluator (§6).
//!
//! Grammar (no arbitrary code — just numbers, global references, and the four
//! operators with parentheses and unary minus):
//!
//! ```text
//! expr    := term (('+' | '-') term)*
//! term    := factor (('*' | '/') factor)*
//! factor  := '-' factor | '(' expr ')' | number | ident
//! ```
//!
//! `ident`s resolve to [`crate::model::Global`] values via a caller-supplied
//! resolver, which is where cycle/missing-reference detection happens (the
//! resolver returns an error and the calc engine surfaces it as `#ERR`).

use rust_decimal::Decimal;
use std::str::FromStr;

/// Errors from parsing or evaluating an expression.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("unknown global reference: {0}")]
    UnknownRef(String),
    #[error("reference cycle through: {0}")]
    Cycle(String),
    #[error("division by zero")]
    DivByZero,
    #[error("numeric overflow")]
    Overflow,
}

/// Parsed expression tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Num(Decimal),
    Ref(String),
    Neg(Box<Ast>),
    Add(Box<Ast>, Box<Ast>),
    Sub(Box<Ast>, Box<Ast>),
    Mul(Box<Ast>, Box<Ast>),
    Div(Box<Ast>, Box<Ast>),
}

impl Ast {
    /// Collect every global name referenced by this expression (for building
    /// the dependency graph).
    pub fn refs(&self, out: &mut Vec<String>) {
        match self {
            Ast::Num(_) => {}
            Ast::Ref(name) => out.push(name.clone()),
            Ast::Neg(a) => a.refs(out),
            Ast::Add(a, b) | Ast::Sub(a, b) | Ast::Mul(a, b) | Ast::Div(a, b) => {
                a.refs(out);
                b.refs(out);
            }
        }
    }

    /// Evaluate, resolving identifiers through `resolve`.
    pub fn eval<F>(&self, resolve: &F) -> Result<Decimal, EvalError>
    where
        F: Fn(&str) -> Result<Decimal, EvalError>,
    {
        // All arithmetic is checked: `rust_decimal`'s `+ - * /` panic on
        // overflow, but `eval` must be total (callers turn `Err` into `#ERR`).
        Ok(match self {
            Ast::Num(n) => *n,
            Ast::Ref(name) => resolve(name)?,
            Ast::Neg(a) => a
                .eval(resolve)?
                .checked_mul(Decimal::NEGATIVE_ONE)
                .ok_or(EvalError::Overflow)?,
            Ast::Add(a, b) => a
                .eval(resolve)?
                .checked_add(b.eval(resolve)?)
                .ok_or(EvalError::Overflow)?,
            Ast::Sub(a, b) => a
                .eval(resolve)?
                .checked_sub(b.eval(resolve)?)
                .ok_or(EvalError::Overflow)?,
            Ast::Mul(a, b) => a
                .eval(resolve)?
                .checked_mul(b.eval(resolve)?)
                .ok_or(EvalError::Overflow)?,
            Ast::Div(a, b) => {
                let d = b.eval(resolve)?;
                if d.is_zero() {
                    return Err(EvalError::DivByZero);
                }
                a.eval(resolve)?.checked_div(d).ok_or(EvalError::Overflow)?
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(Decimal),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

fn lex(src: &str) -> Result<Vec<Tok>, EvalError> {
    let mut toks = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ws if ws.is_whitespace() => i += 1,
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                toks.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            d if d.is_ascii_digit() || d == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                let n = Decimal::from_str(&s)
                    .map_err(|_| EvalError::Syntax(format!("bad number '{s}'")))?;
                // `Decimal` caps scale at 28 digits, so an ultra-small literal
                // like `0.0…01` (>28 fractional digits) silently rounds to 0.
                // Reject rather than mislead (a nonzero source becoming 0 would
                // turn `1 / x` into a spurious DivByZero).
                if n.is_zero() && s.bytes().any(|b| b.is_ascii_digit() && b != b'0') {
                    return Err(EvalError::Syntax(format!(
                        "number '{s}' underflows decimal precision (28 dp) to zero"
                    )));
                }
                toks.push(Tok::Num(n));
            }
            a if a.is_alphabetic() || a == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                toks.push(Tok::Ident(s));
            }
            other => return Err(EvalError::Syntax(format!("unexpected character '{other}'"))),
        }
    }
    Ok(toks)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self) -> Result<Ast, EvalError> {
        let mut left = self.term()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Plus => {
                    self.next();
                    left = Ast::Add(Box::new(left), Box::new(self.term()?));
                }
                Tok::Minus => {
                    self.next();
                    left = Ast::Sub(Box::new(left), Box::new(self.term()?));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Ast, EvalError> {
        let mut left = self.factor()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Star => {
                    self.next();
                    left = Ast::Mul(Box::new(left), Box::new(self.factor()?));
                }
                Tok::Slash => {
                    self.next();
                    left = Ast::Div(Box::new(left), Box::new(self.factor()?));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn factor(&mut self) -> Result<Ast, EvalError> {
        match self.next() {
            Some(Tok::Minus) => Ok(Ast::Neg(Box::new(self.factor()?))),
            Some(Tok::Num(n)) => Ok(Ast::Num(n)),
            Some(Tok::Ident(s)) => Ok(Ast::Ref(s)),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err(EvalError::Syntax("expected ')'".into())),
                }
            }
            other => Err(EvalError::Syntax(format!("unexpected token: {other:?}"))),
        }
    }
}

/// Parse a source expression into an [`Ast`].
pub fn parse(src: &str) -> Result<Ast, EvalError> {
    let toks = lex(src)?;
    if toks.is_empty() {
        return Err(EvalError::Syntax("empty expression".into()));
    }
    let mut p = Parser { toks, pos: 0 };
    let ast = p.expr()?;
    if p.pos != p.toks.len() {
        return Err(EvalError::Syntax("trailing tokens".into()));
    }
    Ok(ast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    fn eval_with(src: &str, vars: &[(&str, Decimal)]) -> Result<Decimal, EvalError> {
        let map: HashMap<String, Decimal> = vars.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let ast = parse(src)?;
        ast.eval(&|name: &str| {
            map.get(name)
                .copied()
                .ok_or_else(|| EvalError::UnknownRef(name.to_string()))
        })
    }

    #[test]
    fn precedence_and_parens() {
        assert_eq!(eval_with("2 + 3 * 4", &[]).unwrap(), dec!(14));
        assert_eq!(eval_with("(2 + 3) * 4", &[]).unwrap(), dec!(20));
        assert_eq!(eval_with("-5 + 2", &[]).unwrap(), dec!(-3));
    }

    #[test]
    fn resolves_globals() {
        assert_eq!(
            eval_with("SHOOT_DAYS * 1.2", &[("SHOOT_DAYS", dec!(30))]).unwrap(),
            dec!(36.0)
        );
    }

    #[test]
    fn unknown_ref_errors() {
        assert!(matches!(
            eval_with("FOO + 1", &[]),
            Err(EvalError::UnknownRef(_))
        ));
    }

    #[test]
    fn div_by_zero_errors() {
        assert_eq!(eval_with("1 / 0", &[]), Err(EvalError::DivByZero));
    }

    #[test]
    fn collects_refs() {
        let ast = parse("A * (B + 2) - C").unwrap();
        let mut refs = Vec::new();
        ast.refs(&mut refs);
        refs.sort();
        assert_eq!(refs, vec!["A", "B", "C"]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("2 +").is_err());
        assert!(parse("").is_err());
        assert!(parse("2 2").is_err());
    }
}
