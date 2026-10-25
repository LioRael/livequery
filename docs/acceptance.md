# Original prompt acceptance

Scope: the first release requirements in the supplied LiveQuery prompt. ORDER BY was explicitly deferrable; history implementation and section 8 are future directions, not required MVP features. Alpha.2 is a development candidate; crates.io alpha.1 does not contain these additions.

| Requirement | Implementation and executable evidence |
|---|---|
| Explicit typed registration and guard lifetime | Registry, Registration; inspection lifetime/empty-schema tests |
| Shared ownership and collection adapters | Table and CollectionSource for Vec/VecDeque/BTreeMap/HashMap, Arc<Mutex>/Arc<RwLock>; schema_collection tests |
| Custom structs/enums and recursive schema | Inspect + Schemars; recursive Node, optional/maps, enum roots and secret exclusion tests |
| Safe coherent snapshots | One application capture boundary; Published replacement is atomic and validates before swapping; inspection concurrency tests |
| SELECT/WHERE/projection/comparison/boolean/LIMIT | Strict bounded parser and executor; inspection tests including NULL/integer semantics |
| Local agent, discovery, PID, JSON/table CLI | local module + livequery-cli; real-process scripts/smoke.py |
| WATCH, cancellation, slow consumers, high update rate | Conditional publication revisions, full changed-result comparison; no retained agent subscriptions or queue; transport tests and benchmarks |
| Read-only access, secret filtering, finite resources | Private owner-checked UDS; caps, deadlines; transport malformed/slow clients and forbidden callback tests |
| Query cannot invoke unbounded application callbacks | Only expose_published is visible remotely; capture is an explicit trusted application action |
| Runtime independence | Core has no Tokio dependency; concurrent and async-server examples and workload engines |
| Query size/update/subscriber benchmark | benches/inspection.rs (100/1k/10k rows, 1/8/32 subscribers, 10/100 Hz polling) |
| Actual host throughput/tail/CPU/memory | experiments/workload and scripts/benchmark-service.py; reproducible results in performance.md |
| Existing-tool investigation and value | design.md, comparison.md and runnable comparison probes; scope of demonstrated advantage is explicit |
| Open-source integration/demo/docs | README, examples, copyable terminal demo, CLI smoke, licenses, security policy, CI and release checks |
| History design and extension seams | design.md: explicit bounded optional recorder, stable keys, replay/retention limitations; JSON protocol/schema adapter seam |

The implementation does not promise arbitrary-process attachment, multi-source atomicity, mutation, hard real-time deadlines, every intermediate update, exact heap caps, automatic secret detection, production suitability, or universal superiority to specialized tools. Those are outside the evidence and API contract. Optional roadmap: ORDER BY, JOIN, history/replay, object relationships, push subscriptions, Web/MCP, Windows and cross-language adapters.
