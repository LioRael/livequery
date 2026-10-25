# Research and architecture decisions

Investigation date: 2026-10-26. Sources below are upstream documentation; executable comparison probes are described in [comparison](comparison.md). These probes are capability checks, not competitor performance rankings.

## Existing tools

| Project | Already solves | Different boundary from LiveQuery |
|---|---|---|
| [tokio-console](https://github.com/tokio-rs/console/blob/main/console-subscriber/README.md) | Task/resource telemetry via runtime tracing instrumentation and a subscriber | Explaining async scheduling is not querying arbitrary business collections. LiveQuery does not replace its diagnostics. |
| [Bevy Remote](https://docs.rs/bevy/latest/bevy/remote/index.html) and [bevy_reflect](https://docs.rs/bevy_reflect/latest/bevy_reflect/) | Reflection and remote access, including modification, of ECS entities/components | Strong choice inside Bevy; requires the application's ECS/reflect model. LiveQuery exposes opt-in read-only tables independent of ECS. |
| [interact-rs](https://interact-rs.github.io/interact/book/) | Generic online introspection, access paths, interactive completion, assignments and calls | Closest prior art. General runtime exploration is not novel. LiveQuery narrows the interface to bounded relational selection and local read-only records. |
| [tracing](https://docs.rs/tracing/latest/tracing/) / [OpenTelemetry](https://opentelemetry.io/docs/what-is-opentelemetry/) | Instrumented events/spans and collection/export of observability signals | Preserve temporal/causal evidence; LiveQuery observes present application state and does not reconstruct causality. Use both. |
| [sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) | SQL tokenization/parsing | Does not inspect application ownership or execute snapshots. |
| [DataFusion](https://datafusion.apache.org/) | Arrow-based analytical query execution and a much broader SQL engine | Appropriate for analytical workloads; Arrow conversion, planning and executor dependencies are unnecessary for a bounded diagnostic table MVP. |

The examined interfaces do not provide this exact combination of ordinary Rust collection ownership, explicit field filtering, bounded table queries, local CLI and runtime independence out of the box. That is an engineering niche, not proof of adoption or superior performance. If users primarily inspect Tokio scheduling, recommend tokio-console; inside Bevy, evaluate its native remote tooling first. Validate demand before adding a web frontend.

## Decisions

1. **Explicit registration + owned snapshots.** No pointer walking or unsafe reflection. A guard owns the registration; the catalog stores weak references. Trusted local source callbacks run after releasing the catalog lock. Remote sources must be `Published` owned snapshots. An in-flight request may finish after its guard is dropped.
2. **Independent crates with one-way dependencies.** Core owns schemas, registration, collection adapters and publication. Query depends only on Core and SQL parsing. Transport owns protocol/framing, socket security, deadlines and client lifecycle, with no Query dependency. Agent dispatches protocol requests through Query over a published-only registry and composes a local transport server. CLI depends directly on Query result types and Transport. The thin `livequery` facade preserves existing imports and `local::Server`; it contains no duplicated implementation. Core and Agent have no Tokio dependency. Tokio belongs to the independent async example package.
3. **Borrow application data only during snapshot construction.** `Table` uses `try_read`; projection/filter execution takes place on owned JSON rows after unlocking. Existing `RwLock<Vec<T>>` can be adapted without redesign. `Source` supports Mutex-backed or versioned application views; implementers must use nonblocking acquisition and uphold snapshot consistency/budgets themselves. `CollectionSource` supports Vec, VecDeque, BTreeMap and sorted HashMap values through Mutex/RwLock; maps expose values, not implicit key columns.
4. **Reuse the SQL lexer; own a deliberately tiny grammar.** Full AST validation would require auditing every unsupported clause as upstream evolves. The strict parser consumes every token and rejects extra syntax. This does mean maintaining boolean precedence and NULL semantics ourselves, with tests. A fully custom lexer would unnecessarily duplicate SQL string quoting/comment behavior. A future full AST planner should be introduced only alongside additional query semantics.
5. **Serde as a value boundary, not arbitrary reflection.** The macro enumerates named fields, skips sensitive fields before serialization, reports resolved Rust type names, and bounds JSON field writes. Option, vector, map and serde enum/struct fields are representable. Types themselves remain application-owned. Nested serializers decide their own exposure. Schemars supplies per-column recursive JSON Schema, `$defs` and enum variant inventories. Custom nested types implement JsonSchema; root enums are one `value` column using Serde representation.
6. **Revision-aware polling.** Pulling naturally applies backpressure; a slow client simply polls less often. Published revisions skip snapshot copies and scans when unchanged. A new revision produces a full query result, compared with the last result; there is no incremental row delta algorithm. Each changed publication costs O(collection size) per subscriber. There is no unbounded channel, background subscription or per-client task.
7. **Private local socket.** Up to sixteen connections are advanced with nonblocking I/O on the caller's agent thread. Slow clients have absolute deadlines. Remote requests use only validated owned snapshots and never execute source callbacks. Query loops have cooperative deadlines and finite row/byte caps. No network listener, remote credentials, mutation verbs or arbitrary function calls.

## History and extension seams

History is not implemented. A future opt-in recorder can consume owned snapshots, assign source/version/timestamp metadata, diff by an explicitly declared stable key, and retain a ring bounded by both bytes and time. Disk persistence must be separately enabled, permission checked, and redacted before storage. Replay cannot promise intermediate events absent from snapshots. Do not equate row index with identity or preserve secrets in a hidden history buffer.

The version-one JSON request/response shapes can support a future Web Explorer or MCP adapter; those adapters must preserve local authorization and limits. JOIN needs multi-source consistency semantics before implementation. Push-based watch would need overflow/resync semantics beyond the existing pull revisions. Cross-language support can consume the JSON Schema but still needs transport/API compatibility policy. None of these capabilities is shipped here.

## Risks to validate next

- Production workload lock-hold time and serializer cost, especially long strings/nested values.
- Whether basic predicates and JSON projection satisfy real troubleshooting questions.
- Requested polling rate versus actual successful observations under application contention.
- Explicit capture still invokes trusted serializers/adapters; applications choose capture frequency and thread. Remote requests cannot invoke them.
- macOS is locally verified; Linux/macOS CI and Rust 1.88 checks run on each push. Consult the CI run for the specific commit before relying on its result. Windows transport is absent.


## Workspace layout

```text
livequery/
├── crates/
│   ├── livequery-core/       # schemas, ownership, snapshots, collections
│   ├── livequery-derive/     # derive works with facade or standalone Core
│   ├── livequery-query/      # bounded SQL and WATCH
│   ├── livequery-agent/      # published-only dispatch, composed server
│   ├── livequery-transport/  # wire protocol, socket server/client and discovery
│   ├── livequery-cli/        # executable client
│   └── livequery/            # compatible embedding facade
├── examples/
│   ├── basic/
│   ├── concurrent/
│   └── async-server/
├── benches/                 # livequery-benchmarks package
└── docs/
```

The transport server accepts a trusted request handler; it does not know about registries or execute SQL. Its handler must return promptly. `livequery_agent::Agent::handle` is usable without a socket; `livequery_agent::local::Server` provides the bounded published-data implementation. `Registry::published_only` supplies the access boundary without exposing private registry internals. Root examples and benchmarks are workspace packages with `publish = false`.

Release dependency order: derive → core → query/transport → agent → facade/CLI. Local `cargo package` verification uses Cargo's temporary registry for all seven publishable packages. All seven alpha.2 crates have been published; alpha.1 publication metadata is historical.
