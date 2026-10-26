# Driver implementation audit

This audit did not identify or fix the remaining CI Deadline failures. It does
not supersede the failed 5bc2613 acceptance candidate or qualify a release.

## Evidence and rejected hypotheses

- The four TCP clients run closed-loop without a requested arrival rate: each
  immediately sends another 64-byte request after receiving the previous reply.
  Optimizing the driver can increase offered traffic instead of leaving more CPU
  available to inspection. No pacing, concurrency, request deadline, observer
  count, or acceptance condition was changed.
- Observer error paths reach the `finally` block and its poll delay, including
  the `continue` branches for reported errors. There is no unthrottled error
  retry at this call site.
- The publisher releases the application mutex before starting capture timing.
  The inspection server advances bounded clients on its existing thread; it
  does not spawn a thread for every observer connection.
- Three resource reads on this host took 28.50–32.01ms elapsed and
  23.37–25.58ms child CPU each. At one read per five seconds this measured cost
  alone does not explain sustained CPU saturation. These local measurements
  do not establish the cost on the hosted runner.
- A ten-second cProfile run with local Python 3.9.6 exposed approximately
  134,000 task creations, largely associated with per-request `wait_for`.
  **This is not the CI interpreter.** The retained official sync and async host
  activity identifies Homebrew Python **3.15.0**. This invalidates attributing
  the CI failures to the Python 3.9 task-per-read implementation.
- A second, bounded ten-second profile used the already installed Python
  3.15.1 and the same workload binary and parameters. Its positive-timeout
  `wait_for` awaits directly inside a timeout context. Socket send/receive and
  kqueue dominate individual driver costs. Removing a nonexistent task-per-read
  cost would not address this runner's failure mechanism.

Both profiles used the existing benchmark driver with sync, 5000 rows,
96-byte strings, four observers, and four business clients. Profiling changes
timing; the binary's source was not independently verified for this audit.
These reports are driver investigation evidence, not library performance or
acceptance evidence. Absolute profiler costs and throughput across interpreter
versions are not a controlled comparison. Raw report binary hashes and bounded
profile tables are retained alongside this note.

The existing CI evidence still supports substantial off-CPU intervals, but
does not identify a repairable application lock or a specific scheduler cause.
Reducing the workload, changing priority, accepting Deadline errors, or
relabeling a passing retry would not establish a fix. No further CI load was
dispatched as a result of these rejected hypotheses.
