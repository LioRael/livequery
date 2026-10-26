# Candidate 5bc2613 CI result: not accepted

Run 40396370860 completed all four 300-second measurements on macOS 26 ARM. Every original gate failed on Deadline errors and reproduced on local raw-report recheck. Build base and workflow head are both 5bc2613b43d841e17e9c37a94f98264d56d81f55. Official binary SHA256 dc7aa356b8e450de5b2cdc99e51c039b6bb9c14ee628c70dfcae465599172ccf; diagnostic cb456f330076117553a21e365348291d20c59487eb26f03cb060872066aca98f. Both report/manifest hashes match. No paid runner, relaxed limits or retries were used.

| Variant | Engine | Actual seconds | Capture Deadline | Query Deadline | Business errors |
|---|---|---:|---:|---:|---:|
| Official | sync | 300.007458 | 4 | 2 | 0 |
| Official | async | 300.026569 | 3 | 1 | 0 |
| Diagnostic | sync | 300.083433 | 2 | 1 | 0 |
| Diagnostic | async | 300.040334 | 5 | 0 | 0 |

The allocation reduction is real in the local benchmark, but it did not eliminate the CI timeouts. Differences in failure counts from earlier runs are not a controlled comparison. Do not label this optimization a root-cause fix or start final endurance/release on the basis of ordinary CI passing.

Diagnostic sync retained three failed spans; three slow pre-return intervals had three total state-1 samples, and two expanded fault/pagein/COW brackets were zero. One query scan interval was 122.093ms wall versus 0.026583ms CPU and 0.00075ms allocator wall, with zero fault bracket and an overlapping five-second CPU window at 297.91% of one core. That interval had no thread samples, so it cannot establish continuous runnable status.

Diagnostic async retained five failed spans, seven slow pre-return intervals, three total state-1 samples and seven zero fault/pagein/COW brackets. One rows interval was 112.833ms wall / 0.5065ms CPU; 108.07147ms elapsed inside allocator boundaries. This does not by itself identify allocator locking: descheduling during allocation also contributes allocator wall time. The overlapping CPU window was 295.33%. Raw event, thread, counter and host-window associations are retained.

Ordinary CI run40396340250 passed Linux/macOS/MSRV; Stability run40396340658 passed API/dependencies/fuzz, with sustained-load correctly skipped by the explicit trigger policy. These successes do not override the measured failures. No subsequent load was dispatched. Original historical failures remain retained.
