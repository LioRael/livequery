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
impl Error {
    /// Stable diagnostic category; messages are not a compatibility contract.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid",
            Self::Limit(_) => "limit",
            Self::Busy => "busy",
            Self::Deadline => "deadline",
            Self::NotFound(_) => "not_found",
            Self::Serialization(_) => "serialization",
            Self::Io(_) => "io",
        }
    }
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
        json_bytes(&row, max_bytes)?;
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
        validate_json(&bytes)?;
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
// Walk through serde_json's normal recursive deserializer without constructing a
// second JSON tree. deserialize_any retains its nesting and numeric checks.
struct ValidatedJson(bool);
impl<'de> Deserialize<'de> for ValidatedJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ValidatedJson;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a bounded JSON value")
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(ValidatedJson(false))
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(ValidatedJson(false))
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(ValidatedJson(false))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(ValidatedJson(false))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(ValidatedJson(matches!(
                    value,
                    "$serde_json::private::Number" | "$serde_json::private::RawValue"
                )))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(ValidatedJson(false))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut special = false;
                while let Some(value) = seq.next_element::<ValidatedJson>()? {
                    special |= value.0;
                }
                Ok(ValidatedJson(special))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut special = false;
                while let Some(key) = map.next_key::<ValidatedJson>()? {
                    special |= key.0 | map.next_value::<ValidatedJson>()?.0;
                }
                Ok(ValidatedJson(special))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn validate_json(bytes: &[u8]) -> Result<(), serde_json::Error> {
    let scanned: ValidatedJson = serde_json::from_slice(bytes)?;
    // Feature unification can enable Value's reserved-key representations. Keep
    // their acceptance semantics without rebuilding ordinary snapshot trees.
    if scanned.0 {
        let _: Value = serde_json::from_slice(bytes)?;
    }
    Ok(())
}

/// An opaque source identity and publication sequence used for conditional observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    registration: u64,
    sequence: u64,
}
impl Revision {
    /// Successful publication sequence within this source registration.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
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
/// A consistent read of one immutable snapshot and its publication metadata.
/// Published reads share owned immutable data; live local sources are captured once.
#[derive(Debug, Clone)]
pub struct SnapshotView {
    snapshot: Arc<Snapshot>,
    publication: Option<crate::PublicationStatus>,
    revision: Option<Revision>,
}
impl SnapshotView {
    /// Borrows rows without copying the complete published table.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    /// Metadata sampled from this exact snapshot version, or none for a live source.
    pub fn publication(&self) -> Option<&crate::PublicationStatus> {
        self.publication.as_ref()
    }
    /// Registration and publication sequence corresponding to these rows.
    pub fn revision(&self) -> Option<Revision> {
        self.revision
    }
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
    /// Lists schemas while bounding aggregate serialization and copying.
    /// # Errors
    /// Returns Busy, a limit or deadline error instead of waiting or constructing an oversized catalog.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn schemas_bounded(&self, limits: Limits) -> Result<BTreeMap<String, Vec<Column>>, Error> {
        let start = Instant::now();
        let entries = self.entries.try_read().map_err(catalog_lock_error)?;
        let mut result = BTreeMap::new();
        let mut remaining = limits.bytes;
        for (name, entry) in entries.iter() {
            if start.elapsed() >= limits.query_timeout {
                return Err(Error::Deadline);
            }
            if let Some(entry) = entry.upgrade().filter(|entry| self.visible(entry)) {
                let size = json_bytes(&(name, &entry.columns), remaining)?.len();
                remaining = remaining.saturating_sub(size);
                result.insert(name.clone(), entry.columns.clone());
            }
        }
        json_bytes(&result, limits.bytes)?;
        Ok(result)
    }
    /// Reports publication freshness and failure counters without capturing application state.
    /// # Errors
    /// Returns busy, deadline or resource-limit errors.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn publication_status(
        &self,
        limits: Limits,
    ) -> Result<BTreeMap<String, crate::PublicationStatus>, Error> {
        let start = Instant::now();
        let entries = self.entries.try_read().map_err(catalog_lock_error)?;
        let mut result = BTreeMap::new();
        let mut remaining = limits.bytes;
        for (name, entry) in entries.iter() {
            if start.elapsed() >= limits.query_timeout {
                return Err(Error::Deadline);
            }
            if let Some(entry) = entry.upgrade().filter(|entry| self.visible(entry))
                && let Some(published) = &entry.published
            {
                let status = published.status()?;
                remaining =
                    remaining.saturating_sub(json_bytes(&(name, &status), remaining)?.len());
                result.insert(name.clone(), status);
            }
        }
        json_bytes(&result, limits.bytes)?;
        Ok(result)
    }
    /// Copies one source without retaining the catalog lock during application code.
    /// # Errors
    /// Returns missing-source, busy, invalid-snapshot, or budget errors.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn snapshot(&self, name: &str, limits: Limits) -> Result<Snapshot, Error> {
        Ok(self.read_snapshot(name, limits)?.snapshot().clone())
    }
    /// Reads a shared immutable publication, or captures a local source once.
    /// # Errors
    /// Returns Busy, missing-source, schema or budget errors.
    /// # Panics
    /// Panics if the registry lock is poisoned.
    pub fn read_snapshot(&self, name: &str, limits: Limits) -> Result<SnapshotView, Error> {
        let entry = self
            .entries
            .try_read()
            .map_err(catalog_lock_error)?
            .get(name)
            .and_then(Weak::upgrade)
            .filter(|entry| self.visible(entry))
            .ok_or_else(|| Error::NotFound(name.into()))?;
        if let Some(published) = &entry.published {
            let (snapshot, publication) = published.read(limits)?;
            let revision = Revision {
                registration: entry.id,
                sequence: publication.sequence,
            };
            return Ok(SnapshotView {
                snapshot,
                publication: Some(publication),
                revision: Some(revision),
            });
        }
        let snapshot = entry.source.snapshot(limits)?;
        if entry.published.is_none() {
            snapshot.validate(limits)?;
            if snapshot.columns != entry.columns {
                return Err(Error::Invalid(
                    "source changed its registered schema".into(),
                ));
            }
        }
        Ok(SnapshotView {
            snapshot: Arc::new(snapshot),
            publication: None,
            revision: None,
        })
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
            .try_read()
            .map_err(catalog_lock_error)?
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
fn catalog_lock_error<T>(error: std::sync::TryLockError<T>) -> Error {
    match error {
        std::sync::TryLockError::WouldBlock => Error::Busy,
        std::sync::TryLockError::Poisoned(_) => panic!("registry lock poisoned"),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_free_validation_preserves_json_acceptance_and_depth_limit() {
        for depth in 0..140 {
            for (open, close) in [("[", "]"), ("{\"key\":", "}")] {
                let input = format!("{}null{}", open.repeat(depth), close.repeat(depth));
                assert_eq!(
                    validate_json(input.as_bytes()).is_ok(),
                    serde_json::from_str::<Value>(&input).is_ok(),
                    "depth={depth}, input={open}"
                );
            }
        }
        for input in [
            r#"{"$serde_json::private::Number":"not-a-number"}"#,
            r#"{"$serde_json::private::RawValue":"invalid"}"#,
            "null",
            "true",
            "-9223372036854775808",
            "18446744073709551615",
            "1.25",
            "1e400",
            "[1,]",
            "{\"x\":}",
            "null true",
            "\"escaped\\ntext\"",
        ] {
            assert_eq!(
                validate_json(input.as_bytes()).is_ok(),
                serde_json::from_str::<Value>(input).is_ok(),
                "{input}"
            );
        }
    }
    #[test]
    fn request_paths_do_not_wait_for_registration_writers() {
        let registry = Registry::new();
        let held = registry.entries.write().unwrap();
        let reader = registry.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let results = [
                matches!(reader.schemas_bounded(Limits::default()), Err(Error::Busy)),
                matches!(
                    reader.publication_status(Limits::default()),
                    Err(Error::Busy)
                ),
                matches!(
                    reader.snapshot("items", Limits::default()),
                    Err(Error::Busy)
                ),
                matches!(reader.revision("items"), Err(Error::Busy)),
            ];
            tx.send(results).unwrap();
        });
        let result = rx.recv_timeout(Duration::from_secs(1));
        drop(held);
        worker.join().unwrap();
        assert_eq!(
            result.expect("request paths must not await application registration"),
            [true; 4]
        );
    }
}
