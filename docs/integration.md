# Explicit capture, simpler ownership

`PublishedHandle` combines a publication and its registration guard. Dropping it unregisters the table. It exposes `refresh`, `publish` and `status` but starts no capture thread or timer. Existing `Published`, `Registration`, `Registry` and `Server` APIs remain available.

```rust
use livequery::{Inspect, Limits, PublishedHandle, Registry, Table};
use livequery::local::RunningAgent;
#[derive(Debug, Inspect)]
struct Job { id: u64 }
fn main() -> Result<(), livequery::Error> {
let socket_path = livequery::local::directory().join(format!("{}.sock", std::process::id()));
let registry = Registry::new();
let jobs = Table::new(vec![Job { id: 1 }]);
let publication = PublishedHandle::register::<Job>(&registry, "jobs", Limits::default())?;
let agent = RunningAgent::start(socket_path, registry, Limits::default())?;
// Call this at application-owned checkpoints or on your chosen capture thread.
if let Err(error) = publication.refresh(&jobs) {
    eprintln!("diagnostics stale: {error}"); // previous snapshot and failure count remain
}
agent.stop(); // idempotent request
agent.wait()?; // joins and reports service failure
Ok(())
}
```

Binding/startup errors are explicit: the application chooses whether diagnostics are mandatory or should be disabled while business work continues. `Busy` during refresh is expected backpressure; do not spin or terminate business work. Retry at the next chosen checkpoint and inspect failure counters. `publish` validates a prepared snapshot before swapping it. When assembling a snapshot outside `refresh`, the application must also handle capture errors; use `refresh` when unified capture/publication failure accounting is desired.

`RunningAgent` owns one diagnostic thread. `start` binds before returning. `stop` is an idempotent cancellation request, not a completed join; `wait` consumes the handle and waits for termination (call `stop` first). Calling `wait` on a still-running service does not request cancellation. Drop also stops and joins as a fallback, but cannot return listener errors. Both `wait` and Drop can block the calling thread. Cancellation is checked between clients; an in-progress synchronous handler is not forcibly preempted, so this is not a hard shutdown-time guarantee. Keep orderly shutdown explicit. In Tokio, capture and join potentially blocking work off the executor:

```rust
let publication = std::sync::Arc::new(publication);
let handle = publication.clone();
let source = jobs.clone();
match tokio::task::spawn_blocking(move || handle.refresh(&source)).await {
    Ok(Ok(_)) => {}
    Ok(Err(error)) => eprintln!("diagnostics stale: {error}"),
    Err(error) => eprintln!("capture task failed: {error}"),
}
agent.stop();
tokio::task::spawn_blocking(move || agent.wait()).await??;
```

Do not schedule overlapping unlimited blocking captures. Await completion before scheduling another; the application chooses frequency. The ordinary-thread and Tokio recipes are exercised in `examples/concurrent` and `examples/async-server`. The jobs example copies lightweight diagnostic values while holding the business lock, releases it, then serializes/publishes. This reduces lock occupancy but still costs allocation and copying; large strings should be reduced to the fields actually useful for diagnosis.

A `SnapshotView` pins an immutable publication. Query reads use it internally and copy only returned fields; `Registry::snapshot` keeps its existing owned-copy behavior. Holding many views across many publications retains those old snapshots, so scope views to a query and drop them promptly. Metadata and revision belong to the same pinned version. Publication age is sampled when the query acquires that view; network and rendering time may add latency.

Published snapshots are immutable owned data. Dropping a registration handle prevents future registry lookups, while already acquired views can finish against the version they hold. Publishing replaces the current version without invalidating those readers; the last reader releases a superseded version. The caller controls how many old versions remain pinned. A single publication's serialized byte budget therefore does not bound total memory retained by application-held views.

The [candidate stable contract](stable-contract.md) collects the API ownership, failure and compatibility rules. Task-table long-run validation and a fresh registry-only installation walkthrough remain release acceptance work; the existing integration recipes alone do not establish those results. No real-project integration is required for this scope.
