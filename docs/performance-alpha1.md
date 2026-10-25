# Performance measurements

Measured locally on 2026-10-26, macOS/Darwin arm64, Apple M2 Pro, 16 GiB RAM, rustc 1.99.0 (b940084d7 2026-09-28), Cargo bench optimized profile, default allocator. These are single-run microbenchmarks, not production guarantees or a comparison against competing tools.

Raw files: [benchmark.csv](benchmark.csv), [benchmark-resources.txt](benchmark-resources.txt). Harness: [inspection.rs](https://github.com/LioRael/livequery/blob/v0.1.0-alpha.1/crates/livequery/benches/inspection.rs).

## Snapshot + query latency

Thirty samples for each size, three scalar fields per row, predicate matching every row, two projected fields, LIMIT 100. Timing includes snapshot acquisition, bounded serialization/decoding, owned row construction, filtering, projection and serialized-size checks. Parsing occurs before the timed loop. There are no concurrent updates in these rows.

| Rows | p50 | p99 (sample quantile) |
|---:|---:|---:|
| 100 | 68 µs | 82 µs |
| 1,000 | 525 µs | 616 µs |
| 10,000 | 5,734 µs | 7,543 µs |

The linear scan is visible. LIMIT does not avoid snapshot copying. Larger sources exceed the default scan budget rather than silently truncating the inspected collection. Thirty samples are insufficient for robust tail-latency claims.

## Update and WATCH contention

Each scenario runs about 400 ms on 100 rows. Subscriber counts are 0, 1, 8 and 32; requested polling rates are 10 and 100 Hz. Update rates are maximum-speed (`watch_update_0`), requested 100 Hz, and requested 1,000 Hz. Subscribers are polled sequentially on one reader thread, with a sleep after each batch. Requested rates are not guaranteed: polling time, sleep granularity and scheduling reduce actual rates.

Selected maximum-speed cases:

| Subscribers / requested Hz | Successful updates/s | Busy update attempts | Successful observations | Observation errors |
|---|---:|---:|---:|---:|
| 0 / 100 | 20,153,968 | 0 | 0 | 0 |
| 1 / 100 | 19,379,135 | 28,205 | 33 | 1 |
| 8 / 100 | 19,961,416 | 162,298 | 241 | 15 |
| 32 / 100 | 18,740,731 | 535,474 | 815 | 49 |

Counters cover the entire scenario. Busy updates are **not retried**; this exposes lost application work if a caller ignores Busy. Busy observations are counted, not treated as successful low-cost queries. Even unchanged watches rescan. In the 1,000 Hz requested update cases, actual successful updates were 801/s without watchers and 752/s with 32 watchers at requested 100 Hz, with 19 busy attempts in the latter run. These are workload measurements, not a general percentage-overhead claim.

The p50/p99 columns for WATCH rows measure single update attempts, including busy attempts, not query latency or completion-after-retry latency. Maximum-speed runs sample every thousandth attempt; paced runs sample every attempt. OS scheduling jitter and small sample counts dominate the paced measurements.

## Disabled / idle baseline

One million tight-loop updates with the same data and nonblocking write lock:

| Configuration | Successful updates/s |
|---|---:|
| Plain std RwLock, no LiveQuery integration | 164,389,191 |
| Table wrapper, no registration | 156,424,144 |
| Table wrapper, registered but no observer | 157,351,262 |

This is a microbenchmark with inlining and no real business work. It does not establish zero overhead or that registration improves performance. Registration starts no threads, performs no repeated reads and records no history; idle per-update code is the same Table write path. An application that compiles out integration has its original code path, but feature-gated binary size/startup costs have not been measured here.

## CPU and memory

The already-built benchmark executable, excluding compilation, took 10.15 s wall time, 3.70 s user CPU and 0.07 s system CPU. macOS reported 12,664,832 bytes maximum resident set size and 11,108,736 bytes peak memory footprint. These are process-wide peaks across all cases, not incremental per-subscription allocations. They include allocator/loaded executable effects. Each Watch retains one full result, so retained memory grows with subscriber count and result size.

## Reproduce

```sh
cargo bench -p livequery --bench inspection
```

For macOS process resource accounting, first build using `cargo bench -p livequery --bench inspection --no-run`, then pass the executable path printed by Cargo to `/usr/bin/time -l`. Redirect stdout to a CSV and stderr to a resource report. Use `/usr/bin/time -v` on Linux. Running time around Cargo itself would conflate compiler memory with runtime memory.

The real process/CLI smoke tests also verified an ordinary concurrent program and a Tokio TCP echo service, including counters after a real echoed request. They are correctness tests, not service throughput benchmarks. CLI transport latency, sustained production service tail latency, allocator attribution, multi-process fairness and extended load/soak behavior remain unmeasured. A safety/serialization improvement may cost throughput; do not remove bounds based solely on these numbers.
