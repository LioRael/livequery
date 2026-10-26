# Candidate contract for the first stable release

This is the contract proposed for the first non-prerelease `0.1.0`, not an announcement that it has shipped. It describes the stable-readiness working tree after alpha.4 and must be checked against the final release commit. The acceptance status is maintained in [stability.md](stability.md).

The supported scope is local, read-only diagnostics on Linux and macOS, with Rust 1.88 or later. Root and same-UID processes are trusted. Applications choose which diagnostic data to publish and when; LiveQuery is not an arbitrary process debugger, an authorization boundary against hostile same-user code, a hard real-time service, or a lossless event store. Real-project validation is excluded from this release's requested acceptance scope.

## Public API decisions

| Surface | Candidate contract | Acceptance / compatibility boundary |
|---|---|---|
| Registration | Registry entries require a live registration guard. Dropping `PublishedHandle` unregisters its table. | A reader that already owns a view can finish after unregistration. Re-registration gets a distinct source identity. |
| Publication | `refresh` captures and publishes explicitly. Failed capture or publication preserves the previous successful snapshot and records the failure. | `Busy` is backpressure, not an instruction to spin or terminate business work. Prepared snapshots passed to `publish` are validated; capture outside `refresh` requires caller-owned error accounting. |
| Scheduling | `PublishedHandle` starts no timer or capture thread. | The application owns capture frequency, concurrency and cost. Async callers must move potentially blocking capture off the executor. |
| Snapshot ownership | `SnapshotView` pins one immutable publication; `Registry::snapshot` remains an owned-copy API. | Holding views across publications retains old versions. Dropping the final reader releases a superseded version; LiveQuery does not retain an unbounded version history itself. |
| Query metadata | Detailed rows, publication sequence/time and revision refer to the version actually read. Age is sampled at acquisition. | Failure counters are independently sampled cumulative diagnostics. Network/rendering time adds to observed age; the metadata is not a transactional event log. |
| Query results | Basic `QueryResult` fields remain `columns`, `rows`, `scanned`, `truncated`. Column order follows projection. | `execute` budgets the old result shape; `execute_detailed` budgets its complete shape, including metadata. Remote envelope encoding is additionally bounded. |
| Ordering | One scalar sort key; stable ties retain source order; NULL is last in both directions. Integers retain full signed/unsigned precision. | Mixed float/integer or otherwise incompatible scalar values fail. Stable ties do not imply stable object identity across publications. |
| Aggregates | COUNT(*) counts all matches; MAX ignores NULL and returns NULL for empty/all-NULL input. | No mixed projection/aggregate or aggregate ORDER BY. LIMIT does not reduce scan cost; LIMIT 0 omits the aggregate row and sets `truncated`. |
| Budgets | Row and serialized-byte limits fail explicitly; timeouts are cooperative. | Sort, allocation, comparison and serialization are not forcibly preempted. Byte limits are not heap caps or hard response-time limits. |
| Agent lifetime | `start` binds synchronously; `stop` requests cancellation; `wait` consumes and joins. Drop requests stop and joins as fallback. | `wait` without `stop` can wait indefinitely while service runs. Join/Drop can block; use `spawn_blocking` for async shutdown. In-flight handlers are not forcibly interrupted. |
| Errors | Callers may branch on documented `Error::code()` categories. | Error text is diagnostic, not a stable machine interface. Query location tests protect useful positions, not permanent wording. |

The existing alpha.2 library baseline remains the current automated source-compatibility check. Before release, review all intentional changes and record the first stable tag as the subsequent stable compatibility baseline. Proc-macro behavior, wire semantics and CLI behavior require their own tests; library SemVer checks do not cover them.

## CLI and Agent compatibility

Crate versions and protocol versions are independent. A successful protocol-2 Hello returns capabilities; protocol 2 alone does not imply support for every alpha.4 feature.

| Client | Agent | Supported behavior |
|---|---|---|
| Alpha.2 CLI / legacy requests | Alpha.4 or stable-readiness Agent | Existing schema/query/legacy observation commands remain accepted. Legacy cursors keep their session-local limitations. |
| Alpha.4 / stable-readiness CLI | Alpha.2 Agent | Rejected because the Agent lacks the required Hello negotiation. No silent downgrade. |
| Alpha.4 / stable-readiness CLI | Alpha.3 protocol-2 Agent | Basic queries and v2 observation remain usable; query-result freshness metadata is unavailable. Alpha.3 was not published to crates.io. |
| Alpha.4 / stable-readiness CLI | Agent advertising `query_details`, `order_by`, `aggregates` | Detailed results and the corresponding new SQL features are supported. An older Agent rejects unsupported SQL; the CLI does not emulate it. |

The CLI selects QueryDetails when the negotiated Agent advertises it. Users of custom protocol clients must inspect capabilities before relying on optional operations. JSON stdout contains successful result payloads; failures/retries go to stderr. Session history is bounded and held in memory only.

V2 cursors bind Agent identity, exact SQL and source revision. I/O reconnect resets cached result/cursor/freshness. In the stable-readiness CLI, a session change without a transport error also resets the result baseline, and a result without metadata clears old freshness. Published alpha.4 lacks these last two corrections; see [the incremental review](security-review-alpha4.md).

WATCH polls snapshots. A new session produces a complete first result, then unchanged rows/schema/truncation may be suppressed. Polling may miss intermediate changes; it has no event backlog or row-level difference contract. Restarting at a different socket/PID requires selecting a new target.

## Version and support policy

Seven crates share release versions. The first stable line will preserve these documented APIs, error categories, default safety boundaries and MSRV within patch releases. Before 1.0, intentional breaking changes require a documented minor release; an MSRV increase also requires a documented minor release. Security fixes may require upgrading to the latest patch of the supported minor. There is no paid support or response SLA; private reporting is described in SECURITY.md.

JOIN, GROUP BY, nested-field queries, unique-key WATCH diffs, Windows transport and Web/MCP integrations are outside the first stable contract. They do not block this scoped release. The first stable version still requires final source-bound CI, representative fixture results and registry-install acceptance; this document does not substitute for them.
