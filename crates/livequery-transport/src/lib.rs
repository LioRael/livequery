//! Read-only protocol shared by agents and clients, with optional Unix transport.
use serde::{Deserialize, Serialize};
#[cfg(unix)]
pub mod local;
/// Version-one read-only request; one request is served per connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum Request {
    /// Discover source schemas.
    Schema,
    /// Fetch a changed publication without retaining a server-side subscription.
    Observe {
        /// SELECT statement, identical across polls belonging to one observation.
        sql: String,
        /// Previously returned revision, or none for the first observation.
        since: Option<livequery_core::Revision>,
    },
    /// Execute one bounded query.
    Query {
        /// SELECT statement.
        sql: String,
    },
}
/// Machine-readable success or diagnostic response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    /// JSON schema map or query result.
    Ok {
        /// Response payload.
        data: serde_json::Value,
    },
    /// A changed published snapshot, represented as a full query result.
    Observation {
        /// Publication revision sampled before query execution.
        revision: livequery_core::Revision,
        /// Full replacement query result.
        data: serde_json::Value,
    },
    /// The source publication has not changed; no snapshot scan was performed.
    NotModified {
        /// Current publication revision.
        revision: livequery_core::Revision,
    },
    /// Stable error category and readable detail.
    Error {
        /// Error category.
        code: String,
        /// Diagnostic detail.
        message: String,
    },
}
