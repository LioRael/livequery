# Capability comparison and demonstrated value

These are narrow reproducible capability probes, not benchmark rankings or claims that a competitor cannot be extended. The isolated `experiments/comparison` workspace is not a production dependency. Its lockfile records the exact tested versions: interact 0.3.6, Bevy 0.17.3, console-subscriber 0.4.1 and console-api 0.8.1. Bevy 0.17.3 was chosen for a Rust 1.88-compatible fixture, not as a claim about the latest release.

```sh
cargo build --manifest-path experiments/comparison/Cargo.toml --bin interact --bin bevy
python3 scripts/comparison-probes.py
RUSTFLAGS='--cfg tokio_unstable' cargo run --manifest-path experiments/comparison/Cargo.toml --bin console
```

The first probe reads and assigns a nested ordinary Rust field through interact. The second reflects and changes a component field, then queries its actual ECS value through Bevy Remote HTTP. The console probe connects to the real gRPC stream and asserts that task telemetry arrives. These demonstrate that runtime exploration, reflection and task diagnostics already exist; none is claimed as a LiveQuery invention. Raw observations are in [comparison-probes.txt](comparison-probes.txt).

| Need | Observation and choice |
|---|---|
| Inspect/change ordinary Rust objects by access path | Interact already handles this. Prefer it when interactive mutation or method invocation is desired. LiveQuery deliberately offers no such remote operations. |
| Reflect ECS components and query a running Bevy world | Bevy Remote already handles this. Prefer native Bevy facilities for a Bevy-only application; do not migrate plain application state into ECS solely to use LiveQuery. |
| Diagnose task scheduling/resources | console supplies actual Tokio telemetry. LiveQuery's business-state tables cannot replace scheduling diagnostics. |
| Events, causality, exports | tracing/OpenTelemetry remain complementary. No attempt to reconstruct historical causes from a current snapshot. |
| SQL parsing | sqlparser supplies the lexer; strict small grammar and bounded executor stay local. Full AST and DataFusion were compared architecturally, not throughput-benchmarked. |
| Ordinary collections, one read-only schema/query/watch CLI, no required runtime | This is the implemented LiveQuery combination. Explicit owned publication separates remote queries from application callbacks. |

## Hand-written Debug API comparison

The same `experiments/workload` service can run with a small manual Unix diagnostic endpoint or a Published LiveQuery source. Both expose the same counters; the manual endpoint implements exactly one hard-coded `min_completed` filter. It does not implement the equivalent schema discovery, projections, compound predicates, watch comparison, structured diagnostics, or hardened nonblocking client handling. Therefore their benchmark is a host-cost reference, not a feature-equivalent ranking.

After registering once, the unchanged LiveQuery program accepts:

```sql
SELECT completed FROM counters;
SELECT bytes FROM counters WHERE completed > 100 AND bytes > 6400 LIMIT 1;
WATCH SELECT * FROM counters WHERE completed > 0;
```

The existing CLI also discovers the source and returns JSON Schema. Adding another filter/projection does not add a server endpoint. The service fixture keeps its existing Arc<Mutex<Counters>> and copies two counters at a chosen checkpoint; neither engine's architecture changes. `scripts/workload-smoke.py` asserts multiple queries and coherence against both real service engines.

This demonstrates less *per-question* application code for structured exploration than that manual endpoint. It does not prove that setup is always shorter than printing one log line, that users prefer SQL, or that LiveQuery is simpler than native tools inside their specialist domains. The published snapshot setup and scheduling are explicit costs. The evidence supports the narrow runtime-independent read-only exploration use case; wider product adoption needs actual users, not more speculative features.
