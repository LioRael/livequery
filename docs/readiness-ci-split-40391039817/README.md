# Split build/measurement macOS 26 result — run 40391039817

Workflow head fe5917f30f89a74e41129e5a5537abe8d21163db. All four measurements completed on fresh macOS 26 VMs with no compilation or toolchain installation in the measurement jobs. The added 25% admission gate was removed. Original 5000-row/96-byte/four-observer/100ms workload and error gates remained unchanged; each trial ran at least 300 seconds. Build-manifest, measurement provenance, binary hashes, raw reports, runtime CPU windows and original gate outputs are retained. Local original-gate rechecks reproduced every failure.

| Variant | Engine | Capture Deadline | Query Deadline | Business errors |
|---|---|---:|---:|---:|
| Official | sync | 3 | 1 | 0 |
| Official | async | 1 | 0 | 0 |
| Diagnostic | sync | 8 | 0 | 0 |
| Diagnostic | async | 10 | 0 | 0 |

Both official reports match binary SHA256 41ce07123d6ff339bd6b81155ebc2b84ce4f4427d21ec4304e81869e81645bb2. Both diagnostic reports match c0133c0c3acaf939e182ff13f33fc7643d9dd27b0d159bc02698493272655e14. Build image was 20261013.0424.1; official sync used that image, other measurement VMs used 20261021.0445.8. All were macOS 26.6.2, three logical CPUs. The variants ran on separate VMs, so failure counts are not a controlled measurement of instrumentation overhead.

## Associated failure evidence

Diagnostic sync: 8 failed spans, 10 pre-return slow intervals, 6 thread samples all state 1, all 10 expanded fault/page-in/COW brackets zero. Overlapping 5-second host CPU windows measured 232.85–298.83% of one core (300% means all three cores).

Diagnostic async: 10 failed spans, 21 pre-return slow intervals, 10 thread samples all state 1, all 21 expanded fault/page-in/COW brackets zero. Overlapping host CPU windows measured 235.56–297.50% of one core.

Example sync failed capture validate interval: 49.530ms wall, 5.798916ms thread CPU, 0.145581ms allocator wall, zero fault bracket, state 1 sample; overlapping host window 297.08%. Example async failed capture rows interval: 84.601ms wall, 0.722708ms CPU, 13.003136ms allocator wall, zero fault bracket, two state 1 samples; overlapping host window 297.50%.

This strengthens the CPU-service-shortage explanation for the instrumented failures and rules out concurrent compilation on the measurement VM as a necessary cause. Separating compilation and disabling indexing did not solve the original workload timeouts. Host CPU includes expected benchmark/workload activity plus OS activity; five-second windows and sparse runnable samples cannot attribute every millisecond or identify the physical-host scheduler. Official slow captures have no absolute timestamp, so exact official failure/CPU association is unavailable. Do not transfer instrumented timing to official failures as direct proof, infer resource-gate success after early error assertions, or treat this as final acceptance.

No retry, additional workload, long acceptance or publication was started after this failed run. Next work must assess the measured CPU consumers and feasible resource isolation while preserving the required workload; repeating this same split setup is not a fix.
