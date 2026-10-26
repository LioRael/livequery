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
