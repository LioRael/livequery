# Measured performance (alpha.2 candidate)

Measured 2026-10-26 on macOS arm64, Apple M2 Pro, 16 GiB RAM, Rust 1.99.0, optimized Cargo profile and default allocator. Raw data and harnesses are committed. These are local diagnostic experiments, not production guarantees or competitor rankings. [Alpha.1 measurements](performance-alpha1.md) describe the older live-callback design and must not be used as alpha.2 results.

## Actual application cost

A real 64-byte TCP echo workload runs on either OS threads or a two-worker Tokio runtime. Four persistent Python asyncio clients generate closed-loop load on the same machine. Each request atomically updates two counters. Separate observers query a Unix socket every 10 ms after the previous response. Application publication copies two counters under its existing mutex and captures/serializes outside it; default publication interval is 10 ms.

Each row below is the median of three separate child processes, with case order rotated. The load lasts two seconds after 100 ms warmup; the host lifetime is three seconds. CPU is child user+system time over its entire lifetime, excluding compilation and the load generator. RSS is that child's maximum resident set, not incremental allocation or a leak measurement. p99 is the median of per-run sample quantiles, not a pooled percentile. Python load generation, machine noise and short runs constrain inference. The manual endpoint is intentionally less capable than LiveQuery; this is a cost reference, not a feature-equivalent comparison.

| Engine | Mode | Observers | Publish ms | Requests/s | Request p99 µs | Host CPU s | Peak RSS MiB |
|---|---|---:|---:|---:|---:|---:|---:|
| sync | baseline | 0 | — | 22076 | 285 | 0.461 | 2.23 |
| sync | manual | 1 | — | 21709 | 329 | 0.461 | 2.34 |
| sync | published | 0 | 10 | 21901 | 315 | 0.481 | 2.81 |
| sync | published | 1 | 10 | 21761 | 327 | 0.479 | 3.09 |
| sync | published | 4 | 10 | 21495 | 418 | 0.475 | 3.12 |
| sync | published | 4 | 50 | 20692 | 398 | 0.467 | 3.09 |
| async | baseline | 0 | — | 22291 | 301 | 0.637 | 2.44 |
| async | manual | 1 | — | 21791 | 377 | 0.640 | 2.56 |
| async | published | 0 | 10 | 23708 | 291 | 0.662 | 2.98 |
| async | published | 1 | 10 | 23128 | 327 | 0.659 | 3.31 |
| async | published | 4 | 10 | 22866 | 419 | 0.658 | 3.27 |
| async | published | 4 | 50 | 20904 | 455 | 0.631 | 3.23 |

All 36 cases had zero request, observation and publication errors. Some observed throughput exceeds baseline due to run noise; this is not a speedup claim. Tail latency rises in several observed cases. The 50 ms publication cases returned `not_modified` responses, proving that polling can skip work when publication is slower than polling. No universal overhead percentage is inferred. Raw results: [service-benchmark.json](service-benchmark.json).

## Disabled build

The same fixture was separately compiled without its `inspection` feature, removing the LiveQuery dependency and integration. It retains its application, manual-debug and Tokio dependencies; this is not a minimal executable-size benchmark. Both artifacts use the same toolchain/profile.

Enabled binary: 1,827,648 bytes; disabled binary: 1,154,144 bytes. The local toolchain reported a missing libLLVM for its automatic debug stripping, so these are actual produced file sizes, not fully stripped deployment sizes. No toolchain/global configuration was changed.

- Disabled sync: 20430 requests/s, 464 µs p99, 0.465 s host CPU, 2.03 MiB peak RSS (three runs).
- Disabled async: 23079 requests/s, 284 µs p99, 0.640 s host CPU, 2.28 MiB peak RSS (three runs).

Raw: [disabled-benchmark.json](disabled-benchmark.json). Compilation without inspection performs no publication, agent polling or observation; startup latency was not separately measured.

## Published snapshot queries and WATCH

Thirty samples per size, three fields per row, two projected fields, LIMIT 100, matching all rows. Query parsing and application capture occur before timing; measurements include cloning the published view, scan/filter/projection and result validation. These differ from alpha.1 capture-inclusive timing.

| Rows | p50 µs | p99 µs | Unchanged Watch polls/s |
|---:|---:|---:|---:|
| 100 | 45 | 101 | 30,770,414 |
| 1000 | 225 | 243 | 25,595,361 |
| 10000 | 2591 | 3758 | 29,655,621 |

Unchanged Watch is an in-process tight loop checking the revision; it is not a socket round-trip rate. Changed publications still require independent full scans per subscriber. The remaining CSV scenarios measure 100-row publication/update attempts at unpaced, requested 100 Hz and 1,000 Hz rates, with 0/1/8/32 watchers at requested 10/100 Hz. Capture/publication cost is included on the writer thread; the `busy` counter includes failed publication attempts. Readers poll sequentially, so requested rates are not achieved automatically. Busy/errors are counted rather than silently treated as successful work. Each Watch retains a full last result, so memory scales with subscriber count and result size.

The entire microbenchmark took 10.09 s wall, 4.08 s user CPU and 0.06 s system CPU, with 27,279,360 bytes maximum RSS. This is a whole-process peak, not per-subscriber allocation attribution. Raw: [benchmark-alpha2.csv](benchmark-alpha2.csv), [resource report](benchmark-alpha2-resources.txt).

## Sustained diagnostic load

One 30-second load run for each engine, four observers, 10 ms publication. This checks continued service and cleanup; thirty seconds is not a production endurance or leak certification.

- sync: 546,872 requests, 6,165 observations, p99 764 µs, host peak RSS 3.05 MiB; request/observation/publication errors 0/0/0.
- async: 614,989 requests, 6,430 observations, p99 499 µs, host peak RSS 3.22 MiB; request/observation/publication errors 0/1/1.

The async run recorded one failed observation and one failed publication attempt, while all application requests succeeded. This harness counts all diagnostics failures, including Busy, together and did not record their categories; it cannot attribute these two events more precisely. Failed publication preserves the previous snapshot and the next application tick tries again. This is observable snapshot sampling, not lossless event delivery.

Raw: [soak-benchmark.json](soak-benchmark.json). Maximum RSS alone cannot establish stable memory over time. Production-tail behavior, allocator attribution, fairness under hostile same-user load and multi-hour endurance remain outside these measurements.

## Reproduce

```sh
cargo bench -p livequery-benchmarks --bench inspection
cargo build --release --manifest-path experiments/workload/Cargo.toml --locked
python3 scripts/workload-smoke.py
python3 scripts/benchmark-service.py
cargo build --release --no-default-features --features async --target-dir experiments/workload/target/disabled --manifest-path experiments/workload/Cargo.toml --locked
python3 scripts/benchmark-service.py --only baseline --binary experiments/workload/target/disabled/release/livequery-workload --output docs/disabled-benchmark.json
python3 scripts/benchmark-service.py --only soak --repeats 1 --seconds 30 --output docs/soak-benchmark.json
```

Run benchmarks without concurrent compilation. `os.wait4` collects resources for the already-built host child only; the same script supports Linux RSS unit conversion. No timing threshold is used in CI. Never remove safety bounds solely to improve these numbers.
