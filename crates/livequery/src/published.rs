use crate::{Column, Error, Inspect, Limits, Snapshot, Source};
use std::sync::{Arc, RwLock, TryLockError};

#[derive(Debug)]
struct Version {
    sequence: u64,
    snapshot: Snapshot,
    bytes: usize,
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
}
impl Published {
    /// Starts with an empty typed snapshot and fixed publication budgets.
    /// # Errors
    /// Rejects a schema larger than the configured byte budget.
    pub fn new<T: Inspect>(limits: Limits) -> Result<Self, Error> {
        Self::from_snapshot(
            Snapshot {
                columns: T::schema(),
                rows: Vec::new(),
            },
            limits,
        )
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
            }))),
            limits,
        })
    }
    /// Publishes a complete replacement, leaving the previous snapshot intact on failure.
    /// # Errors
    /// Rejects changed schema, exhausted budgets, a busy publication lock or version overflow.
    /// # Panics
    /// Panics if the publication lock was poisoned.
    pub fn publish(&self, snapshot: Snapshot) -> Result<u64, Error> {
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
        self.publish(source.snapshot(self.limits)?)
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
