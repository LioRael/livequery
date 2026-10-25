# Research and architecture decisions

Investigation date: 2026-10-26. Sources below are upstream documentation, not a claim that every competing system was built or benchmarked locally.

## Existing tools

| Project | Already solves | Different boundary from LiveQuery |
|---|---|---|
| [tokio-console](https://github.com/tokio-rs/console/blob/main/console-subscriber/README.md) | Task/resource telemetry via runtime tracing instrumentation and a subscriber | Explaining async scheduling is not querying arbitrary business collections. LiveQuery does not replace its diagnostics. |
| [Bevy Remote](https://docs.rs/bevy/latest/bevy/remote/index.html) and [bevy_reflect](https://docs.rs/bevy_reflect/latest/bevy_reflect/) | Reflection and remote access, including modification, of ECS entities/components | Strong choice inside Bevy; requires the application's ECS/reflect model. LiveQuery exposes opt-in read-only tables independent of ECS. |
| [interact-rs](https://interact-rs.github.io/interact/book/) | Generic online introspection, access paths, interactive completion, assignments and calls | Closest prior art. General runtime exploration is not novel. LiveQuery narrows the interface to bounded relational selection and local read-only records. |
| [tracing](https://docs.rs/tracing/latest/tracing/) / [OpenTelemetry](https://opentelemetry.io/docs/what-is-opentelemetry/) | Instrumented events/spans and collection/export of observability signals | Preserve temporal/causal evidence; LiveQuery observes present application state and does not reconstruct causality. Use both. |
| [sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) | SQL tokenization/parsing | Does not inspect application ownership or execute snapshots. |
| [DataFusion](https://datafusion.apache.org/) | Arrow-based analytical query execution and a much broader SQL engine | Appropriate for analytical workloads; Arrow conversion, planning and executor dependencies are unnecessary for a bounded diagnostic table MVP. |

No examined tool provides this exact combination of ordinary Rust collection ownership, explicit field filtering, bounded table queries, local CLI and runtime independence out of the box. That is an engineering niche, not proof of adoption or superior performance. If users primarily inspect Tokio scheduling, recommend tokio-console; inside Bevy, evaluate its native remote tooling first. Validate demand before adding a web frontend.

## Decisions

1. **Explicit registration + owned snapshots.** No pointer walking or unsafe reflection. A guard owns the registration; the catalog stores weak references. Source callbacks run after releasing the catalog lock. An in-flight request may finish after its guard is dropped.
2. **One crate for cohesive MVP modules.** `livequery` contains state/query/local modules; `livequery-derive` and `livequery-cli` require separate crates for Rust target reasons. No artificial agent/core/transport crate boundaries. Core has no Tokio dependency; Tokio is a dev-dependency for the example only.
3. **Borrow application data only during snapshot construction.** `Table` uses `try_read`; projection/filter execution takes place on owned JSON rows after unlocking. Existing `RwLock<Vec<T>>` can be adapted without redesign. `Source` supports Mutex-backed or versioned application views; implementers must use nonblocking acquisition and uphold snapshot consistency/budgets themselves.
4. **Reuse the SQL lexer; own a deliberately tiny grammar.** Full AST validation would require auditing every unsupported clause as upstream evolves. The strict parser consumes every token and rejects extra syntax. This does mean maintaining boolean precedence and NULL semantics ourselves, with tests. A fully custom lexer would unnecessarily duplicate SQL string quoting/comment behavior. A future full AST planner should be introduced only alongside additional query semantics.
5. **Serde as a value boundary, not arbitrary reflection.** The macro enumerates named fields, skips sensitive fields before serialization, reports resolved Rust type names, and bounds JSON field writes. Option, vector, map and serde enum/struct fields are representable. Types themselves remain application-owned. Nested serializers decide their own exposure. A structured recursive schema is future work.
6. **Polling full-result comparison.** Pulling naturally applies backpressure; a slow client simply polls less often. No unbounded channel, background subscription or per-client task. It costs O(collection size) per successful poll. Event-driven invalidation and immutable published views could reduce scan/lock cost but require application cooperation.
7. **Private local socket.** Up to sixteen connections are advanced with nonblocking I/O on the caller's agent thread. Slow clients have absolute deadlines; trusted source callbacks still execute serially. No network listener, remote credentials, mutation verbs or arbitrary function calls.

## History and extension seams

History is not implemented. A future opt-in recorder can consume owned snapshots, assign source/version/timestamp metadata, diff by an explicitly declared stable key, and retain a ring bounded by both bytes and time. Disk persistence must be separately enabled, permission checked, and redacted before storage. Replay cannot promise intermediate events absent from snapshots. Do not equate row index with identity or preserve secrets in a hidden history buffer.

The version-one JSON request/response shapes can support a future Web Explorer or MCP adapter; those adapters must preserve local authorization and limits. JOIN needs multi-source consistency semantics before implementation. Event-driven watch needs revision numbers and overflow/resync semantics. Cross-language support needs a language-neutral schema instead of Rust type names. None of these capabilities is shipped here.

## Risks to validate next

- Production workload lock-hold time and serializer cost, especially long strings/nested values.
- Whether top-level schema and basic predicates satisfy real troubleshooting questions.
- Requested polling rate versus actual successful observations under application contention.
- Independent source adapters may violate bounds or block indefinitely; this is a trusted-code contract.
- macOS is locally verified; Linux/macOS CI and Rust 1.88 checks run on each push. Consult the CI run for the specific commit before relying on its result. Windows transport is absent.
