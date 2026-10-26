# Task endurance attempt 5: partial results

Run 40403709408, source 29b2f17cdf4ee49397242858384907562495e5cd. Both macOS 26 ARM engines failed the original 600-second, 1000-row/128-byte publication gate. The 5000-row stage was not started. Linux task jobs and all four baseline jobs were still running when these results were recorded.

Sync: 600.016 seconds, 13,154,350 business requests with zero errors; one capture Deadline (217.065 ms wall, 1.370 ms thread CPU), one publication Busy, seven query Busy. Async: 600.024 seconds, 16,163,811 business requests with zero errors; one capture Deadline (254.473 ms wall, 4.581 ms thread CPU), eight query Busy. Both original check-load invocations fail on publication errors; this does not certify later resource assertions.

Both reports are clean source and have binary SHA256 cc4ae973507639d4e112dd637c69183bdd783be9fc66e30f19d62286c8e6486c, matching the saved build checksums. Inside probes show default QoS 21, base 31/max 63 and nice 0. This disproves sufficiency of removing the inherited Utility clamp for the original acceptance workload. These calls spent most elapsed time off CPU; the bounded official capture samples have no absolute timestamp or continuous thread state, so they do not identify the remaining wait mechanism or a particular competing process. No retry, threshold adjustment, or release was performed.

Raw cost reports, failed workload reports, toolchain, checksum manifests, policy and cleanup records, and gate rechecks are preserved in the engine directories.
