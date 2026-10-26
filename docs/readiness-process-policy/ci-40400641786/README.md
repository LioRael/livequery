# Hosted policy correction candidate

[Run 40400641786](https://github.com/LioRael/livequery/actions/runs/40400641786)
completed on `be31c4c19b2b723f07e30d99cf5c511bda9e66e7`. Library behavior is
unchanged from `5bc2613`; official and diagnostic binary hashes are identical
to their respective binaries in the previous failed run 40396370860.

| Binary | Engine | Actual seconds | Capture Deadline | Query Deadline | Original gate |
| --- | --- | ---: | ---: | ---: | --- |
| Official | sync | 300.024702 | 0 | 0 | PASS |
| Official | async | 300.025914 | 0 | 0 | PASS |
| Diagnostic | sync | 300.019553 | 4 | 0 | FAIL |
| Diagnostic | async | 300.020889 | 2 | 0 | FAIL |

Business errors were zero throughout. Official sync recorded one publication
Busy and nine query Busy; official async recorded one of each, within the
unchanged original gates. All four reports were downloaded, checked against
the build manifest, and rechecked with `scripts/check-load.py` for the matching
engine and 300 seconds. Gate logs are retained. The workflow remains failed
because both diagnostic jobs failed; no failed evidence was overwritten.

## Confirmed environment defect and correction

The before/after probes confirm that the hosted measurement process inherited
a Utility clamp: requested default QoS on a new pthread could not exceed
base/max priority 20. Outside niceness was -10, so this was not simply a
positive nice value. The owned launchd job produced nice 0, default QoS 21,
base 31 and max 63. Actual instrumented workload threads also retained base
31/max 63. All four owned jobs were removed successfully and verified absent.
The full raw records are in `policy-review.json` and each artifact directory.

This corrects a measured scheduling-policy inheritance problem, not a library
priority override. Business service, load generator, and observation all run
with the same ordinary application policy. No real-time/user-interactive
thread QoS, CPU-time deadline, reduced traffic, or relaxed error gate was used.

## Remaining limits and next acceptance

The six diagnostic capture failures still contain substantial off-CPU time.
All fifteen retained slow intervals have zero expanded fault/pagein/COW
deltas; all 31 in-interval thread samples are runnable with base 31/max 63.
Overlapping five-second host CPU windows range from about 279% to 300% of one
core on three-core VMs. This does not establish a specific continuous
scheduler history or prove allocator locking. The diagnostic binary adds
per-allocation clocks and checkpoint overhead, and its VM differs from the
official VM; it is not valid to attribute every difference to probe overhead.

The official short candidate now passes with the confirmed policy correction.
That justifies testing the correction against the **original** full endurance
requirements. It does not yet establish that all tail timeouts are resolved.
The two Linux and two macOS 26 ARM task cohorts and four baseline cohorts must
pass before release; diagnostic failures remain visible but do not create a
new instrumented-binary release gate. If original-binary endurance fails,
release stays blocked and this candidate must not be called a complete fix.
