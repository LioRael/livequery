use crate::{Column, Error, Limits, Registry, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlparser::{
    dialect::GenericDialect,
    tokenizer::{Token, Tokenizer},
};
use std::{cmp::Ordering, time::Instant};

/// A validated, finite SELECT query; unsupported SQL is always rejected.
#[derive(Debug, Clone)]
pub struct Query {
    source: String,
    projection: Option<Vec<String>>,
    filter: Option<Expr>,
    limit: Option<usize>,
}
#[derive(Debug, Clone)]
enum Expr {
    Compare(String, Token, Value),
    Null(String, bool),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}
/// An owned result; column order follows the requested projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    /// Projected fields.
    pub columns: Vec<Column>,
    /// Matching records, bounded by the query and server limits.
    pub rows: Vec<Row>,
    /// Total snapshot rows scanned.
    pub scanned: usize,
    /// True if matching rows were omitted by the output limit.
    pub truncated: bool,
}
impl Query {
    /// Parses SELECT, projection, WHERE comparisons, AND/OR/NOT, IS NULL and LIMIT.
    /// # Errors
    /// Rejects unsupported clauses, malformed SQL, excessive size or nesting.
    pub fn parse(sql: &str) -> Result<Self, Error> {
        if sql.len() > 4096 {
            return Err(Error::Limit("query bytes".into()));
        }
        let tokens = Tokenizer::new(&GenericDialect, sql)
            .tokenize()
            .map_err(|e| Error::Invalid(e.to_string()))?
            .into_iter()
            .filter(|t| !matches!(t, Token::Whitespace(_)))
            .collect::<Vec<_>>();
        if tokens.len() > 256 {
            return Err(Error::Limit("query tokens".into()));
        }
        let mut p = Parser { tokens, pos: 0 };
        p.require("SELECT")?;
        let projection = if p.take(&Token::Mul) {
            None
        } else {
            let mut fields = vec![p.ident()?];
            while p.take(&Token::Comma) {
                fields.push(p.ident()?);
            }
            if fields
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != fields.len()
            {
                return Err(invalid());
            }
            Some(fields)
        };
        p.require("FROM")?;
        let source = p.ident()?;
        let filter = if p.keyword("WHERE") {
            Some(p.expr(0)?)
        } else {
            None
        };
        let limit = if p.keyword("LIMIT") {
            match p.next() {
                Some(Token::Number(n, false)) => Some(n.parse().map_err(|_| invalid())?),
                _ => return Err(invalid()),
            }
        } else {
            None
        };
        p.take(&Token::SemiColon);
        if p.pos != p.tokens.len() {
            return Err(invalid());
        }
        Ok(Self {
            source,
            projection,
            filter,
            limit,
        })
    }
    /// Executes against one consistent source snapshot.
    /// # Errors
    /// Reports unknown fields even on empty tables, incompatible comparisons, and budgets.
    pub fn execute(&self, registry: &Registry, limits: Limits) -> Result<QueryResult, Error> {
        let start = Instant::now();
        let snapshot = registry.snapshot(&self.source, limits)?;
        let find = |name: &str| {
            snapshot
                .columns
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| Error::Invalid(format!("unknown field: {name}")))
        };
        let columns = match &self.projection {
            Some(names) => names
                .iter()
                .map(|n| find(n))
                .collect::<Result<Vec<_>, _>>()?,
            None => snapshot.columns.clone(),
        };
        if let Some(expr) = &self.filter {
            expr.validate(&snapshot.columns)?;
        }
        let limit = self.limit.unwrap_or(limits.result_rows);
        if limit > limits.result_rows {
            return Err(Error::Limit("result rows".into()));
        }
        let mut result = QueryResult {
            columns,
            rows: Vec::new(),
            scanned: snapshot.rows.len(),
            truncated: false,
        };
        for row in snapshot.rows {
            if start.elapsed() >= limits.query_timeout {
                return Err(Error::Deadline);
            }
            if self
                .filter
                .as_ref()
                .map(|e| e.eval(&row))
                .transpose()?
                .unwrap_or(Some(true))
                != Some(true)
            {
                continue;
            }
            if result.rows.len() == limit {
                result.truncated = true;
                continue;
            }
            result.rows.push(
                result
                    .columns
                    .iter()
                    .map(|c| (c.name.clone(), row[&c.name].clone()))
                    .collect(),
            );
        }
        if serde_json::to_vec(&result)?.len() > limits.bytes {
            return Err(Error::Limit("result bytes".into()));
        }
        if start.elapsed() >= limits.query_timeout {
            return Err(Error::Deadline);
        }
        Ok(result)
    }
    /// Publication revision, or none for a live local source.
    /// # Errors
    /// Returns missing-source or busy errors.
    pub fn revision(&self, registry: &Registry) -> Result<Option<crate::Revision>, Error> {
        registry.revision(&self.source)
    }
}
impl Expr {
    fn validate(&self, columns: &[Column]) -> Result<(), Error> {
        match self {
            Self::Compare(name, _, _) | Self::Null(name, _) => {
                if columns.iter().any(|c| &c.name == name) {
                    Ok(())
                } else {
                    Err(Error::Invalid(format!("unknown field: {name}")))
                }
            }
            Self::And(a, b) | Self::Or(a, b) => {
                a.validate(columns)?;
                b.validate(columns)
            }
            Self::Not(a) => a.validate(columns),
        }
    }
    fn eval(&self, row: &Row) -> Result<Option<bool>, Error> {
        Ok(match self {
            Self::Compare(name, op, value) => {
                let actual = &row[name];
                if actual.is_null() || value.is_null() {
                    return Ok(None);
                }
                let ordering = compare(actual, value)?;
                Some(match op {
                    Token::Eq => ordering == Ordering::Equal,
                    Token::Neq => ordering != Ordering::Equal,
                    Token::Lt => ordering == Ordering::Less,
                    Token::Gt => ordering == Ordering::Greater,
                    Token::LtEq => ordering != Ordering::Greater,
                    Token::GtEq => ordering != Ordering::Less,
                    _ => return Err(invalid()),
                })
            }
            Self::Null(name, negate) => Some(row[name].is_null() != *negate),
            Self::Not(a) => a.eval(row)?.map(|v| !v),
            Self::And(a, b) => match (a.eval(row)?, b.eval(row)?) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            Self::Or(a, b) => match (a.eval(row)?, b.eval(row)?) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            },
        })
    }
}
fn compare(a: &Value, b: &Value) -> Result<Ordering, Error> {
    match (a, b) {
        (Value::String(a), Value::String(b)) => Ok(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
        (Value::Number(a), Value::Number(b)) => {
            // Integers retain all 64 bits; never round an identifier through f64.
            if let (Ok(a), Ok(b)) = (a.to_string().parse::<i128>(), b.to_string().parse::<i128>()) {
                return Ok(a.cmp(&b));
            }
            if a.is_f64() && b.is_f64() {
                return a
                    .as_f64()
                    .and_then(|a| b.as_f64().and_then(|b| a.partial_cmp(&b)))
                    .ok_or_else(invalid);
            }
            Err(Error::Invalid(
                "mixed integer/float comparison; use a matching literal type".into(),
            ))
        }
        _ => Err(Error::Invalid(
            "comparison requires matching scalar types".into(),
        )),
    }
}
fn invalid() -> Error {
    Error::Invalid("expected SELECT fields FROM source [WHERE predicate] [LIMIT integer]".into())
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}
impl Parser {
    fn next(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(t)
    }
    fn take(&mut self, token: &Token) -> bool {
        if self.tokens.get(self.pos) == Some(token) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn keyword(&mut self, word: &str) -> bool {
        if matches!(self.tokens.get(self.pos), Some(Token::Word(w)) if w.quote_style.is_none() && w.value.eq_ignore_ascii_case(word))
        {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, word: &str) -> Result<(), Error> {
        if self.keyword(word) {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    fn ident(&mut self) -> Result<String, Error> {
        match self.next() {
            Some(Token::Word(w)) => Ok(w.value),
            _ => Err(invalid()),
        }
    }
    fn expr(&mut self, depth: usize) -> Result<Expr, Error> {
        let mut value = self.and(depth)?;
        while self.keyword("OR") {
            value = Expr::Or(Box::new(value), Box::new(self.and(depth)?));
        }
        Ok(value)
    }
    fn and(&mut self, depth: usize) -> Result<Expr, Error> {
        let mut value = self.atom(depth)?;
        while self.keyword("AND") {
            value = Expr::And(Box::new(value), Box::new(self.atom(depth)?));
        }
        Ok(value)
    }
    fn atom(&mut self, depth: usize) -> Result<Expr, Error> {
        if depth >= 32 {
            return Err(Error::Limit("predicate nesting".into()));
        }
        if self.keyword("NOT") {
            return Ok(Expr::Not(Box::new(self.atom(depth + 1)?)));
        }
        if self.take(&Token::LParen) {
            let expr = self.expr(depth + 1)?;
            if !self.take(&Token::RParen) {
                return Err(invalid());
            }
            return Ok(expr);
        }
        let name = self.ident()?;
        if self.keyword("IS") {
            let negate = self.keyword("NOT");
            self.require("NULL")?;
            return Ok(Expr::Null(name, negate));
        }
        let op = self.next().ok_or_else(invalid)?;
        if !matches!(
            op,
            Token::Eq | Token::Neq | Token::Lt | Token::Gt | Token::LtEq | Token::GtEq
        ) {
            return Err(invalid());
        }
        let negative = self.take(&Token::Minus);
        let value = match self.next() {
            Some(Token::Number(n, false)) => {
                let literal = format!("{}{n}", if negative { "-" } else { "" });
                if n.contains(['.', 'e', 'E']) {
                    serde_json::from_str::<Value>(&literal)?
                } else if negative {
                    Value::from(
                        literal
                            .parse::<i64>()
                            .map_err(|_| Error::Invalid("integer literal out of range".into()))?,
                    )
                } else {
                    Value::from(
                        n.parse::<u64>()
                            .map_err(|_| Error::Invalid("integer literal out of range".into()))?,
                    )
                }
            }
            Some(Token::SingleQuotedString(s)) if !negative => Value::String(s),
            Some(Token::Word(w)) if !negative && w.quote_style.is_none() => {
                match w.value.to_ascii_uppercase().as_str() {
                    "TRUE" => Value::Bool(true),
                    "FALSE" => Value::Bool(false),
                    "NULL" => Value::Null,
                    _ => return Err(invalid()),
                }
            }
            _ => return Err(invalid()),
        };
        Ok(Expr::Compare(name, op, value))
    }
}
/// Pull-based change detection with one retained result and no background tasks.
#[derive(Debug)]
pub struct Watch {
    query: Query,
    previous: Option<QueryResult>,
    revision: Option<crate::Revision>,
    limits: Option<Limits>,
}
impl Watch {
    /// Starts an observation; the first poll always emits a full result.
    pub fn new(query: Query) -> Self {
        Self {
            query,
            previous: None,
            revision: None,
            limits: None,
        }
    }
    /// Returns a full replacement result only when rows or schema change.
    /// # Errors
    /// Propagates query errors without changing the last successful observation.
    pub fn poll(
        &mut self,
        registry: &Registry,
        limits: Limits,
    ) -> Result<Option<QueryResult>, Error> {
        let revision = self.query.revision(registry)?;
        if revision.is_some() && revision == self.revision && self.limits == Some(limits) {
            return Ok(None);
        }
        let next = self.query.execute(registry, limits)?;
        self.revision = revision;
        self.limits = Some(limits);
        let changed = self.previous.as_ref().is_none_or(|old| {
            old.rows != next.rows || old.columns != next.columns || old.truncated != next.truncated
        });
        if !changed {
            return Ok(None);
        }
        self.previous = Some(next.clone());
        Ok(Some(next))
    }
}
