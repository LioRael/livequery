# Stable release gates

This checkout also includes alpha.4 investigation improvements. See [investigation-validation.md](investigation-validation.md); the alpha.3 evidence below applies only to that earlier source revision.

Alpha.3 is an unpublished hardening candidate. Current measured evidence is in [stability-validation.md](stability-validation.md). Passing these checks is necessary, not evidence of universal production suitability. The first stable scope is local, read-only diagnostics on Linux/macOS, with trusted same-user processes, no hard real-time or lossless-event guarantee.

| Gate | Implementation / evidence | Remaining acceptance |
|---|---|---|
| API compatibility | cargo-semver-checks against alpha.2, external consumers, MSRV 1.88 | Review any intentional breaking change before adjusting baseline |
| Wire compatibility | Hello protocol negotiation; v2 observation payload inside existing Ok envelope; alpha.2 commands remain supported | v1 conditional cursors remain session-local legacy hints; use v2 for restart safety |
| WATCH lifecycle | UUID Agent identity + exact SQL digest + source revision; client clears caches and retries at most three consecutive I/O failures | Malformed responses and incompatible agents fail closed |
| Freshness / faults | Status exposes successful publication time, monotonic age and failure categories; failed capture preserves previous data | No promise of observing every application event |
| Resource limits | Capped result/catalog serialization, bounded transport buffers, query-loop deadlines | Timeouts remain cooperative; serialized budgets are not exact heap caps |
| Security regression | Parser/wire fuzz target, generated query properties, private socket/ancestor checks | Independent focused source review completed with fixes; see [review scope and limits](security-review-alpha3.md) |
| Dependency health | Pinned cargo-audit CI, weekly checks | Resolve or explicitly assess every new advisory; never silently ignore |
| Sustained load | Classified request/observation/publication failures; RSS, thread and FD samples every five seconds | Four Linux/macOS × sync/async two-hour runs passed on alpha.4; see [raw evidence and scope](endurance-alpha4/README.md) |
| Integration scope | Independent registry/facade/component consumers and ordinary-thread/Tokio TCP service fixtures | Real-project integration is explicitly excluded from the requested acceptance scope |

## Initial load gate

The fixture's bounded target is one diagnostic row, four observers polling every 10 ms, publication every 10 ms, four TCP clients. No application request failures; only Busy diagnostic failures are accepted, below 1% per observation and per actual publication attempt. This is a regression budget, not an externally validated service SLO. Actual publication attempts and failures are counted separately; requested rates are not assumed to be achieved.

After ten seconds warmup, resource samples must be available, thread range ≤1, FD range ≤32 (transport has sixteen active slots), and RSS range ≤8 MiB. These thresholds identify obvious regressions in this small fixture; they do not establish a general memory limit. Latency samples retain only the last 100,000 completed requests to bound load-generator memory. CPU/maxRSS belong to the host child. Sampling and the Python client run on the same machine and can affect latency.

Normal push/PR CI runs two minutes per engine on Linux and macOS. The manual Stability gates workflow runs ten minutes per engine. The Two-hour endurance workflow runs each engine for 7,200 seconds on separate Linux/macOS workers and retains raw results with source/binary identity. All four jobs must pass before closing the fixture endurance gate. A single short local run cannot close the multi-hour endurance gate. Real-project integration is not required for this acceptance scope; conclusions remain limited to the documented fixtures. Baseline-relative throughput/tail budgets need controlled hardware and a representative application, not a universal percentage chosen from the echo fixture.

## Compatibility policy

The Rust library API baseline is alpha.2; new additions preserve its source API. Public errors expose stable `code()` categories, while message wording is not a contract. Protocol versions are independent of crate versions. The CLI requires protocol 2; older Agents produce an explicit incompatibility diagnostic. Protocol 1 requests remain accepted for old clients, with their original limitations. Cursor/session identifiers are cache hints, not credentials. Changing query/session/source forces a full replacement; no server subscription is retained.

Seven crates share release versions. Patch releases must preserve documented stable APIs, error categories, default safety boundaries and MSRV. Raising MSRV requires a documented minor release. Before 1.0, any breaking change requires an explicitly documented minor release; prerelease migrations remain clearly documented. Automated SemVer checking does not validate wire semantics or proc-macro behavior: external consumers, protocol fixtures and compile-fail tests remain required.

Once a stable line exists, the latest stable minor is the supported line; security fixes may require upgrading to its latest patch. There is no paid support or response SLA. Private reporting is documented in SECURITY.md. Optional ORDER BY, history, JOIN, Web/MCP and Windows are not stable-release gates for the scope above.
