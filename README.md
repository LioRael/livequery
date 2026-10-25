# LiveQuery

**Experimental · alpha.3 development · Rust 1.88+ · macOS / Linux**

**Turn running programs into queryable, observable worlds.**

Expose a Rust collection, then ask questions about its live state without writing a debug HTTP API:

```sql
SELECT id, username FROM players WHERE health < 20;
```

LiveQuery is an opt-in, read-only runtime inspection library and local CLI. It works with ordinary threads and Tokio applications. It does **not** inspect arbitrary processes or memory. This experimental Unix MVP is available on crates.io. It is not a production diagnostics service.

## Run the demo

From this checkout, on macOS or Linux:

```sh
cargo install --path crates/livequery-cli --locked
cargo run -p livequery-example-concurrent
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

For the published alpha.2 library (protocol 1), use:

```sh
cargo add livequery@0.1.0-alpha.2
```

Fields must implement serde `Serialize` and `schemars::JsonSchema`; the struct itself needs only `Inspect`. For local development, a path dependency on `crates/livequery` also works.

```rust
use livequery::{Inspect, Limits, Published, Query, Registry, Table};

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
let published = Published::new::<Player>(Limits::default())?;
published.refresh(&players)?;
let _registration = registry.expose_published("players", published.clone())?;
let result = Query::parse("SELECT id FROM players WHERE health < 20")?
    .execute(&registry, Limits::default())?;
```

Keep the registration guard alive. Dropping it immediately removes discovery and releases ownership when any in-flight snapshot finishes. `Table::update` applies an update or returns `Busy`; the application decides whether to retry. `Table::from_shared` accepts existing `Arc<RwLock<Vec<T>>>` state. `CollectionSource` also adapts `VecDeque`, `BTreeMap`, `HashMap`, and `Arc<Mutex<C>>`. Map values become rows (hash keys are sorted for stable output). The `Source` trait supports custom adapters. Call `published.refresh(&players)` at application-chosen checkpoints to make changes visible; failed publications preserve the previous version. `Registry::expose` remains available for trusted in-process queries only.

To allow a CLI connection, bind `local::Server` in `local::directory()` and call `serve` on a thread owned by the application. The library does not spawn a thread or require a runtime. See the complete examples:

- [Basic source](examples/basic/src/main.rs)
- [Concurrent changing players](examples/concurrent/src/main.rs), exits after 30 seconds
- [Tokio TCP echo service](examples/async-server/src/main.rs), exposes request counters and exits after 30 seconds

## What is implemented

- Named-struct and enum derive, recursive JSON Schema, explicit registration, secret-field omission.
- Whole-table snapshots with nonblocking lock acquisition and bounded derived-field serialization.
- SELECT, projection, scalar comparisons, AND/OR/NOT, parentheses, IS NULL, LIMIT.
- Unix Socket discovery, schema/query protocol, machine-readable errors, CLI tables and JSON.
- Revision-aware polling WATCH skips unchanged publications, with one retained result and no server-side queues.
- Tests, external consumers, compile-fail checks, real process/CLI smoke tests, benchmarks, and Linux/macOS/MSRV CI.

Defaults: 10,000 scanned rows, 1,000 output rows, 1 MiB serialized snapshot/result, 4 KiB SQL, 256 SQL tokens, 32 predicate nesting levels, and a cooperative 100 ms query deadline. Oversized snapshots fail; results limited by LIMIT report `truncated`.

## Boundaries

No ORDER BY, JOIN, aggregation, history, nested-field queries, Windows transport, write operations, or arbitrary PID attachment. Schema includes Rust type names and JSON Schema with recursive definitions and enum variants. Nested custom types derive `Serialize` and `JsonSchema`; root enums additionally derive `Inspect` and appear in a `value` column. See [schema tests](crates/livequery/tests/schema_collection.rs).

Each changed publication is scanned; unchanged revisions skip the snapshot and scan. Large collections fail before scanning; use dedicated diagnostic views. Polling misses intermediate states. Same-user processes can read exposed data. Custom serializers/adapters run only during explicit application capture/publication or trusted in-process queries, and cannot be forcibly preempted. Remote queries read only validated owned JSON snapshots. Use short bounded adapters and dedicated inspection views for latency-sensitive applications.

## Stability work

This checkout is the unpublished alpha.3 candidate. Its CLI negotiates protocol 2; build the CLI from this checkout for its examples. `livequery status --pid <pid> --json` reports snapshot age, last publication time and categorized failures. WATCH retries up to three consecutive I/O interruptions and binds cursors to the Agent session and exact query. Existing alpha.2 CLI requests remain accepted. Use a path dependency for candidate APIs.

See [stable release gates and remaining evidence](docs/stability.md). No stable-release or production-readiness claim is made.

## Workspace

Core, Query, Agent and Transport are independent crates under `crates/`; `livequery` remains a thin compatible entry point. Runnable packages live in `examples/basic`, `examples/concurrent`, and `examples/async-server`; benchmarks live in `benches/`. See the [dependency boundaries and directory tree](docs/design.md#workspace-layout).

## Verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
python3 scripts/consumer-check.py
cargo doc --workspace --no-deps
cargo build --workspace --bins
python3 scripts/smoke.py
cargo bench -p livequery-benchmarks --bench inspection
```

The [acceptance matrix](docs/acceptance.md) maps the original requirements to executable evidence and remaining roadmap.

Read [research and decisions](docs/design.md), [query and protocol](docs/query.md), [security and consistency](docs/security.md), and [measured performance](docs/performance.md). Licensed under MIT or Apache-2.0.

Release scope: [CHANGELOG](CHANGELOG.md). Vulnerability reporting: [SECURITY.md](SECURITY.md).

Published packages: [livequery](https://crates.io/crates/livequery/0.1.0-alpha.2), [livequery-derive](https://crates.io/crates/livequery-derive/0.1.0-alpha.2), [livequery-cli](https://crates.io/crates/livequery-cli/0.1.0-alpha.2).

Independent packages: [Core](https://crates.io/crates/livequery-core/0.1.0-alpha.2), [Query](https://crates.io/crates/livequery-query/0.1.0-alpha.2), [Agent](https://crates.io/crates/livequery-agent/0.1.0-alpha.2), [Transport](https://crates.io/crates/livequery-transport/0.1.0-alpha.2).
