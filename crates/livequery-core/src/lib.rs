//! Runtime-independent schemas, registration, collection adapters and owned publication.
extern crate self as livequery_core;
mod collection;
mod published;
mod state;
pub use collection::{CollectionSource, RowCollection, Table};
pub use livequery_derive::Inspect;
pub use published::{PublicationStatus, Published};
#[doc(hidden)]
pub use state::value_with_budget;
pub use state::{
    Column, Error, Inspect, Limits, Registration, Registry, Revision, Row, Snapshot, Source,
    json_bytes,
};
pub use {schemars, serde, serde_json};
