# LiveQuery

**Experimental · 0.1.0-alpha.1 · Rust 1.88+ · macOS / Linux**

**Turn running programs into queryable, observable worlds.**

Expose a Rust collection, then ask questions about its live state without writing a debug HTTP API:

```sql
SELECT id, username FROM players WHERE health < 20;
```

LiveQuery is an opt-in, read-only runtime inspection library and local CLI. It works with ordinary threads and Tokio applications. It does **not** inspect arbitrary processes or memory. This repository contains a working Unix MVP, not a published crate or production diagnostics service.

## Run the demo

From this checkout, on macOS or Linux:

```sh
cargo install --path crates/livequery-cli --locked
cargo run -p livequery --example concurrent
```

In a second terminal, use the PID printed by the example:

```sh
livequery list
livequery attach <pid>
livequery schema --pid <pid>
livequery query 'SELECT id, username, health FROM players WHERE health < 80' --pid <pid>
livequery watch 'SELECT * FROM players WHERE health < 80' --pid <pid> --interval-ms 300
livequery query 'SELECT * FROM players LIMIT 1' --pid <pid> --json
```

`attach` verifies connectivity and prints schemas; it does not inject code or save a global session. Pass `--pid` or `--socket` on subsequent commands. WATCH prints full replacements only when the visible result changes. Ctrl-C cancels the client; the agent retains no subscription. `--count 10` stops after ten polls, including unchanged polls.

Example output (health changes while the host runs):

```text
id  username  health
1   "Ada"     58
2   "Lin"     8
2 rows · 2 scanned
```

## Embed

Until publication, add a path dependency on `crates/livequery`. Fields must implement serde `Serialize`; the struct itself needs only `Inspect`.

```rust
use livequery::{Inspect, Limits, Query, Registry, Table};

#[derive(Debug, Inspect)]
struct Player {
    id: u64,
    username: String,
    health: u32,
    #[inspect(skip)]
    token: String,
}

let registry = Registry::new();
let players = Table::new(vec![Player {
    id: 42, username: "Ada".into(), health: 12, token: "private".into(),
}]);
let _registration = registry.expose("players", players.clone())?;
let result = Query::parse("SELECT id FROM players WHERE health < 20")?
    .execute(&registry, Limits::default())?;
```

Keep the registration guard alive. Dropping it immediately removes discovery and releases ownership when any in-flight snapshot finishes. `Table::update` applies an update or returns `Busy`; the application decides whether to retry. `Table::from_shared` accepts existing `Arc<RwLock<Vec<T>>>` state. The `Source` trait supports other collections and custom adapters.

To allow a CLI connection, bind `local::Server` in `local::directory()` and call `serve` on a thread owned by the application. The library does not spawn a thread or require a runtime. See the complete examples:

- [Basic source](crates/livequery/examples/basic.rs)
- [Concurrent changing players](crates/livequery/examples/concurrent.rs), exits after 30 seconds
- [Tokio TCP echo service](crates/livequery/examples/async-server.rs), exposes request counters and exits after 30 seconds

## What is implemented

- Named-struct derive, static field/type schemas, explicit registration, secret-field omission.
- Whole-table snapshots with nonblocking lock acquisition and bounded derived-field serialization.
- SELECT, projection, scalar comparisons, AND/OR/NOT, parentheses, IS NULL, LIMIT.
- Unix Socket discovery, schema/query protocol, machine-readable errors, CLI tables and JSON.
- Polling WATCH with one retained result and no server-side subscriber queues.
- Tests, external consumers, compile-fail checks, real process/CLI smoke tests, benchmarks, and Linux/macOS/MSRV CI.

Defaults: 10,000 scanned rows, 1,000 output rows, 1 MiB serialized snapshot/result, 4 KiB SQL, 256 SQL tokens, 32 predicate nesting levels. Oversized snapshots fail; results limited by LIMIT report `truncated`.

## Boundaries

No ORDER BY, JOIN, aggregation, history, nested-field queries, Windows transport, write operations, or arbitrary PID attachment. Schema describes top-level fields using Rust type names; it is not recursive reflection or JSON Schema. Enum and nested struct fields can be serialized/projected as JSON, but nested schemas and enum variant inventories are not generated.

Each poll scans the full source. Large collections fail before scanning; use dedicated diagnostic views. Polling misses intermediate states. Same-user processes can read exposed data. Custom serializers/adapters are trusted application code and cannot be forcibly preempted. Use short bounded adapters and dedicated inspection views for latency-sensitive applications.

## Verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
python3 scripts/consumer-check.py
cargo doc --workspace --no-deps
cargo build --workspace --examples --bins
python3 scripts/smoke.py
cargo bench -p livequery --bench inspection
```

Read [research and decisions](docs/design.md), [query and protocol](docs/query.md), [security and consistency](docs/security.md), and [measured performance](docs/performance.md). Licensed under MIT or Apache-2.0.

Release scope: [CHANGELOG](CHANGELOG.md). Vulnerability reporting: [SECURITY.md](SECURITY.md).
