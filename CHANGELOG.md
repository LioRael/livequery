# Changelog

## Unreleased — stable-readiness

- Acceptance scope: macOS task-table high-load endurance has not passed and is excluded from the 0.1.0 release gate by explicit release-owner decision. Regular macOS CI and two-hour baseline remain required. Deadline/Busy must be handled; the remaining macOS delay has no established specific root cause or verified fix. Historical failures are retained in [attempt 5](docs/readiness-task-attempt5/README.md).

- Enforce detailed-result metadata byte budgets while preserving basic-query budgets.
- Report syntax errors at the unexpected token or EOF.
- Reset WATCH result/freshness state on session changes, missing metadata and I/O retries.
- Check cancellation between client dispatches; test slow-client shutdown, refresh recovery and historical snapshot release.
- Add representative task-table socket load, property/fuzz checks, registry-install acceptance and a candidate stable API contract.

## 0.1.0-alpha.4 — 2026-10-26

All seven crates published. Includes the alpha.3 hardening changes below; alpha.3 itself was not published. Four Linux/macOS × sync/async two-hour endurance gates passed; see [evidence](docs/endurance-alpha4/README.md) and [registry verification](docs/release-alpha4.json).

- Complete stuck-worker investigation fixture and CLI acceptance test.
- Optional PublishedHandle and RunningAgent lifecycle conveniences with explicit capture scheduling.
- Stable scalar ORDER BY, COUNT(*) and MAX with documented NULL/scan/output limits.
- Explicit interactive CLI sessions, memory-only history, catalog completion and coherent freshness metadata.
- Shared immutable query reads, preserving owned Snapshot APIs, plus representative task-table cost benchmarks.

## 0.1.0-alpha.3 (unpublished; included in alpha.4)

- Protocol negotiation, session/query-bound observation cursors and bounded CLI reconnection.
- Publication freshness and categorized failure diagnostics through status.
- Capped result/catalog serialization and stricter local socket/ancestor validation.
- Compatibility, property, lifecycle and secret-filter regression tests; fuzz, dependency and API CI gates.
- Reject ignored enum/container inspect attributes, reuse validated canonical socket directories and retry interrupted response EOF.
- Independent source security review and two-hour Linux/macOS endurance workflow.
- Bounded long-run load sampling with diagnostic error categories and RSS/thread/FD trends.


## 0.1.0-alpha.2

- Independent Core, Query, Agent and Transport crates; compatible livequery facade.
- Root examples/basic, examples/concurrent, examples/async-server and benches packages.

- Recursive JSON Schema and root enums; nested custom fields now require JsonSchema.
- VecDeque, sorted map values and shared Mutex/RwLock collection adapters.
- Remote access requires explicit Published snapshots; ordinary expose is local-only.
- Cooperative query deadlines and revision-aware WATCH with Busy retries.
- Real synchronous/Tokio service benchmarks and reproducible comparison probes.

Migration: derive JsonSchema for nested types, register with expose_published, and refresh at application checkpoints. All seven alpha.2 crates are published on crates.io.


## 0.1.0-alpha.1 — 2026-10-26

First experimental release. APIs and the local protocol may change during alpha.

Available on crates.io: `livequery`, `livequery-derive`, and `livequery-cli` at `0.1.0-alpha.1`.

- Explicit source registration with lifetime guards, static top-level type schemas, owned snapshots, and nonblocking application lock acquisition.
- Inspect derive with field omission, generic field bounds, and renamed dependency support.
- Bounded SELECT/projection/filter/LIMIT queries with boolean expressions, NULL semantics, and exact integer handling.
- Local Unix Socket agent with up to sixteen active clients, per-client I/O deadlines, private directory checks and bounded response encoding.
- CLI discovery, schema, query, JSON/table output, and polling WATCH.
- Ordinary-thread and Tokio examples, external consumer and compile-fail checks, Linux/macOS CI, Rust 1.88 MSRV checks, and reproducible microbenchmarks.

Limitations: no recursive schema, ordering, JOIN, history, Windows transport or arbitrary process attachment. WATCH may miss intermediate changes. Custom source callbacks and serializers remain trusted, non-preemptible application code. This is not a production hard-latency guarantee.
