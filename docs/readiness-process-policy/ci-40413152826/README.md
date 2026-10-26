# Existing system logs during shared stalls

Run [40413152826](https://github.com/LioRael/livequery/actions/runs/40413152826), source `7f18a69ea0adac309138889ed678e174ca963f95`, completed **FAIL**. This is an instrumented diagnosis, not release acceptance. Both clean reports and manifests identify diagnostic binary `cb456f330076117553a21e365348291d20c59487eb26f03cb060872066aca98f`, unchanged from the preceding experiment. Both owned measurement jobs bootstrapped and cleaned up successfully, with absence confirmed.

The only experiment change was reading already-existing unified logs **after** workload and sampler exit. No new runtime recorder, policy adjustment, library change, traffic reduction, or gate change was made. Collection succeeded without truncation or omitted failure windows. Native samples are preserved losslessly as gzip; other artifacts retain their original contents, including independently rerun gate failures.

| Engine | Actual seconds | Business requests | Business errors | Capture errors | Query errors |
| --- | ---: | ---: | ---: | --- | --- |
| sync | 600.0042235 | 14,397,405 | 0 | Deadline 1, Busy 1 | Busy 9 |
| async | 600.026776375 | 22,674,238 | 0 | none | Deadline 1, Busy 4 |

Both original gates fail. Their later resource assertions are not certified by those executions.

## What the failure windows establish

Sync capture attempt 9046 took 178.921 ms wall / 2.553 ms CPU at the external boundary. Its instrumented pre-return span was 110.673 ms wall / 1.659 ms CPU. Two row intervals at 22:40:18.214504–.267235 and .268266–.325068 UTC took 52.731/56.802 ms wall with only 0.359/0.208 ms CPU and 0.115/0.067 ms allocator time. Expanded fault/pagein/COW deltas were zero. A separate sampler gap covering the first interval was 59.172 ms with 0.025209 ms observer CPU. Kernel records at .246494 and .297100 show `ps` processes unnesting DYLD shared regions **during** these intervals. Their presence does not establish that unnesting caused the delays.

Async query took 145.593 ms wall / 0.126 ms CPU. Its scan intervals were 22:39:01.067781–.115239 and .115262–.213337 UTC. The separate sampler gap was 146.348 ms / 0.035666 ms CPU. Expanded fault/pagein/COW deltas were zero. Kernel IOSurface paravirtual mapping records occur at .122199, .132213 and .172186 inside the second interval. Therefore the entire interval cannot be characterized as one continuous complete guest-VM freeze. Shorter interruptions remain possible.

The records also contain desktop power assertions, beta-enrollment activity and periodic thermalmonitord SMC connection failures. None is an explicit scheduling/throttling event for the measured workload or sampler. Co-occurrence alone does not establish causation; no service was disabled based on it. Guest records cannot identify physical-host scheduling decisions.

## Unresolved boundary

The experiment demonstrates that kernel activity continued while the two measured processes received very little CPU. It does **not** identify a specific scheduler policy, competing process, kernel wait, or host action as the cause. There is no demonstrated library correction to apply from these records, and no justification for another unchanged rerun. The earlier official task failures remain release-blocking. No merge, publication, relaxation, paid resource, or extra acceptance condition was introduced.

Collector validation covered retained-failure UTC mapping, merging overlapping windows, excluding success spans, 2 MiB output truncation, and termination of a silent child at the 20-second limit. `log show` rejected fractional-second date syntax in the first local parser check; the committed collector rounds outward to whole seconds and the no-match 1970-window syntax check passed. No unrelated local application logs were collected.
