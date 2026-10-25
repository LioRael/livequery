use crate::{Column, Error, Inspect, Limits, Snapshot, Source};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt,
    hash::{BuildHasher, Hash},
    sync::{Arc, Mutex, RwLock, TryLockError},
};

/// A collection whose records can be inspected in deterministic order.
pub trait RowCollection: Send + Sync {
    /// Record type shared by all collection entries.
    type Item: Inspect;
    /// Number of records, used before allocating a snapshot.
    fn len(&self) -> usize;
    /// Whether the collection has no records.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Iterates records in source order; maps use sorted key order.
    fn rows(&self) -> impl Iterator<Item = &Self::Item>;
}
impl<T: Inspect> RowCollection for Vec<T> {
    type Item = T;
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn rows(&self) -> impl Iterator<Item = &T> {
        self.iter()
    }
}
impl<T: Inspect> RowCollection for VecDeque<T> {
    type Item = T;
    fn len(&self) -> usize {
        VecDeque::len(self)
    }
    fn rows(&self) -> impl Iterator<Item = &T> {
        self.iter()
    }
}
impl<K: Ord + Send + Sync, T: Inspect> RowCollection for BTreeMap<K, T> {
    type Item = T;
    fn len(&self) -> usize {
        BTreeMap::len(self)
    }
    fn rows(&self) -> impl Iterator<Item = &T> {
        self.values()
    }
}
impl<K: Ord + Hash + Eq + Send + Sync, T: Inspect, S: BuildHasher + Send + Sync> RowCollection
    for HashMap<K, T, S>
{
    type Item = T;
    fn len(&self) -> usize {
        HashMap::len(self)
    }
    fn rows(&self) -> impl Iterator<Item = &T> {
        let mut entries: Vec<_> = self.iter().collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
        entries.into_iter().map(|(_, value)| value)
    }
}
enum Storage<C> {
    ReadWrite(Arc<RwLock<C>>),
    Exclusive(Arc<Mutex<C>>),
}
/// A nonblocking adapter for shared vectors, queues and maps.
pub struct CollectionSource<C> {
    storage: Storage<C>,
}
impl<C> fmt::Debug for CollectionSource<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CollectionSource").finish_non_exhaustive()
    }
}
impl<C> Clone for CollectionSource<C> {
    fn clone(&self) -> Self {
        Self {
            storage: match &self.storage {
                Storage::ReadWrite(value) => Storage::ReadWrite(value.clone()),
                Storage::Exclusive(value) => Storage::Exclusive(value.clone()),
            },
        }
    }
}
impl<C> CollectionSource<C> {
    /// Owns a collection without starting background work.
    pub fn new(collection: C) -> Self {
        Self::from_shared(Arc::new(RwLock::new(collection)))
    }
    /// Adapts an existing read/write lock without copying its collection.
    pub fn from_shared(collection: Arc<RwLock<C>>) -> Self {
        Self {
            storage: Storage::ReadWrite(collection),
        }
    }
    /// Adapts an existing mutex without copying its collection.
    pub fn from_mutex(collection: Arc<Mutex<C>>) -> Self {
        Self {
            storage: Storage::Exclusive(collection),
        }
    }
    /// Applies an application update without waiting for another lock holder.
    /// # Errors
    /// Returns busy when the collection is locked.
    /// # Panics
    /// Panics if a prior application update poisoned the lock.
    pub fn update<R>(&self, update: impl FnOnce(&mut C) -> R) -> Result<R, Error> {
        match &self.storage {
            Storage::ReadWrite(value) => Ok(update(&mut *value.try_write().map_err(lock_error)?)),
            Storage::Exclusive(value) => Ok(update(&mut *value.try_lock().map_err(lock_error)?)),
        }
    }
}
impl<C: RowCollection> Source for CollectionSource<C> {
    fn schema(&self) -> Vec<Column> {
        C::Item::schema()
    }
    fn snapshot(&self, limits: Limits) -> Result<Snapshot, Error> {
        let capture = |collection: &C| {
            if collection.len() > limits.scan_rows {
                return Err(Error::Limit("scan rows".into()));
            }
            Snapshot::capture(collection.rows(), limits)
        };
        match &self.storage {
            Storage::ReadWrite(value) => capture(&*value.try_read().map_err(lock_error)?),
            Storage::Exclusive(value) => capture(&*value.try_lock().map_err(lock_error)?),
        }
    }
}
fn lock_error<T>(error: TryLockError<T>) -> Error {
    match error {
        TryLockError::WouldBlock => Error::Busy,
        TryLockError::Poisoned(_) => panic!("collection lock poisoned by application update"),
    }
}
/// A shared vector with whole-collection snapshot consistency.
pub type Table<T> = CollectionSource<Vec<T>>;
