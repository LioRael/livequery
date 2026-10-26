# Stable release gates

Alpha.4 is published with investigation improvements and alpha.3 hardening. The first stable release is still a candidate: the stable-readiness working tree contains additional corrections, so published alpha.4 evidence must not be relabeled as validation of those changes. See [published validation](investigation-validation.md), the [candidate API/protocol contract](stable-contract.md), and the [incremental independent review](security-review-alpha4.md).

The first stable scope is local, read-only diagnostics on Linux/macOS, with trusted same-user processes, no hard real-time or lossless-event guarantee. Alpha.3 was not published separately. Historical hardening measurements remain in [stability-validation.md](stability-validation.md).

| Gate | Existing evidence / implementation | Remaining acceptance |
|---|---|---|
| API and semantics | Alpha.2 compatibility checks, external consumers, MSRV 1.88; proposed [stable contract](stable-contract.md) | Review/freeze the final API and query semantics, then establish the stable tag as the future baseline. Re-run checks after corrections. |
| Wire compatibility | Hello capabilities, legacy commands, v2 observation; documented client/Agent matrix | Exercise final client/version combinations; protocol 2 alone does not imply optional query support. |
| WATCH lifecycle and freshness | Session-bound cursors; readiness fixes clear session baselines and stale age; focused regressions | Final CLI/fixture regression pass. Malformed responses and incompatible Agents must fail closed. Polling remains lossy. |
| Shutdown and publication faults | Explicit stop/wait, old-data preservation, refresh recovery and view-lifetime checks | Final lifecycle regression pass, including active/slow clients. No hard shutdown-time claim for in-flight synchronous work. |
| Resource limits | Bounded snapshots/results, full detailed-object byte check, bounded transport buffers, cooperative deadlines | Verify worst permitted inputs and representative task-table load. Serialized budgets are not exact heap caps. |
| Independent review | Alpha.3 review plus alpha.4 incremental review and separate candidate verifier; four correctness findings corrected | Bind final regressions/CI to the corrected source. No third-party certification or dynamic cross-UID attack result is claimed. |
| Dependency and parser health | Pinned cargo-audit CI, weekly checks and parser/wire fuzz harness | Run final audit/property/fuzz checks after changes; assess new advisories explicitly. |
| Sustained load | Four OS/engine two-hour alpha.4 one-row runs passed; [raw evidence](endurance-alpha4/README.md) | Representative task-table long-run validation is pending; rerun required endurance on the final behavior revision. |
| Installation and investigation | Repository jobs/CLI walkthrough, facade/component consumers and package verification | Published alpha.4 registry-only installation and full investigation passed locally; the current CLI also passed against that registry-built host. Final candidate CI remains required. |
| Integration scope | Repository ordinary-thread/Tokio and task-service fixtures | Real-project integration is explicitly excluded; no adoption or production-SLO claim follows from fixture results. |

## Load acceptance and evidence boundaries

The fixture's bounded target is one diagnostic row, four observers polling every 10 ms, publication every 10 ms, four TCP clients. No application request failures; only Busy diagnostic failures are accepted, below 1% per observation and per actual publication attempt. This is a regression budget, not an externally validated service SLO. Actual publication attempts and failures are counted separately; requested rates are not assumed to be achieved.

After ten seconds warmup, resource samples must be available, thread range ≤1, FD range ≤32 (transport has sixteen active slots), and RSS range ≤8 MiB. These thresholds identify obvious regressions in this small fixture; they do not establish a general memory limit. Latency samples retain only the last 100,000 completed requests to bound load-generator memory. CPU/maxRSS belong to the host child. Sampling and the Python client run on the same machine and can affect latency.

Normal push/PR CI runs two minutes per engine on Linux and macOS. The manual Stability gates workflow runs ten minutes per engine. The Two-hour endurance workflow runs each engine for 7,200 seconds on separate Linux/macOS workers and retains raw results with source/binary identity. All four jobs must pass on the applicable behavior revision before closing that fixture endurance gate. A single short local run cannot close the multi-hour endurance gate. Real-project integration is not required for this acceptance scope; conclusions remain limited to the documented fixtures. Baseline-relative throughput/tail budgets need controlled hardware and a representative application, not a universal percentage chosen from the echo fixture.

Task-table acceptance adds at least 1,000 rows and a case near the configured byte budget, repeated sort/count/max queries through the diagnostic socket, and sustained business traffic. Record publication/query errors, actual traffic and sampling coverage, resource ranges, and business latency alongside source/binary identities. Compare publication frequency and observer-count variants on the same setup. The one-row alpha.4 pass and short local task benchmarks cannot substitute for those pending results.

## Compatibility policy

The [candidate stable contract](stable-contract.md) defines ownership, query semantics and the CLI/Agent version matrix. The current Rust library API baseline is alpha.4; new additions preserve its source API. Public errors expose stable `code()` categories, while message wording is not a contract. Protocol versions are independent of crate versions. The CLI requires protocol 2; older Agents produce an explicit incompatibility diagnostic. Protocol 1 requests remain accepted for old clients, with their original limitations. Cursor/session identifiers are cache hints, not credentials. Changing query/session/source forces a full replacement; no server subscription is retained.

Seven crates share release versions. Patch releases must preserve documented stable APIs, error categories, default safety boundaries and MSRV. Raising MSRV requires a documented minor release. Before 1.0, any breaking change requires an explicitly documented minor release; prerelease migrations remain clearly documented. Automated SemVer checking does not validate wire semantics or proc-macro behavior: external consumers, protocol fixtures and compile-fail tests remain required.

Once a stable line exists, the latest stable minor is the supported line; security fixes may require upgrading to its latest patch. There is no paid support or response SLA. Private reporting is documented in SECURITY.md. ORDER BY, aggregates and in-memory session history are implemented and belong to the candidate contract. JOIN, GROUP BY, nested-field queries, unique-key WATCH diffs, Web/MCP and Windows transport remain outside this scoped stable release.

## 0.1.0 task-table acceptance scope (2026-10-27)

The release owner explicitly limited the task-table endurance release gate to Linux sync/async. macOS remains covered by regular correctness/smoke CI and the four-cohort Linux/macOS two-hour baseline. All original Linux task durations, repetitions, traffic, 100ms wall deadlines, error and resource gates remain required. This is a disclosed acceptance-scope change, not a timeout fix or a passing macOS task result.

macOS task-table high-load acceptance has **not passed**. The original 600-second, 1,000-row/128-byte trials at 29b2f17 each recorded a publication Deadline despite ordinary default scheduling; their 5,000-row stage did not run. Historical failures and diagnostics remain in [attempt 5](readiness-task-attempt5/README.md) and [the last correlated investigation](readiness-process-policy/ci-40413152826/README.md). The remaining cross-process delay has no established specific root cause or verified fix. Applications using macOS task-table workloads must handle Deadline/Busy and retain the prior successful snapshot; no high-load success or latency guarantee is claimed for this release.
