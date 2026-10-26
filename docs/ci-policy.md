# CI execution and stable-release evidence

Pull requests run CI (Linux/macOS correctness, smoke, package and MSRV) and Stability gates (dependency audit, API compatibility and fuzz). Branch pushes do not duplicate these checks. Pushes to main still run both workflows; manual dispatch remains available.

Sustained-load jobs run on explicit Stability dispatch or the existing weekly schedule. Both two-hour workflows and the macOS diagnostic workflow are explicit-dispatch only. Editing a workflow no longer starts a two-hour experiment automatically. Diagnostic results do not count as stable-release acceptance.

These trigger changes do not remove release obligations. Before 0.1.0, retain the original Linux/macOS × sync/async baseline and task workloads, cost comparisons, durations, traffic/sampling/resource checks, 100ms wall deadlines and error gates. Record the tested commit and binary identities. A skipped routine sustained-load job is not acceptance evidence. Dispatch the full release suite on the final candidate, inspect and retain raw artifacts, and merge/publish only after every original acceptance condition is satisfied.

## Resource investigation (40391039817)

See readiness-ci-split-40391039817/cpu-consumer-audit.json for the audit of existing measurements; no additional load was generated for this analysis. The official workload process used 243.63/256.32 CPU seconds over approximately 300 seconds (sync/async), about 81.20%/85.42% of one core. This is cumulative wait4 accounting, including startup/finalization, not an instantaneous failure measurement. Python process snapshots averaged 73.12%/70.55%. Three-core host counters averaged 231.87%/236.49%, with five/six five-second windows above 290%.

The evidence does not support claiming that the application alone continuously consumes all three CPUs. Nor can a subtraction of decayed, threshold-filtered ps samples from interval CPU counters attribute the difference to a particular OS process. Snapshot profiles identify several transient system consumers rather than one persistent removable offender. The previous build separation and indexing changes were insufficient; no library fix is claimed.

GitHub documents standard macos-26 ARM runners as three CPUs/7GB and macos-26-intel as four CPUs/14GB: https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job . Switching to Intel would change architecture as well as resources and would not resolve or replace the existing ARM acceptance requirement. A repository hosted-runners API request returned 404; that is not evidence that an authorized larger ARM runner exists or does not exist. Do not silently enable paid capacity or substitute architecture.

The remaining resource question is short-interval CPU availability to the failed thread, not mean process utilization. Existing correlations support CPU service shortage but do not establish a safe configuration that removes it. Preserve this distinction when selecting a targeted change; no identical-configuration retry or new admission threshold is justified by this audit.

## Resource scope

The user declined paid runners. The diagnostic workflow is fixed to the existing standard `macos-26` ARM runner. The temporary xlarge selector has been removed; no paid job was dispatched, no validation repository created, and no repository moved. Further investigation stays within the existing implementation, workload and free CI; adding capacity is not a prerequisite for release. Original acceptance requirements remain unchanged.
