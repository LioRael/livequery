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

/// An optional caller-owned service thread. Capture scheduling remains with the application.
/// Dropping requests cancellation and joins; use `stop` then `wait` to observe failures.
#[derive(Debug)]
#[must_use = "dropping the running agent shuts it down"]
pub struct RunningAgent {
    stop: std::sync::Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), Error>>>,
}
impl RunningAgent {
    /// Binds synchronously, then starts a dedicated diagnostic thread.
    /// # Errors
    /// Returns bind or thread creation errors without leaving a running service.
    pub fn start(
        path: impl AsRef<Path>,
        registry: Registry,
        limits: Limits,
    ) -> Result<Self, Error> {
        let server = Server::bind(path, registry, limits)?;
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let thread = std::thread::Builder::new()
            .name("livequery-agent".into())
            .spawn(move || server.serve(&signal))?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
    /// Requests cancellation; safe to call repeatedly.
    pub fn stop(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    /// Waits for exit after `stop`, propagating listener failures or a thread panic.
    /// This blocks the current thread; async applications should use `spawn_blocking`.
    /// # Errors
    /// Returns listener failures or an I/O error if the service thread panicked.
    pub fn wait(mut self) -> Result<(), Error> {
        self.join()
    }
    fn join(&mut self) -> Result<(), Error> {
        match self.thread.take() {
            Some(thread) => thread
                .join()
                .map_err(|_| std::io::Error::other("LiveQuery service thread panicked"))?,
            None => Ok(()),
        }
    }
}
impl Drop for RunningAgent {
    fn drop(&mut self) {
        self.stop();
        let _ = self.join();
    }
}
