# Native-sampler reproduction, run 40409161324

Both free macOS 26 ARM jobs ran the failed 1000-row/128-byte workload for 600 seconds, with four business clients and four observers. This diagnostic run is not release acceptance. Library behavior and the diagnostic binary remain unchanged (SHA256 cb456f330076117553a21e365348291d20c59487eb26f03cb060872066aca98f); only the external sampler changed. Reports are clean source 38fc8cf. Build manifest, measurement verification, report hash, policy and cleanup records were checked.

Sync failed with one publication Deadline and nine query Busy; async passed the original check-load with zero Deadline and eight query Busy. Both had zero business errors. Original gates were rechecked locally and their outputs retained. No instrumented passing result replaces the failed official acceptance.

## Shared delay at the reproduced failure

The sync capture lasted 196.857 ms with 2.732 ms thread CPU. Its pre-Deadline rows interval lasted 185.550 ms with 0.360 ms thread CPU. Allocator wall time within that interval was 185.316 ms; it includes preemption and does not establish allocator lock contention.

The external native sampler last completed a 9-microsecond read before the interval. Its next sample started 208.451 ms later despite a requested 10 ms sleep; the observer's thread CPU advanced only 17,708 ns. There are no samples inside the failed interval. The sampler runs in a different process and does not acquire LiveQuery application or allocator locks. Therefore a capture-local allocator wait alone does not explain both observed delays. This is evidence of a shared scheduling/wakeup delay, but cannot distinguish delayed timer delivery, OS-wide contention, a common process-policy effect, or a hypervisor pause. It is not continuous scheduling evidence.

The expanded process bracket has zero faults/pageins/COW. The overlapping five-second host CPU window is 292.72 percent of one core on a three-core VM. Prior ps activity included WebKit and mds; ps is decayed and the window is too wide to attribute this 185 ms pause to any named process. No further system service was disabled.

The native observer consumed 1.904 CPU seconds in the sync run and 1.831 seconds in async (about 0.32/0.31 percent of one core). Its maximum sample gaps were 217.245/93.487 ms. Reducing observer CPU did not eliminate the sync failure. There were no dropped diagnostic events.

## Boundary

The earlier two official failures lack absolute capture timestamps, so this reproduction cannot retroactively prove their precise cause. No safe library change follows from these measurements: shortening computation cannot eliminate a greater-than-100-ms period without CPU service. The original acceptance remains failed; no merge or publication is authorized by a diagnostic green result. No identical rerun or threshold change was performed.

Raw thread samples are losslessly gzipped; other reports and provenance are preserved. shared-delay.json contains the exact bracketing frames. analyze.py runs on an uncompressed downloaded engine directory and writes native-correlation.json.
