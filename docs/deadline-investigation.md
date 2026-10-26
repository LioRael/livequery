# Stable 0.1.0 deadline investigation — release blocked

The production library remains at behavior revision 38dffc0, official source 12fa024. Stable 0.1.0 has not passed acceptance. No merge, version bump or publication is justified by the diagnostics below. All earlier failed reports are retained.

## Correlated local finding

[Failed-interval evidence](readiness-task-local-12fa024/offcpu-diagnostic/failure-correlated/README.md) links 60 capture and 2 query Deadlines to phase CPU/wall time, allocator timing and kernel thread/process counters in the same run. Of 144 selected pre-Deadline intervals, 111 have zero process faults in an expanded bracket and all sampled thread states running/runnable. There are 54 consecutive sample pairs with unchanged thread CPU, up to 28.894 ms apart. A query scan gap of 39.352 ms used 0.042083 ms CPU, with no allocator calls and no process faults. This is strong evidence of insufficient CPU scheduling service as the main local mechanism, rather than application lock waits or excessive CPU work. The sampling limitations and diagnostic overhead remain material; it is not a continuous scheduler trace or proof about every historical failure.

The local endurance comparison was not isolated: exported trace metadata confirms another task's benchmark/CPU Counters sessions during the async run. This identifies a confounder, not which process caused an individual timeout. Original CI reports only distinguish wall time from thread CPU and do not yet provide equivalent correlated state/fault evidence.

## CI action

The macOS deadline diagnosis workflow uses the existing macos-latest class and original gates. It records a separate instrumented archive, runs sync and async serially after compilation, rejects known competing build/load/profiler processes in the same runner, and takes a cooperative measurement lock. It preserves both successes and failures, source/patch/binary identity, host activity, thread snapshots and expanded fault-counter brackets. It does not rerun two-hour acceptance or silently replace the accepted environment. Different GitHub hosted jobs already receive separate VM instances; serializing unrelated workflows is not evidence of physical-host isolation.

Next decision is based on failed CI intervals: determine whether they show the same runnable-with-little-CPU mechanism, or a different code/kernel-wait path. Do not infer success from a short run without a Deadline. Do not replace wall-clock limits with CPU time, increase the timeout, retry away failed operations, or globally raise library thread priority. Once the cause and correction are established, rerun the unchanged full baseline/task acceptance and final release checks.

## CI result 40383573185

[Raw correlated CI evidence](readiness-ci-diagnostic-40383573185/README.md) reproduces five capture Deadlines per engine despite serial execution and no known build/profiler overlap. CPU/wall, zero-fault brackets and running/runnable samples support the same CPU-service-shortage mechanism, with weaker sampling coverage than the local case. Host snapshots show substantial OS background activity on the 3-vCPU VM. Known-process exclusion alone did not establish a quiet host. A fixed, fail-closed host admission check now rejects this condition before measurement; it is not evidence that the problem has been solved and does not replace any original gate. No system service is killed or disabled.

## Isolation follow-up and admission correction

Disk-indexing shutdown alone left user indexing agents active; removing those agents succeeded, but macOS 26 still showed other substantial background activity before load. The [macOS 15 comparison](readiness-ci-isolation-40388527325/README.md) never started a workload: its old admission failed only on load-average history. It cannot establish that switching OS fixes any Deadline. The new admission heuristic incorrectly conflated historical run-queue load with current CPU competition; it now measures interval CPU ticks with the same 25%-of-one-core budget. Original error, workload and resource gates remain untouched. Verification returns to macOS 26; no OS coverage is dropped.

## Actual-CPU admission result

[Run 40389493414](readiness-ci-isolation-40389493414/README.md) returned to macOS 26 and recorded actual pre-workload CPU use of 1.576, 0.498 and 0.522 core equivalents in three five-second intervals. It did not start the workload. Indexing interventions have not established the requested isolation. The 25% diagnostic admission budget is an added conservative prerequisite, not evidence of a workload timeout and not an original acceptance requirement. macOS 15 has no workload result; it cannot be used to bypass macOS 26 failures. Stop repeated identical hosted runs until an execution-isolation strategy is justified; keep every failed report and the original acceptance gates.

## User-directed build/measurement separation

The diagnostic 25% admission requirement is removed. Build and measurement now run on separate hosted VMs, with verified official and instrumented binaries and separate environment provenance. Four macOS 26 jobs run the unchanged actual workload and original gates once each (official/diagnostic × sync/async). Runtime CPU windows are evidence only and are linked to diagnostic failure intervals. This addresses the explicit request to measure actual behavior rather than block on a new host criterion. Isolation effectiveness and timeout resolution remain to be established by completed results.


## Paid runner withdrawal and implementation audit

The paid runner selector was withdrawn at the user's explicit request. No paid resources were provisioned or dispatched. Continue on standard macOS 26 ARM and keep the original acceptance requirements.

The current implementation audit establishes these boundaries:

- Workload `record` takes the application counter mutex, but snapshot capture runs after cloning and releasing that mutex. The reported capture deadline cannot include waiting to acquire this application mutex. Copy time is separately recorded.
- Registered published queries use `Registry::read_snapshot` and an immutable `Arc` snapshot. They do not use the deep-copy `Source::snapshot` fallback. Replacing that fallback would not repair this measured query path.
- Capture validates constructed snapshots, and publication validates public, mutable snapshot input again. Eliminating publication validation based on an assumption that every snapshot came from capture would permit mutated or manually constructed snapshots to bypass validation; it is not a safe timeout fix.
- Query deadlines use wall time from execution entry; per-row and final checks remain intact. Sorting currently processes all matching rows before applying LIMIT, an identifiable optimization opportunity, but it does not explain capture deadlines or establish that the observed failures were CPU-bound sorting. No speculative query change was made.
- The business driver has four clients and observers back off after errors. There is no observed error-triggered tight retry loop to remove. Driver throttling would change the required workload and is not a repair.

This audit has not identified a code defect explaining the observed off-CPU capture intervals. No fix, successful acceptance, or permission to relax the original gates is claimed. Historical evidence remains intact; do not resume the withdrawn paid-resource route or dispatch identical trials merely to obtain a pass.
# Confirmed hosted scheduling-policy inheritance

The current candidate corrects an inherited Utility QoS clamp in hosted
measurement processes, with direct before/after evidence rather than an
inference from CPU utilization. See the [policy investigation and complete
candidate results](readiness-process-policy/ci-40400641786/README.md).
Official sync and async each passed 300 seconds on unchanged binaries after
the correction. Instrumented counterparts still failed; this is not proof
that all tail timeouts are resolved. Proceed to the original full acceptance
on the official binaries, retaining every earlier failure and the exact
measurement policy. No library priority change, paid runner, or new gate is
part of this correction.
