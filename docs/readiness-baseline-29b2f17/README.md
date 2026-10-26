# Two-hour baseline at 29b2f17

Run [40403717952](https://github.com/LioRael/livequery/actions/runs/40403717952) completed all four Linux/macOS sync/async baseline trials. Every raw report passed the unchanged `scripts/check-load.py --min-seconds 7200 --engine <engine>` recheck. The gate verifies errors, continuous business/observation traffic, sampling coverage and resource bounds. Busy backpressure remains visible and within the original limits. This baseline does not exercise task-table sort/count/max traffic.

Reports identify clean source `29b2f17cdf4ee49397242858384907562495e5cd`; library behavior is 5bc2613, still alpha.4. Downloaded build binaries were independently SHA-256 hashed and matched each report plus both build and measurement manifests. Original toolchain, manifests, raw reports and macOS policy/cleanup records are retained here; binaries remain in `target/baseline-40403717952`. Both macOS launch jobs exited and cleaned up successfully.

This result does not erase the macOS task-table failures in `docs/readiness-task-attempt5`, establish their root cause, or satisfy release acceptance. No rerun, gate change, merge or publication was performed for this receipt.
