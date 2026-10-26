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
    /// Negotiate a protocol version before issuing commands.
    Hello {
        /// Requested wire protocol version.
        protocol: u16,
    },
    /// Report freshness and failed capture/publication attempts.
    Status,
    /// Observe with an agent-session and exact-query-bound cursor.
    ObserveV2 {
        /// SELECT statement.
        sql: String,
        /// Opaque cursor returned by a previous observation.
        since: Option<Cursor>,
    },
    /// Query with freshness metadata for the actual read version (query_details capability).
    QueryDetails {
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

/// Current negotiated wire protocol. Legacy alpha.2 commands remain accepted.
pub const PROTOCOL_VERSION: u16 = 2;
/// Opaque conditional observation hint; never an authorization credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    /// Random identity generated for each Agent instance.
    pub session: String,
    /// Digest of the exact SQL bytes.
    pub query: String,
    /// Source registration and publication sequence.
    pub revision: livequery_core::Revision,
}

/// Protocol-v2 observation payload inside a successful Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Observation {
    /// Full replacement result and its cursor.
    Changed {
        /// Cursor to echo on the next poll.
        cursor: Cursor,
        /// Query result.
        result: serde_json::Value,
    },
    /// Same session, SQL and publication as the previous poll.
    Unchanged {
        /// Cursor to echo on the next poll.
        cursor: Cursor,
    },
}
