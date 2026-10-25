use crate::{Column, Error, Inspect, Limits, Snapshot, Source};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, RwLock, TryLockError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
/// Freshness of the last successful publication and cumulative failure categories.
/// Counters are sampled independently and contain no application error messages.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublicationStatus {
    /// Current successful publication sequence.
    pub sequence: u64,
    /// Monotonic age in milliseconds, or none before the first successful capture.
    pub age_ms: Option<u64>,
    /// Wall-clock publication timestamp, or none before the first successful capture.
    pub published_at_unix_ms: Option<u64>,
    /// Failed capture/publication attempts by stable error code, including Busy.
    pub failures: BTreeMap<String, u64>,
}
const FAILURE_CODES: [&str; 7] = [
    "busy",
    "deadline",
    "limit",
    "invalid",
    "not_found",
    "serialization",
    "io",
];

#[derive(Debug)]
struct Version {
    sequence: u64,
    snapshot: Snapshot,
    bytes: usize,
    captured: Option<(Instant, u64)>,
}
/// Immutable snapshots explicitly published by the application for remote queries.
///
/// Queries never invoke application serializers or acquire application collection locks.
/// Capture runs only when the application calls `refresh` or `publish`.
#[derive(Debug, Clone)]
pub struct Published {
    current: Arc<RwLock<Arc<Version>>>,
    columns: Arc<Vec<Column>>,
    limits: Limits,
    failures: Arc<[AtomicU64; 7]>,
}
impl Published {
    /// Starts with an empty typed snapshot and fixed publication budgets.
    /// # Errors
    /// Rejects a schema larger than the configured byte budget.
    pub fn new<T: Inspect>(limits: Limits) -> Result<Self, Error> {
        let published = Self::from_snapshot(
            Snapshot {
                columns: T::schema(),
                rows: Vec::new(),
            },
            limits,
        )?;
        // Construction is not a successful application capture.
        Arc::get_mut(
            &mut *published
                .current
                .write()
                .expect("new publication lock is unpoisoned"),
        )
        .expect("new version is uniquely owned")
        .captured = None;
        Ok(published)
    }
    /// Owns an initial snapshot, checked before any remote registration.
    /// # Errors
    /// Rejects schema mismatches, oversized or excessively nested JSON data.
    pub fn from_snapshot(snapshot: Snapshot, limits: Limits) -> Result<Self, Error> {
        let bytes = snapshot.validate(limits)?;
        Ok(Self {
            columns: Arc::new(snapshot.columns.clone()),
            current: Arc::new(RwLock::new(Arc::new(Version {
                sequence: 0,
                snapshot,
                bytes,
                captured: Some((Instant::now(), unix_ms())),
            }))),
            limits,
            failures: Arc::new(std::array::from_fn(|_| AtomicU64::new(0))),
        })
    }
    /// Publishes a complete replacement, leaving the previous snapshot intact on failure.
    /// # Errors
    /// Rejects changed schema, exhausted budgets, a busy publication lock or version overflow.
    /// # Panics
    /// Panics if the publication lock was poisoned.
    pub fn publish(&self, snapshot: Snapshot) -> Result<u64, Error> {
        let result = self.publish_inner(snapshot);
        if let Err(error) = &result {
            self.record_failure(error);
        }
        result
    }
    fn publish_inner(&self, snapshot: Snapshot) -> Result<u64, Error> {
        let bytes = snapshot.validate(self.limits)?;
        if snapshot.columns != *self.columns {
            return Err(Error::Invalid("published schema cannot change".into()));
        }
        let mut current = self.current.try_write().map_err(lock_error)?;
        let sequence = current
            .sequence
            .checked_add(1)
            .ok_or_else(|| Error::Limit("publication sequence".into()))?;
        let old = std::mem::replace(
            &mut *current,
            Arc::new(Version {
                sequence,
                snapshot,
                bytes,
                captured: Some((Instant::now(), unix_ms())),
            }),
        );
        drop(current);
        drop(old);
        Ok(sequence)
    }
    /// Captures a source now, on the application's chosen thread, then publishes it.
    /// # Errors
    /// Propagates capture errors without replacing the last successful publication.
    pub fn refresh(&self, source: &impl Source) -> Result<u64, Error> {
        match source.snapshot(self.limits) {
            Ok(snapshot) => self.publish(snapshot),
            Err(error) => {
                self.record_failure(&error);
                Err(error)
            }
        }
    }
    fn record_failure(&self, error: &Error) {
        if let Some(index) = FAILURE_CODES.iter().position(|code| *code == error.code()) {
            self.failures[index].fetch_add(1, Ordering::Relaxed);
        }
    }
    /// Samples freshness without inspecting or copying application rows.
    /// # Errors
    /// Returns Busy during the short publication swap.
    /// # Panics
    /// Panics if the publication lock was poisoned.
    pub fn status(&self) -> Result<PublicationStatus, Error> {
        let current = self.current.try_read().map_err(lock_error)?;
        Ok(PublicationStatus {
            sequence: current.sequence,
            age_ms: current
                .captured
                .map(|(instant, _)| instant.elapsed().as_millis().min(u64::MAX as u128) as u64),
            published_at_unix_ms: current.captured.map(|(_, timestamp)| timestamp),
            failures: FAILURE_CODES
                .iter()
                .enumerate()
                .map(|(i, code)| (code.to_string(), self.failures[i].load(Ordering::Relaxed)))
                .collect(),
        })
    }
    /// Current publication number, or busy if another publication is being swapped.
    /// # Errors
    /// Returns busy while a publisher holds the short internal lock.
    /// # Panics
    /// Panics if the publication lock was poisoned.
    pub fn sequence(&self) -> Result<u64, Error> {
        Ok(self.current.try_read().map_err(lock_error)?.sequence)
    }
}
impl Source for Published {
    fn schema(&self) -> Vec<Column> {
        (*self.columns).clone()
    }
    fn snapshot(&self, limits: Limits) -> Result<Snapshot, Error> {
        let current = self.current.try_read().map_err(lock_error)?.clone();
        if current.bytes > limits.bytes || current.snapshot.rows.len() > limits.scan_rows {
            return Err(Error::Limit("published snapshot".into()));
        }
        Ok(current.snapshot.clone())
    }
}
fn lock_error<T>(error: TryLockError<T>) -> Error {
    match error {
        TryLockError::WouldBlock => Error::Busy,
        TryLockError::Poisoned(_) => panic!("publication lock poisoned"),
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
