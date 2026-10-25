use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fmt,
    sync::{Arc, RwLock, Weak},
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
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            scan_rows: 10_000,
            result_rows: 1_000,
            bytes: 1_048_576,
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
/// A shared vector with nonblocking, whole-collection snapshot consistency.
pub struct Table<T> {
    inner: Arc<RwLock<Vec<T>>>,
}
impl<T> fmt::Debug for Table<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Table").finish_non_exhaustive()
    }
}
impl<T> Clone for Table<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T> Table<T> {
    /// Owns rows without starting threads or recording history.
    pub fn new(rows: Vec<T>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(rows)),
        }
    }
    /// Connects an existing shared vector without copying its contents.
    pub fn from_shared(inner: Arc<RwLock<Vec<T>>>) -> Self {
        Self { inner }
    }
    /// Applies an application update if the collection is currently unlocked.
    /// # Errors
    /// Returns busy when a reader or writer holds the lock.
    /// # Panics
    /// Panics if an earlier application update poisoned the lock.
    pub fn update<R>(&self, update: impl FnOnce(&mut Vec<T>) -> R) -> Result<R, Error> {
        match self.inner.try_write() {
            Ok(mut rows) => Ok(update(&mut rows)),
            Err(std::sync::TryLockError::WouldBlock) => Err(Error::Busy),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                panic!("table lock poisoned by application update")
            }
        }
    }
}
impl<T: Inspect> Source for Table<T> {
    fn schema(&self) -> Vec<Column> {
        T::schema()
    }
    fn snapshot(&self, limits: Limits) -> Result<Snapshot, Error> {
        let rows = match self.inner.try_read() {
            Ok(rows) => rows,
            Err(std::sync::TryLockError::WouldBlock) => return Err(Error::Busy),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                panic!("table lock poisoned by application update")
            }
        };
        if rows.len() > limits.scan_rows {
            return Err(Error::Limit("scan rows".into()));
        }
        let mut snapshot = Snapshot {
            columns: T::schema(),
            rows: Vec::with_capacity(rows.len()),
        };
        let mut bytes = 0;
        for row in rows.iter() {
            let row = row.inspect_bounded(limits.bytes.saturating_sub(bytes))?;
            bytes += serde_json::to_vec(&row)?.len();
            if bytes > limits.bytes {
                return Err(Error::Limit("snapshot bytes".into()));
            }
            snapshot.rows.push(row);
        }
        Ok(snapshot)
    }
}
#[derive(Debug)]
struct Entry {
    source: Arc<dyn Source>,
}
/// A process-local catalog; clones refer to the same registrations.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: Arc<RwLock<BTreeMap<String, Weak<Entry>>>>,
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
    /// Exposes one named source until its registration guard is dropped.
    /// # Errors
    /// Rejects duplicate or invalid identifiers and invalid field schemas.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn expose(&self, name: &str, source: impl Source + 'static) -> Result<Registration, Error> {
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
        let entry = Arc::new(Entry {
            source: Arc::new(source),
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
            .filter_map(|(name, entry)| entry.upgrade().map(|entry| (name, entry.source.schema())))
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
            .ok_or_else(|| Error::NotFound(name.into()))?;
        let snapshot = entry.source.snapshot(limits)?;
        if snapshot.rows.len() > limits.scan_rows
            || serde_json::to_vec(&snapshot)?.len() > limits.bytes
        {
            return Err(Error::Limit("snapshot".into()));
        }
        if snapshot.columns != entry.source.schema()
            || snapshot.rows.iter().any(|row| {
                row.len() != snapshot.columns.len()
                    || snapshot.columns.iter().any(|c| !row.contains_key(&c.name))
            })
        {
            return Err(Error::Invalid("source returned inconsistent schema".into()));
        }
        Ok(snapshot)
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

pub(crate) fn json_bytes<T: Serialize>(value: &T, max: usize) -> Result<Vec<u8>, Error> {
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
