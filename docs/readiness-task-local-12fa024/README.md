# Local macOS diagnostic comparison — failed acceptance

Source: `12fa024461de4096b325ed3c9b8c87da735ba1e7`, clean at measurement. Both engines used the same binary; see `summary.json` and `toolchain.txt`. The driver and all baseline/cost/task JSON reports are retained here. This synthetic local comparison is not real-project validation and does not replace failed hosted CI evidence.

Both 5000-row trials ran 7200 seconds. Sync passes the unchanged checker. Async fails: 2 query Deadline errors and 63 capture Deadline errors, plus 30 query Busy and 5 publication Busy. Business requests had zero errors in both trials. `gate-results.txt` records revalidation with the original checker. The async checker stops at query errors, so its failure does not establish that later resource checks passed.

All 63 failing captures have CPU measurements: wall time 100.609–344.403 ms, thread CPU 0.792–19.522 ms (respective medians 118.688 and 11.980 ms). Each failure spent most elapsed time outside thread CPU execution. This does not distinguish scheduling, blocking, or other off-CPU causes. Capture measurements do not diagnose the two query failures. No scheduling attribution or successful fix is established.

The local suite ran sync then async; these were not simultaneous or randomized controlled trials. Other host activity was not comprehensively recorded. Therefore differences between engines are not causal proof. The local failure rules out treating these symptoms as exclusive to hosted CI.

The progress log reported business and query errors only; publication errors became available in the final report. The driver exited on the failed gate and did not produce a PASS completion marker. No unchanged two-hour rerun is authorized as a way to obtain green results. Next work should establish a shorter reproducible diagnostic scenario and identify where off-CPU time occurs before another behavior change or full acceptance run. All 100 ms and error gates remain unchanged; stable release remains pending.

## Short reproduction attempt

A single unchanged 60-second async run (5000 rows, 96-byte strings, four observers) completed in 60.006 seconds with zero business, query, and publication errors and no slow capture samples. See `async-short-diagnostic.json`. This did not reproduce the failure and is not acceptance or evidence of a fix. A reliable short reproduction remains missing.

A second 60-second diagnostic added a 20-second native `sample` capture at 1 ms intervals to inspect thread stacks. It also had zero errors and no slow capture samples. `async-profile-diagnostic.json` and `async-profile-sample.txt` preserve the result. Sampling changes execution and did not overlap a reproduced failure, so these stacks cannot identify the earlier off-CPU cause. Do not use this diagnostic as acceptance or run repeated uninstrumented tests until green.

## Independent sampling/resource audit

Because the original async checker stops at its error assertion, sampling and resources were also inspected separately without changing the checker or its result. See `sampling-resource-audit.json`: sync/async have 1430/1427 warmed samples, maximum gaps 5.061/5.363 seconds, continuous business/observation traffic and all three query kinds in every interval. Thread ranges are zero, FD ranges four, RSS ranges 4,243,456/7,847,936 bytes (both below 8 MiB). These checks do not override the Deadline failures.
