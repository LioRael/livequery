# LiveQuery 0.1.0-alpha.1

Explore explicitly exposed Rust application state using bounded, read-only queries:

```sql
SELECT id, username, health FROM players WHERE health < 20;
```

This experimental release includes an Inspect derive, registration lifecycle guards, safe owned snapshots, a SQL subset, local Unix Socket discovery, and a table/JSON CLI with polling WATCH. It supports ordinary threaded programs and Tokio applications without making Tokio a runtime dependency.

## Try it

```sh
git clone https://github.com/LioRael/livequery.git
cd livequery
git checkout v0.1.0-alpha.1
cargo install livequery-cli --version 0.1.0-alpha.1 --locked
cargo run -p livequery --example concurrent --locked
```

In a second terminal, use the printed PID:

```sh
livequery list
livequery query 'SELECT * FROM players WHERE health < 80' --pid <pid>
livequery watch 'SELECT * FROM players' --pid <pid> --interval-ms 300
```

Requires Rust 1.88+ and macOS or Linux. The library, derive macro and CLI are published on crates.io as `0.1.0-alpha.1`. Precompiled binaries are not included. It does not inject into arbitrary running programs.

## Boundaries

APIs and the wire protocol may change. Schema is top-level Rust type metadata, not recursive reflection. Polling rescans and can miss intermediate states. No JOIN, ORDER BY, history, Windows transport or production latency guarantee is included. Same-user local processes are trusted, and application source callbacks cannot be forcibly interrupted.

See README for integration, CHANGELOG for scope, and docs/performance.md for measured microbenchmarks and their limitations.

## Published packages

- [livequery](https://crates.io/crates/livequery/0.1.0-alpha.1)
- [livequery-derive](https://crates.io/crates/livequery-derive/0.1.0-alpha.1)
- [livequery-cli](https://crates.io/crates/livequery-cli/0.1.0-alpha.1)

Add the library with `cargo add livequery@0.1.0-alpha.1`.
