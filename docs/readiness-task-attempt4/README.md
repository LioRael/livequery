# Task endurance attempt 4 — diagnostic run, failed acceptance

Run https://github.com/LioRael/livequery/actions/runs/40314135645 tested clean source `12fa024461de4096b325ed3c9b8c87da735ba1e7`. Library behavior is unchanged from `38dffc0`; workload instrumentation adds capture thread CPU and wall time.

Linux sync/async passed the original 600-second and 7200-second gates. macOS async failed its 600-second stage with one capture Deadline and did not run the 5000-row stage. macOS sync completed 7200 seconds and failed with two capture Deadline errors. Business errors were zero. See raw reports, toolchains, and summary.json.

The async failing capture used 294.137 ms wall time and 0.409 ms thread CPU. The two sync failures used 111.136/19.941 ms and 104.849/3.697 ms wall/CPU. These establish substantial off-CPU time, not its cause. They do not establish a library fix or an exclusive hosted-runner problem; the subsequent local diagnostic also failed (../readiness-task-local-12fa024).

Matching baseline run https://github.com/LioRael/livequery/actions/runs/40314141298 passed all four original gates. Baseline success does not override these task-table failures. Stable acceptance remains failed.
