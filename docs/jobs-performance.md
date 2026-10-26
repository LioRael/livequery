# Task-table cost envelope

Local macOS arm64 trial, Rust 1.99, optimized build. Raw data and source/binary hashes: [jobs-benchmark-macos.json](jobs-benchmark-macos.json). These are short fixture measurements, not a production SLO or a capacity guarantee.

Reproduce with `cargo bench -p livequery-benchmarks --bench jobs`. Micro-operations run 20 times. The allocator counter counts allocation calls/total allocated bytes on the measured thread, not peak resident memory. Setup is excluded from publication timings; publication includes validation and swapping/dropping the previous version. Capture time includes acquisition and snapshot construction under the table read lock, plus surrounding work; copy time is an upper bound on holding the lock while copying the lightweight view.

## Snapshot/query costs

The table below uses 128-byte state strings and projects/sorts two fields, returning 20 rows.

| Rows | Capture µs | Copy view under lock µs | Owned read µs / allocations | Shared read µs / allocations | Sorted query µs / allocations | Publish µs |
|---:|---:|---:|---:|---:|---:|---:|
| 100 | 186.98 | 1.91 | 15.18 / 768 | 0.30 / 9 | 12.16 / 149 | 84.92 |
| 1000 | 1835.89 | 17.35 | 142.43 / 7068 | 0.31 / 9 | 53.67 / 153 | 785.05 |
| 5000 | 9160.98 | 84.41 | 690.33 / 35068 | 0.32 / 9 | 232.78 / 156 | 3991.80 |

Shared reads eliminate the whole-table clone; they do not eliminate filtering, sorting, serialization or output allocation. Numeric ordering no longer allocates temporary decimal strings. Published reads pin an immutable version, so callers retaining many views can retain old publications. Drop views promptly. The old owned-copy API remains available.

## TCP request interference

One client sends a byte over loopback TCP; the service updates a task under its business write lock and echoes the byte. Each case runs one second, with a 100 µs client pause. Observers query directly every 10 ms (there is no diagnostic socket serialization in these cases). Business latency includes TCP/scheduling/lock acquisition. Tables use 32-byte strings. Baseline has no periodic capture or observers. These single short runs are noisy and not suitable for universal percentage claims.

| Rows | Capture mode | Requested publication ms | Observers | Requests | TCP p99 µs |
|---:|---|---:|---:|---:|---:|
| 1000 | baseline | 0 | 0 | 6399 | 72.38 |
| 1000 | serialize under lock | 10 | 1 | 5648 | 1503.88 |
| 1000 | serialize under lock | 10 | 4 | 5577 | 1506.96 |
| 1000 | serialize under lock | 100 | 1 | 6226 | 82.75 |
| 1000 | serialize under lock | 100 | 4 | 6233 | 81.67 |
| 1000 | copy, unlock, serialize | 10 | 1 | 6306 | 80.00 |
| 1000 | copy, unlock, serialize | 10 | 4 | 6329 | 82.08 |
| 1000 | copy, unlock, serialize | 100 | 1 | 6381 | 75.92 |
| 1000 | copy, unlock, serialize | 100 | 4 | 6345 | 73.46 |

In this recorded trial all TCP replies were validated and no query/publication error was recorded. The harness reports categorized diagnostic failures rather than assuming Busy; reruns may show contention or scheduler effects. Earlier exploratory runs were used to refine the harness, not mixed into this report.

## Choosing initial settings

- Start with about 100–1,000 lightweight diagnostic rows and 100 ms publication. Measure the application before increasing frequency or observer count.
- Copy a compact view while holding the business lock, then release the lock before serialization. This still allocates and copies strings; avoid carrying large payloads that do not help diagnosis.
- 5,000 rows with 256-byte strings exceeded the existing 1 MiB snapshot budget in this fixture and were rejected. No default limit was raised. A serialized-byte budget is not a heap-memory cap.
- A 10 ms publisher can noticeably affect business latency when serialization holds the business lock. Use the raw measurements to choose a starting point, then repeat on representative hardware and traffic.
- This benchmark complements the stuck-worker acceptance fixture and the separate endurance tests. It does not extend alpha.3 endurance evidence to alpha.4 or prove real-project adoption.
