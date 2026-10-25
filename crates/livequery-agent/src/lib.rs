//! Read-only dispatch over published sources, independent of a transport event loop.
use livequery_core::{Error, Limits, Registry};
use livequery_query::Query;
use livequery_transport::{Request, Response};
#[cfg(unix)]
pub mod local;
/// Dispatches bounded requests against a published-only registry view.
#[derive(Debug)]
pub struct Agent {
    registry: Registry,
    limits: Limits,
}
impl Agent {
    /// Creates a dispatcher without starting threads or opening a listener.
    pub fn new(registry: Registry, limits: Limits) -> Self {
        Self {
            registry: registry.published_only(),
            limits,
        }
    }
    /// Executes a read-only request without invoking application capture callbacks.
    /// # Errors
    /// Returns query, publication, schema and resource-limit errors.
    pub fn handle(&self, request: Request) -> Result<Response, Error> {
        match request {
            Request::Schema => {
                let mut remaining = self.limits.bytes;
                Ok(Response::Ok {
                    data: livequery_core::value_with_budget(
                        &self.registry.schemas(),
                        &mut remaining,
                    )?,
                })
            }
            Request::Query { sql } => Ok(Response::Ok {
                data: serde_json::to_value(
                    Query::parse(&sql)?.execute(&self.registry, self.limits)?,
                )?,
            }),
            Request::Observe { sql, since } => {
                let query = Query::parse(&sql)?;
                let revision = query.revision(&self.registry)?.ok_or_else(|| {
                    Error::Invalid("remote observation requires published data".into())
                })?;
                if since == Some(revision) {
                    return Ok(Response::NotModified { revision });
                }
                let data = serde_json::to_value(query.execute(&self.registry, self.limits)?)?;
                Ok(Response::Observation { revision, data })
            }
            _ => Err(Error::Invalid("unsupported request".into())),
        }
    }
}
