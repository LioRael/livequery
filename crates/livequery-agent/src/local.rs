//! Unix agent lifecycle combining a dispatcher with the bounded transport server.
use crate::Agent;
use livequery_core::{Error, Limits, Registry};
use std::{path::Path, sync::atomic::AtomicBool};
/// One bounded local agent, run on a caller-owned thread.
#[derive(Debug)]
pub struct Server {
    transport: livequery_transport::local::Server,
    agent: Agent,
}
impl Server {
    /// Binds a private Unix socket and restricts discovery to published sources.
    /// # Errors
    /// Rejects insecure paths, existing sockets and I/O failures.
    pub fn bind(path: impl AsRef<Path>, registry: Registry, limits: Limits) -> Result<Self, Error> {
        Ok(Self {
            transport: livequery_transport::local::Server::bind(path, limits)?,
            agent: Agent::new(registry, limits),
        })
    }
    /// Advances bounded clients until the caller requests cancellation.
    /// # Errors
    /// Reports listener failures; client failures are isolated by the transport.
    pub fn serve(&self, stop: &AtomicBool) -> Result<(), Error> {
        self.transport
            .serve(stop, |request| self.agent.handle(request))
    }
}
