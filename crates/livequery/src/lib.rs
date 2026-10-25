//! Explicit, bounded, read-only inspection of application-owned runtime state.
//!
//! ```
//! use livequery::{Inspect, Limits, Query, Registry, Table};
//! #[derive(Debug, Inspect)]
//! struct Player { id: u64, health: u32 }
//! let registry = Registry::new();
//! let players = Table::new(vec![Player { id: 42, health: 12 }]);
//! let _registration = registry.expose("players", players.clone())?;
//! let result = Query::parse("SELECT id FROM players WHERE health < 20")?
//!     .execute(&registry, Limits::default())?;
//! assert_eq!(result.rows.len(), 1);
//! # Ok::<(), livequery::Error>(())
//! ```
//!
//! Enums also require Serialize and JsonSchema to define their representation.
//! ```compile_fail
//! #[derive(livequery::Inspect)]
//! enum Unsupported { A, B }
//! ```
#[cfg(unix)]
pub mod local;
extern crate self as livequery;
mod collection;
mod published;
mod query;
mod state;
pub use livequery_derive::Inspect;
pub use query::{Query, QueryResult, Watch};
pub use state::{
    Column, Error, Inspect, Limits, Registration, Registry, Revision, Row, Snapshot, Source,
};
// The wire values and derive output intentionally use one matching serde_json version.
pub use serde_json;

pub use serde;
#[doc(hidden)]
pub use state::value_with_budget;

pub use schemars;

pub use collection::{CollectionSource, RowCollection, Table};
pub use published::Published;
