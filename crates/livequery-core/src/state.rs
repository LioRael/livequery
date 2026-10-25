use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    sync::{
        Arc, RwLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// A named record containing only explicitly exposed fields.
pub type Row = BTreeMap<String, Value>;
/// A queryable field and its declared Rust type, available even for empty sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    /// Stable field identifier.
    pub name: String,
    /// Rust type spelling, including collection and option wrappers.
    pub rust_type: String,
    /// JSON Schema for this field, including recursive definitions and enum variants.
    #[serde(default)]
    pub schema: Value,
}
impl Column {
    /// Describes a field without inspecting a live value.
    pub fn new<T: schemars::JsonSchema>(name: &str) -> Self {
        Self {
            name: name.into(),
            rust_type: std::any::type_name::<T>().into(),
            schema: serde_json::to_value(schemars::schema_for!(T))
                .expect("JSON Schema contains only JSON values"),
        }
    }
}
/// Failures reported by inspection, query validation, and local transport.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Invalid or unsupported query or schema.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// A resource budget was exceeded; partial results are never returned.
    #[error("resource limit exceeded: {0}")]
    Limit(String),
    /// The source is currently locked by an application writer.
    #[error("source busy; retry later")]
    Busy,
    /// A bounded capture or query exhausted its elapsed-time budget.
    #[error("query deadline exceeded")]
    Deadline,
    /// A requested source is not registered.
    #[error("source not found: {0}")]
    NotFound(String),
    /// Serialization failed.
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    /// Local transport failed.
    #[error("transport failed: {0}")]
    Io(#[from] std::io::Error),
}
/// Finite budgets used by both custom and built-in sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum rows scanned; larger sources fail, even with a small LIMIT.
    pub scan_rows: usize,
    /// Maximum result rows, including queries without LIMIT.
    pub result_rows: usize,
    /// Maximum serialized snapshot and response bytes.
    pub bytes: usize,
    /// Cooperative time budget for capture and query loops. Remote queries run only on published data.
    pub query_timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            scan_rows: 10_000,
            result_rows: 1_000,
            bytes: 1_048_576,
            query_timeout: Duration::from_millis(100),
        }
    }
}
/// Explicit conversion of application data into safe, owned query records.
pub trait Inspect: Send + Sync {
    /// Returns the declared fields without requiring any live instance.
    fn schema() -> Vec<Column>;
    /// Copies exposed fields into an owned record.
    /// # Errors
    /// Returns serialization errors; implementations must be bounded and nonblocking.
    fn inspect(&self) -> Result<Row, Error>;
    /// Builds a row within a byte budget; derived implementations bound serialization allocation.
    /// # Errors
    /// Returns budget or serialization errors. Manual implementations should override this
    /// method to enforce limits before allocating; the default validates after inspection.
    fn inspect_bounded(&self, max_bytes: usize) -> Result<Row, Error> {
        let row = self.inspect()?;
        if serde_json::to_vec(&row)?.len() > max_bytes {
            return Err(Error::Limit("row bytes".into()));
        }
        Ok(row)
    }
}
/// One consistent source snapshot, owned independently of application locks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Source schema.
    pub columns: Vec<Column>,
    /// Records copied at one source-defined consistency boundary.
    pub rows: Vec<Row>,
}
/// A trusted application adapter; remote users cannot register or invoke arbitrary code.
pub trait Source: Send + Sync + fmt::Debug {
    /// Returns fields exposed by this adapter.
    fn schema(&self) -> Vec<Column>;
    /// Returns a bounded snapshot without waiting for application locks.
    /// # Errors
    /// Returns busy, budget, or serialization failures. Custom implementations must
    /// enforce budgets during construction and return promptly; they cannot be preempted.
    fn snapshot(&self, limits: Limits) -> Result<Snapshot, Error>;
}
impl Snapshot {
    /// Captures records on the caller's thread, checking budgets between records.
    /// # Errors
    /// Rejects row, byte or time limit violations and serialization failures.
    pub fn capture<'a, T: Inspect + 'a>(
        rows: impl IntoIterator<Item = &'a T>,
        limits: Limits,
    ) -> Result<Self, Error> {
        let start = Instant::now();
        let mut snapshot = Self {
            columns: T::schema(),
            rows: Vec::new(),
        };
        let mut bytes = json_bytes(&snapshot.columns, limits.bytes)?.len();
        for row in rows {
            if start.elapsed() >= limits.query_timeout {
                return Err(Error::Deadline);
            }
            if snapshot.rows.len() == limits.scan_rows {
                return Err(Error::Limit("scan rows".into()));
            }
            let row = row.inspect_bounded(limits.bytes.saturating_sub(bytes))?;
            bytes += json_bytes(&row, limits.bytes.saturating_sub(bytes))?.len();
            snapshot.rows.push(row);
        }
        snapshot.validate(limits)?;
        if start.elapsed() >= limits.query_timeout {
            return Err(Error::Deadline);
        }
        Ok(snapshot)
    }
    pub(crate) fn validate(&self, limits: Limits) -> Result<usize, Error> {
        if self.rows.len() > limits.scan_rows {
            return Err(Error::Limit("scan rows".into()));
        }
        let bytes = json_bytes(self, limits.bytes)?;
        // Parsing validates nesting before any remote request can clone a published value.
        let _: serde_json::Value = serde_json::from_slice(&bytes)?;
        let mut names = std::collections::BTreeSet::new();
        if self.columns.is_empty()
            || self
                .columns
                .iter()
                .any(|c| !identifier(&c.name) || !names.insert(&c.name))
            || self.rows.iter().any(|row| {
                row.len() != self.columns.len()
                    || self.columns.iter().any(|c| !row.contains_key(&c.name))
            })
        {
            return Err(Error::Invalid("source returned inconsistent schema".into()));
        }
        Ok(bytes.len())
    }
}
/// An opaque source identity and publication sequence used for conditional observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    registration: u64,
    sequence: u64,
}
#[derive(Debug, Clone, Copy, Default)]
enum Access {
    #[default]
    Local,
    Published,
}
#[derive(Debug)]
struct Entry {
    source: Arc<dyn Source>,
    columns: Vec<Column>,
    published: Option<crate::Published>,
    id: u64,
}
/// A process-local catalog; clones refer to the same registrations.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: Arc<RwLock<BTreeMap<String, Weak<Entry>>>>,
    access: Access,
}
/// Keeps one registration alive; dropping it removes the source from discovery.
#[derive(Debug)]
#[must_use = "dropping the guard unregisters the source"]
pub struct Registration {
    _entry: Arc<Entry>,
}
impl Registry {
    /// Creates an empty registry with no background work.
    pub fn new() -> Self {
        Self::default()
    }
    /// Exposes one source for trusted local queries until its registration guard is dropped.
    /// # Errors
    /// Rejects duplicate or invalid identifiers and invalid field schemas.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn expose(&self, name: &str, source: impl Source + 'static) -> Result<Registration, Error> {
        self.register(name, Arc::new(source), None)
    }
    /// Registers published data for both local queries and the remote agent.
    /// # Errors
    /// Rejects duplicate names, invalid schemas or exhausted registration identifiers.
    pub fn expose_published(
        &self,
        name: &str,
        published: crate::Published,
    ) -> Result<Registration, Error> {
        self.register(name, Arc::new(published.clone()), Some(published))
    }
    /// Returns a catalog view restricted to explicitly published sources.
    pub fn published_only(&self) -> Self {
        Self {
            access: Access::Published,
            ..self.clone()
        }
    }
    fn register(
        &self,
        name: &str,
        source: Arc<dyn Source>,
        published: Option<crate::Published>,
    ) -> Result<Registration, Error> {
        if !identifier(name) {
            return Err(Error::Invalid("source identifier".into()));
        }
        let schema = source.schema();
        let mut names = std::collections::BTreeSet::new();
        if schema.is_empty()
            || schema
                .iter()
                .any(|c| !identifier(&c.name) || !names.insert(&c.name))
        {
            return Err(Error::Invalid("empty, duplicate or invalid fields".into()));
        }
        let mut entries = self.entries.write().expect("registry lock poisoned");
        entries.retain(|_, entry| entry.strong_count() > 0);
        if entries.contains_key(name) {
            return Err(Error::Invalid("duplicate source".into()));
        }
        // Registration identity is process-wide so Watch can move between registries.
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = loop {
            let id = NEXT_ID.load(Ordering::Relaxed);
            let next = id
                .checked_add(1)
                .ok_or_else(|| Error::Limit("registration identifiers".into()))?;
            if NEXT_ID
                .compare_exchange_weak(id, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break id;
            }
        };
        let entry = Arc::new(Entry {
            source,
            columns: schema,
            published,
            id,
        });
        entries.insert(name.to_owned(), Arc::downgrade(&entry));
        Ok(Registration { _entry: entry })
    }
    /// Lists registered schemas in deterministic source order.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn schemas(&self) -> BTreeMap<String, Vec<Column>> {
        let entries = self.entries.read().expect("registry lock poisoned").clone();
        entries
            .into_iter()
            .filter_map(|(name, entry)| {
                entry
                    .upgrade()
                    .filter(|entry| self.visible(entry))
                    .map(|entry| (name, entry.columns.clone()))
            })
            .collect()
    }
    /// Copies one source without retaining the catalog lock during application code.
    /// # Errors
    /// Returns missing-source, busy, invalid-snapshot, or budget errors.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn snapshot(&self, name: &str, limits: Limits) -> Result<Snapshot, Error> {
        let entry = self
            .entries
            .read()
            .expect("registry lock poisoned")
            .get(name)
            .and_then(Weak::upgrade)
            .filter(|entry| self.visible(entry))
            .ok_or_else(|| Error::NotFound(name.into()))?;
        let snapshot = entry.source.snapshot(limits)?;
        if entry.published.is_none() {
            snapshot.validate(limits)?;
            if snapshot.columns != entry.columns {
                return Err(Error::Invalid(
                    "source changed its registered schema".into(),
                ));
            }
        }
        Ok(snapshot)
    }
    fn visible(&self, entry: &Entry) -> bool {
        matches!(self.access, Access::Local) || entry.published.is_some()
    }
    /// Returns a publication revision; live sources have no revision and cannot skip scans.
    /// # Errors
    /// Returns not-found or busy when the source cannot be observed.
    /// # Panics
    /// Panics if the registry lock was poisoned.
    pub fn revision(&self, name: &str) -> Result<Option<Revision>, Error> {
        let entry = self
            .entries
            .read()
            .expect("registry lock poisoned")
            .get(name)
            .and_then(Weak::upgrade)
            .filter(|entry| self.visible(entry))
            .ok_or_else(|| Error::NotFound(name.into()))?;
        entry
            .published
            .as_ref()
            .map(|published| {
                published.sequence().map(|sequence| Revision {
                    registration: entry.id,
                    sequence,
                })
            })
            .transpose()
    }
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Serializes one field without allocating beyond the remaining serialized byte budget.
/// # Errors
/// Returns a limit error before appending oversized data, or a serialization error.
#[doc(hidden)]
pub fn value_with_budget<T: Serialize>(value: &T, remaining: &mut usize) -> Result<Value, Error> {
    let bytes = json_bytes(value, *remaining)?;
    *remaining -= bytes.len();
    Ok(serde_json::from_slice(&bytes)?)
}

/// Serializes a value through a capped writer without retaining excess output.
/// # Errors
/// Returns a limit or serialization error when encoding cannot complete within the budget.
pub fn json_bytes<T: Serialize>(value: &T, max: usize) -> Result<Vec<u8>, Error> {
    struct Buffer {
        bytes: Vec<u8>,
        max: usize,
        exceeded: bool,
    }
    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                self.exceeded = true;
                return Err(std::io::Error::other("field byte budget exceeded"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        max,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut buffer, value);
    if buffer.exceeded {
        return Err(Error::Limit("field bytes".into()));
    }
    result?;
    Ok(buffer.bytes)
}
