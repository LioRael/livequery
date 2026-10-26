//! Bounded SQL parsing, execution and pull-based change detection.
use livequery_core::{Column, Error, Limits, Registry, Revision, Row};
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
    order: Option<(String, bool)>,
    aggregate: Option<Aggregate>,
}
#[derive(Debug, Clone)]
enum Aggregate {
    Count,
    Max(String),
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
/// Query rows with metadata sampled from the same immutable publication.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct QueryExecution {
    /// Existing result shape, flattened for wire compatibility.
    #[serde(flatten)]
    pub result: QueryResult,
    /// None for local sources; age is sampled when acquiring the snapshot.
    pub publication: Option<livequery_core::PublicationStatus>,
    /// Exact revision used to compute this result.
    pub revision: Option<Revision>,
}
impl Query {
    /// Parses SELECT, COUNT/MAX, WHERE predicates, one ORDER BY key and LIMIT.
    /// # Errors
    /// Rejects unsupported clauses, malformed SQL, excessive size or nesting.
    pub fn parse(sql: &str) -> Result<Self, Error> {
        if sql.len() > 4096 {
            return Err(Error::Limit("query bytes".into()));
        }
        let tokens = Tokenizer::new(&GenericDialect, sql)
            .tokenize_with_location()
            .map_err(|e| Error::Invalid(e.to_string()))?
            .into_iter()
            .filter(|t| !matches!(t.token, Token::Whitespace(_)))
            .collect::<Vec<_>>();
        if tokens.len() > 256 {
            return Err(Error::Limit("query tokens".into()));
        }
        let locations = tokens.iter().map(|t| t.span.start).collect::<Vec<_>>();
        let mut p = Parser {
            tokens: tokens.into_iter().map(|t| t.token).collect(),
            pos: 0,
        };
        let parsed = (|| -> Result<Self, Error> {
            p.require("SELECT")?;
            let aggregate = if p.tokens.get(p.pos + 1) == Some(&Token::LParen) && p.keyword("COUNT")
            {
                p.take(&Token::LParen);
                if !p.take(&Token::Mul) || !p.take(&Token::RParen) {
                    return Err(invalid());
                }
                Some(Aggregate::Count)
            } else if p.tokens.get(p.pos + 1) == Some(&Token::LParen) && p.keyword("MAX") {
                p.take(&Token::LParen);
                let field = p.ident()?;
                if !p.take(&Token::RParen) {
                    return Err(invalid());
                }
                Some(Aggregate::Max(field))
            } else {
                None
            };
            let projection = if aggregate.is_some() || p.take(&Token::Mul) {
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
            let order = if p.keyword("ORDER") {
                p.require("BY")?;
                let name = p.ident()?;
                let descending = p.keyword("DESC");
                if !descending {
                    p.keyword("ASC");
                }
                if aggregate.is_some() {
                    return Err(Error::Invalid(
                        "ORDER BY is not supported on aggregate results".into(),
                    ));
                }
                Some((name, descending))
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
                order,
                aggregate,
            })
        })();
        parsed.map_err(|error| match error {
            Error::Invalid(message) => {
                let location = locations
                    .get(p.pos.saturating_sub(1))
                    .or_else(|| locations.last());
                match location {
                    Some(location) => Error::Invalid(format!(
                        "{message} at line {}, column {}",
                        location.line, location.column
                    )),
                    None => Error::Invalid(format!("{message} at line 1, column 1")),
                }
            }
            other => other,
        })
    }
    /// Executes against one consistent source snapshot.
    /// # Errors
    /// Reports unknown fields even on empty tables, incompatible comparisons, and budgets.
    pub fn execute(&self, registry: &Registry, limits: Limits) -> Result<QueryResult, Error> {
        Ok(self.execute_detailed(registry, limits)?.result)
    }
    /// Executes with freshness and revision metadata bound to the actual read version.
    /// # Errors
    /// Returns the same validation and resource errors as `execute`.
    pub fn execute_detailed(
        &self,
        registry: &Registry,
        limits: Limits,
    ) -> Result<QueryExecution, Error> {
        let start = Instant::now();
        let view = registry.read_snapshot(&self.source, limits)?;
        let snapshot = view.snapshot();
        let find = |name: &str| {
            snapshot
                .columns
                .iter()
                .find(|c| c.name == name)
                .cloned()
                .ok_or_else(|| unknown_field(name, &snapshot.columns))
        };
        if let Some((name, _)) = &self.order {
            find(name)?;
        }
        if let Some(Aggregate::Max(name)) = &self.aggregate {
            find(name)?;
        }
        let columns = match &self.aggregate {
            Some(Aggregate::Count) => vec![Column::new::<u64>("count")],
            Some(Aggregate::Max(_)) => vec![Column::new::<Option<Value>>("max")],
            None => match &self.projection {
                Some(names) => names
                    .iter()
                    .map(|n| find(n))
                    .collect::<Result<Vec<_>, _>>()?,
                None => snapshot.columns.clone(),
            },
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
        let mut matching = Vec::new();
        let mut count = 0u64;
        let mut maximum: Option<Value> = None;
        let mut sort_type: Option<&Value> = None;
        for row in &snapshot.rows {
            if start.elapsed() >= limits.query_timeout {
                return Err(Error::Deadline);
            }
            if self
                .filter
                .as_ref()
                .map(|e| e.eval(row))
                .transpose()?
                .unwrap_or(Some(true))
                != Some(true)
            {
                continue;
            }
            count += 1;
            if let Some(Aggregate::Max(name)) = &self.aggregate {
                let value = &row[name];
                if !value.is_null() {
                    compare(value, value)?;
                    if maximum
                        .as_ref()
                        .map(|old| compare(value, old).map(|c| c == Ordering::Greater))
                        .transpose()?
                        .unwrap_or(true)
                    {
                        maximum = Some(value.clone());
                    }
                }
            }
            if self.aggregate.is_some() {
                continue;
            }
            if let Some((name, _)) = &self.order {
                let value = &row[name];
                if !value.is_null() {
                    compare(value, sort_type.unwrap_or(value))?;
                    sort_type = Some(value);
                }
                matching.push(row);
            } else if matching.len() < limit {
                matching.push(row);
            }
        }
        if let Some((name, descending)) = &self.order {
            matching.sort_by(|a, b| match (a[name].is_null(), b[name].is_null()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    let order =
                        compare(&a[name], &b[name]).expect("sort scalar types were validated");
                    if *descending { order.reverse() } else { order }
                }
            });
        }
        if let Some(aggregate) = &self.aggregate {
            result.truncated = limit == 0;
            if limit > 0 {
                result.rows.push(match aggregate {
                    Aggregate::Count => {
                        [("count".into(), Value::from(count))].into_iter().collect()
                    }
                    Aggregate::Max(_) => [("max".into(), maximum.unwrap_or(Value::Null))]
                        .into_iter()
                        .collect(),
                });
            }
        } else {
            result.truncated = count as usize > limit;
            result.rows = matching
                .into_iter()
                .take(limit)
                .map(|row| {
                    result
                        .columns
                        .iter()
                        .map(|column| (column.name.clone(), row[&column.name].clone()))
                        .collect()
                })
                .collect();
        }
        livequery_core::json_bytes(&result, limits.bytes)?;
        if start.elapsed() >= limits.query_timeout {
            return Err(Error::Deadline);
        }
        Ok(QueryExecution {
            result,
            publication: view.publication().cloned(),
            revision: view.revision(),
        })
    }
    /// Publication revision, or none for a live local source.
    /// # Errors
    /// Returns missing-source or busy errors.
    pub fn revision(&self, registry: &Registry) -> Result<Option<Revision>, Error> {
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
                    Err(unknown_field(name, columns))
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
            let integer = |number: &serde_json::Number| {
                number
                    .as_i64()
                    .map(i128::from)
                    .or_else(|| number.as_u64().map(i128::from))
            };
            if let (Some(a), Some(b)) = (integer(a), integer(b)) {
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
fn unknown_field(name: &str, columns: &[Column]) -> Error {
    Error::Invalid(format!(
        "unknown field: {name}; available fields: {}",
        columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}
fn invalid() -> Error {
    Error::Invalid("expected SELECT fields FROM source [WHERE predicate] [ORDER BY field [ASC|DESC]] [LIMIT integer]".into())
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
    revision: Option<Revision>,
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
