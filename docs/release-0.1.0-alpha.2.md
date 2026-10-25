LiveQuery 0.1.0-alpha.2 is available on crates.io.

This release separates Core, Query, Agent and Transport into independent crates, retains the compatible `livequery` entry point, and moves runnable examples and benchmarks to the workspace root.

- Recursive JSON Schema, enum roots, and shared Vec/VecDeque/BTreeMap/HashMap adapters.
- Remote queries read explicitly published immutable snapshots and never invoke application capture callbacks.
- Revision-aware WATCH skips unchanged publications; bounded queries and local-only socket transport remain in place.
- Real sync/Tokio service measurements, comparison probes and requirement acceptance documentation.

### Install

```sh
cargo add livequery@0.1.0-alpha.2
cargo install livequery-cli --version 0.1.0-alpha.2 --locked
```

### Migration from alpha.1

Nested fields need `Serialize` and `JsonSchema`. Use `Published`, `expose_published` and explicit application capture/publication checkpoints for remote access; ordinary `expose` is local-only. Existing `livequery::...` imports and `livequery::local::Server` remain available through the facade.

Validation includes Linux/macOS and Rust 1.88 CI, 30 regression/doc tests, external consumers and compile-fail checks, real CLI/service smoke tests, benchmark execution and seven-crate package verification. Measurements retain limits and observed diagnostic failures; this experimental release is not a production hard-real-time or lossless-history system.

ORDER BY, JOIN, history implementation, Web/MCP and Windows transport remain future work.

Published source: `e163128903c85d1d82c3177b12eba77062694ac4` (PR #1 squash merge).
Registry verification confirmed all seven alpha.2 versions exist and are not yanked. A fresh registry-only CLI installation, aliased facade consumer, standalone component consumer and running host passed. The installed CLI queried schema and filtered results and observed changing WATCH frames from the published-library host; all alpha.2 dependencies resolved from crates.io rather than workspace paths.
