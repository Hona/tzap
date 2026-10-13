//! Resolved postfix expressions avoid recursive evaluation and AST destruction.
use super::scan::identifier;

#[derive(Clone, Debug)]
enum Op {
    Value(f64),
    Parameter(usize),
    Add,
    Sub,
    Mul,
    Div,
    Neg,
}

#[derive(Clone, Debug)]
pub(super) struct Expr {
    code: Vec<Op>,
    constant: Option<f64>,
}

fn finite(value: f64) -> Result<f64, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err("angle expression must be finite".into())
    }
}

fn binary(op: &Op, a: f64, b: f64) -> Result<f64, String> {
    finite(match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
        Op::Div => {
            if b == 0.0 {
                return Err("division by zero in angle expression".into());
            }
            a / b
        }
        _ => unreachable!(),
    })
}

impl Expr {
    /// Cached constants take one step; other expressions execute all opcodes.
    pub fn work(&self) -> usize {
        if self.constant.is_some() {
            1
        } else {
            self.code.len()
        }
    }
    pub fn parse(source: &str, parameters: &[String], max_depth: usize) -> Result<Self, String> {
        let mut parser = Parser {
            source,
            pos: 0,
            parameters,
            max_depth,
            code: Vec::new(),
        };
        parser.space();
        if parser.pos == source.len() {
            return Err("empty angle expression".into());
        }
        let constant = parser.expr(0)?;
        parser.space();
        if parser.pos != source.len() {
            let c = source[parser.pos..].chars().next().unwrap();
            if !c.is_ascii_alphanumeric() && !matches!(c, '_' | '.' | '(' | ')') {
                return Err(format!("unexpected character '{c}' in angle expression"));
            }
            return Err("unexpected token in angle expression".into());
        }
        Ok(Self {
            code: parser.code,
            constant,
        })
    }

    pub fn evaluate(&self, parameters: &[f64]) -> Result<f64, String> {
        if let Some(value) = self.constant {
            return Ok(value);
        }
        let mut stack = smallvec::SmallVec::<[f64; 8]>::new();
        for op in &self.code {
            match op {
                Op::Value(v) => stack.push(*v),
                Op::Parameter(i) => stack.push(parameters[*i]),
                Op::Neg => {
                    let v = stack.last_mut().unwrap();
                    *v = -*v;
                }
                _ => {
                    let b = stack.pop().unwrap();
                    let a = stack.last_mut().unwrap();
                    *a = binary(op, *a, b)?;
                }
            }
        }
        finite(stack[0])
    }
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    parameters: &'a [String],
    max_depth: usize,
    code: Vec<Op>,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.pos < self.source.len() && self.source.as_bytes()[self.pos].is_ascii_whitespace()
        {
            self.pos += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.space();
        self.source.as_bytes().get(self.pos).copied()
    }

    fn expr(&mut self, depth: usize) -> Result<Option<f64>, String> {
        let mut value = self.term(depth)?;
        while let Some(c @ (b'+' | b'-')) = self.peek() {
            self.pos += 1;
            let rhs = self.term(depth)?;
            let op = if c == b'+' { Op::Add } else { Op::Sub };
            value = match (value, rhs) {
                (Some(a), Some(b)) => Some(binary(&op, a, b)?),
                _ => None,
            };
            self.code.push(op);
        }
        Ok(value)
    }
    fn term(&mut self, depth: usize) -> Result<Option<f64>, String> {
        let mut value = self.atom(depth)?;
        while let Some(c @ (b'*' | b'/')) = self.peek() {
            self.pos += 1;
            let rhs = self.atom(depth)?;
            let op = if c == b'*' { Op::Mul } else { Op::Div };
            if matches!(op, Op::Div) && rhs == Some(0.0) {
                return Err("division by zero in angle expression".into());
            }
            value = match (value, rhs) {
                (Some(a), Some(b)) => Some(binary(&op, a, b)?),
                _ => None,
            };
            self.code.push(op);
        }
        Ok(value)
    }
    fn atom(&mut self, depth: usize) -> Result<Option<f64>, String> {
        if depth >= self.max_depth {
            return Err("expression nesting limit exceeded".into());
        }
        let c = self.peek().ok_or("unexpected end of angle expression")?;
        match c {
            b'-' => {
                self.pos += 1;
                let value = self.atom(depth + 1)?.map(|v| -v);
                self.code.push(Op::Neg);
                Ok(value)
            }
            b'(' => {
                self.pos += 1;
                let value = self.expr(depth + 1)?;
                if self.peek() != Some(b')') {
                    return Err("unclosed parenthesis in angle expression".into());
                }
                self.pos += 1;
                Ok(value)
            }
            b'0'..=b'9' | b'.' => {
                let start = self.pos;
                let bytes = self.source.as_bytes();
                while self.pos < bytes.len()
                    && (bytes[self.pos].is_ascii_digit() || bytes[self.pos] == b'.')
                {
                    self.pos += 1;
                }
                if self.pos < bytes.len() && matches!(bytes[self.pos], b'e' | b'E') {
                    self.pos += 1;
                    if self.pos < bytes.len() && matches!(bytes[self.pos], b'+' | b'-') {
                        self.pos += 1;
                    }
                    while self.pos < bytes.len() && bytes[self.pos].is_ascii_digit() {
                        self.pos += 1;
                    }
                }
                let value = self.source[start..self.pos]
                    .parse()
                    .map_err(|e| format!("bad number: {e}"))?;
                let value = finite(value)?;
                self.code.push(Op::Value(value));
                Ok(Some(value))
            }
            c if c.is_ascii_alphabetic() => {
                let (name, _) = identifier(&self.source[self.pos..]);
                self.pos += name.len();
                if name == "pi" {
                    let value = std::f64::consts::PI;
                    self.code.push(Op::Value(value));
                    Ok(Some(value))
                } else {
                    let i = self
                        .parameters
                        .iter()
                        .position(|n| n == name)
                        .ok_or_else(|| format!("unknown parameter '{name}' in angle expression"))?;
                    self.code.push(Op::Parameter(i));
                    Ok(None)
                }
            }
            _ => Err(format!(
                "unexpected character '{}' in angle expression",
                self.source[self.pos..].chars().next().unwrap()
            )),
        }
    }
}
