# Stable-readiness validation after alpha.4

This is unreleased work. It does not change the published alpha.4 crates or announce a stable release. Real-project integration is excluded.

## Completed local checks

- Native Rust 1.99.0: workspace check, 59 tests/doc-tests, strict all-target/all-feature Clippy, rustdoc and formatting. Rust 1.88 workspace tests passed.
- Five-library API checks against `v0.1.0-alpha.4` passed. External facade/component consumers, eight compile-fail checks, process/CLI smoke, task investigation and seven-package verification passed.
- Updated dependency audit passed, including the registry-only consumer lockfile.
- Published alpha.4 installed from crates.io: registry-only job host plus installed CLI completed diagnosis, live ingress, WATCH repair, interactive history/completion/error recovery and cleanup. Current CLI against the registry-only host passed the same walkthrough. These installation results cover alpha.4 dependencies, not a future unpublished registry version.
- Four independently confirmed correctness defects have fixes and regressions; see [incremental review](security-review-alpha4.md). Final source reinspection confirmed the fixes. Cancellation now checks between client dispatches; tests cover all sixteen ready clients, slow clients, refresh failure/recovery and repeated registration. A Core Weak-reference test confirms superseded snapshots are freed after their last reader drops (not a promise that the allocator immediately returns RSS to the OS).

## Load and fuzz evidence in progress

The task workload uses real loopback TCP business requests and diagnostic Unix sockets. Each business request updates one task. Publication copies task data while holding the business lock, releases it, then serializes. Observers rotate ORDER BY, COUNT and MAX; they validate row counts, stable ordering, field coherence, monotonic publication sequence and continuous per-query traffic.

Initial 1,000-row, 128-byte-string, 30-second synchronous smoke passed. A later 5,000-row/64-byte-string run overlapped local builds and ASan fuzz: business errors stayed zero, but query/publication Deadline errors violated the gate. It is not accepted as a load pass. Isolated reruns and dedicated CI workers must establish the supported envelope; no timeout/error gate was relaxed.

The enhanced fuzz harness varies rows, NULL strings and full-width integers, checks aggregate/order properties, and executes arbitrary query/wire inputs. Its first run exposed a harness assertion that incorrectly unwrapped a valid cooperative Deadline under CPU contention; the harness now accepts that documented error and the saved input is replayed. This was not a library panic. After replaying that saved input, the enhanced ASan/libFuzzer run completed 261,713 executions in 301 seconds without a reported library crash. Long-run CI evidence is still pending.

The isolated rerun on clean behavior commit `75d83d5304ae5c046dd00dc478f4934863634f3c` passed for both engines: 5,000 rows/96-byte strings, a 914,901-byte initial snapshot, 60 seconds each, four observers, zero business/query/publication errors, complete traffic/resource gates. Sync completed 1,453,935 business requests; async completed 1,601,024. These are short same-host observations, not portable throughput guarantees. Raw [isolated](readiness-tasks-local.json) and [contended](readiness-tasks-contended.json) reports retain their actual source/dirty flag and binary identity. The contended run remains a failure, not discarded evidence.

Task-table CI run: https://github.com/LioRael/livequery/actions/runs/40265450720. Fresh baseline endurance: https://github.com/LioRael/livequery/actions/runs/40265479241. Both run behavior commit `75d83d5304ae5c046dd00dc478f4934863634f3c`; later documentation commits do not produce new test binaries.

## Required final evidence

- Final commit CI, MSRV, API, audit, parser/wire fuzz and package/registry walkthroughs.
- Four fresh one-row OS/engine endurance runs after the behavior fixes.
- Task-table workflow: each OS/engine records three 30-second repeats per baseline/frequency/observer case, 600 seconds at 1,000 rows/128-byte strings, then 7,200 seconds at 5,000 rows/96-byte strings (initial snapshot about 915 KB under the 1 MiB budget). Four observers alternate all three queries. Preserve raw reports/toolchains, source commit, binary SHA-256, durations, samples, errors, traffic and resource gates.
- Summarize measured cost ranges and limitations after all four task jobs complete. No general production SLO, hard timeout, or adoption claim follows from these fixtures.

The old alpha.4 endurance evidence remains valid for its recorded source only. No merge or publication is part of this readiness run.

## First CI task endurance outcome

[Attempt 1](readiness-task-attempt1/README.md) failed on both macOS engines due to publication Deadline errors (42 sync / 40 async). Both Linux engines passed. All trials reached two hours with no business errors. This is not accepted release evidence. Validation is being optimized to avoid reconstructing an entire JSON tree; corrected behavior requires fresh baseline and task endurance runs.
