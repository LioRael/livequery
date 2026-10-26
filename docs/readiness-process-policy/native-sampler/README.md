# Native sampler mechanism check

The remaining official 600-second failures cannot be precisely correlated retrospectively: their bounded capture records have no absolute timestamp. Existing diagnostic spans already have CLOCK_MONOTONIC timestamps and pthread identities; retain them and replace only the external Python sampler with a native buffered reader.

This reader samples registered threads every 10 ms and records target process CPU/fault/context-switch/syscall counters, sample start/end, and its own thread CPU. It does not write output during sampling. It retains actual gaps instead of treating them as 10 ms. It refreshes bounded identities once per second, fails explicitly on identity overflow, and stops after at most 600 seconds or target exit. It does not change target scheduling. Thread states are snapshots, not continuous scheduling events; process counters cannot attribute positive events to a particular thread.

The owned one-second computing helper was stopped for about 120 ms. Nine interior samples were waiting with no thread CPU growth; 71 outside samples were runnable. The final native sampler consumed 1.082 ms CPU over this calibration. This is not loaded-overhead certification or evidence of the natural failure cause. The earlier sleeping-helper check could not distinguish sleep from injected stop and is not used as causal validation.

One existing diagnostic workflow run will use two engines at the failed 1000-row/128-byte/600-second workload, with original business/observer counts and deadlines. This is diagnosis, not a new release gate or replacement for the pending original acceptance. No identical official short test is repeated.
