# Changelog

## 0.1.0-alpha.1 — 2026-10-26

First experimental release. APIs and the local protocol may change during alpha.

- Explicit source registration with lifetime guards, static top-level type schemas, owned snapshots, and nonblocking application lock acquisition.
- Inspect derive with field omission, generic field bounds, and renamed dependency support.
- Bounded SELECT/projection/filter/LIMIT queries with boolean expressions, NULL semantics, and exact integer handling.
- Local Unix Socket agent with up to sixteen active clients, per-client I/O deadlines, private directory checks and bounded response encoding.
- CLI discovery, schema, query, JSON/table output, and polling WATCH.
- Ordinary-thread and Tokio examples, external consumer and compile-fail checks, Linux/macOS CI, Rust 1.88 MSRV checks, and reproducible microbenchmarks.

Limitations: no recursive schema, ordering, JOIN, history, Windows transport or arbitrary process attachment. WATCH may miss intermediate changes. Custom source callbacks and serializers remain trusted, non-preemptible application code. This is not a production hard-latency guarantee.
