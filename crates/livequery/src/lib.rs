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
pub use livequery_agent::Agent;
#[doc(hidden)]
pub use livequery_core::value_with_budget;
pub use livequery_core::{
    CollectionSource, Column, Error, Inspect, Limits, PublicationStatus, Published, Registration,
    Registry, Revision, Row, RowCollection, Snapshot, Source, Table, schemars, serde, serde_json,
};
pub use livequery_query::{Query, QueryResult, Watch};
