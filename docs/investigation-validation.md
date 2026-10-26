# Alpha.4 investigation validation

The published alpha.4 release implements the repository investigation path described in [stuck-jobs.md](stuck-jobs.md). It includes the alpha.3 hardening changes.

- Native Cargo check, strict Clippy, fmt, 49 tests/doc-tests, rustdoc, eight compile-fail cases, independent facade/component consumers and all seven crate packages passed locally.
- Rust 1.88 workspace tests passed. The CLI uses rustyline 17 to preserve this MSRV; the newer 19 release requires compiler functionality absent in 1.88.
- Five-library patch API checks against alpha.2 passed. QueryResult and owned Snapshot APIs remain available; richer metadata has a separate capability/API.
- RustSec audit of the updated root lockfile (117 dependencies) passed without advisories or warnings. The independent consumers/workload/fuzzer lockfiles are also covered by CI.
- The actual jobs service/CLI acceptance checks sorted diagnosis, aggregates, live ingress, repair observed through WATCH, session error recovery and shutdown. A pseudo-terminal exercises Tab completion, history recall and quit; it drains output while the terminal restores its mode.
- ASan/libFuzzer query/wire smoke: 741,246 executions in 61 seconds, no reported crash. This is bounded fuzz evidence, not exhaustive coverage.

[Task-table performance measurements](jobs-performance.md) retain raw data, source hashes and the benchmark binary hash. They cover row/string sizes, allocation counts, capture/publish time, and loopback TCP tail latency with publication frequency/observer variants. Short local runs guide initial settings; they do not establish a production SLO.

The alpha.3 independent security review and endurance run belong to their recorded source commit. Neither is claimed as full security review or multi-hour endurance evidence for the changed alpha.4 implementation. Linux/macOS CI must pass for the final PR head. Real-project integration, JOIN/GROUP BY, key-based WATCH diffs and adoption claims remain outside this increment.

The alpha.4 [four-platform/engine two-hour endurance gate](endurance-alpha4/README.md) passed on behavior commit `932841ac9fda5848f6fcab44bbf4ec2d37320d1d`; raw reports and toolchains are retained. Alpha.3 endurance was canceled and is not release evidence.

Release: PR #2 and #3 were merged in order. Main commit `28793e3c969b8536cc511c48b1eb39bdd1424571` passed [CI](https://github.com/LioRael/livequery/actions/runs/40259807486) and [Stability gates](https://github.com/LioRael/livequery/actions/runs/40259807472). All seven published crate archives were downloaded from crates.io, their SHA-256 compared with registry metadata, and their embedded source commit checked against this clean main commit. Versions are not yanked; see [verification record](release-alpha4.json). Tag `v0.1.0-alpha.4` points at that source. Later release-documentation changes are not new tested binaries.
