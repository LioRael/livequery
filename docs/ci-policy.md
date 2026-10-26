# CI execution and stable-release evidence

Pull requests run CI (Linux/macOS correctness, smoke, package and MSRV) and Stability gates (dependency audit, API compatibility and fuzz). Branch pushes do not duplicate these checks. Pushes to main still run both workflows; manual dispatch remains available.

Sustained-load jobs run on explicit Stability dispatch or the existing weekly schedule. Both two-hour workflows and the macOS diagnostic workflow are explicit-dispatch only. Editing a workflow no longer starts a two-hour experiment automatically. Diagnostic results do not count as stable-release acceptance.

These trigger changes do not remove release obligations. Before 0.1.0, retain the original Linux/macOS × sync/async baseline and Linux sync/async task workloads, cost comparisons, durations, traffic/sampling/resource checks, 100ms wall deadlines and error gates. Record the tested commit and binary identities. A skipped routine sustained-load job is not acceptance evidence. Dispatch the full release suite on the final candidate, inspect and retain raw artifacts, and merge/publish only after every acceptance condition in the explicitly revised scope below is satisfied.

## Resource investigation (40391039817)

See readiness-ci-split-40391039817/cpu-consumer-audit.json for the audit of existing measurements; no additional load was generated for this analysis. The official workload process used 243.63/256.32 CPU seconds over approximately 300 seconds (sync/async), about 81.20%/85.42% of one core. This is cumulative wait4 accounting, including startup/finalization, not an instantaneous failure measurement. Python process snapshots averaged 73.12%/70.55%. Three-core host counters averaged 231.87%/236.49%, with five/six five-second windows above 290%.

The evidence does not support claiming that the application alone continuously consumes all three CPUs. Nor can a subtraction of decayed, threshold-filtered ps samples from interval CPU counters attribute the difference to a particular OS process. Snapshot profiles identify several transient system consumers rather than one persistent removable offender. The previous build separation and indexing changes were insufficient; no library fix is claimed.

GitHub documents standard macos-26 ARM runners as three CPUs/7GB and macos-26-intel as four CPUs/14GB: https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job . Switching to Intel would change architecture as well as resources and would not resolve or replace the existing ARM acceptance requirement. A repository hosted-runners API request returned 404; that is not evidence that an authorized larger ARM runner exists or does not exist. Do not silently enable paid capacity or substitute architecture.

The remaining resource question is short-interval CPU availability to the failed thread, not mean process utilization. Existing correlations support CPU service shortage but do not establish a safe configuration that removes it. Preserve this distinction when selecting a targeted change; no identical-configuration retry or new admission threshold is justified by this audit.

## Resource scope

The user declined paid runners. The diagnostic workflow is fixed to the existing standard `macos-26` ARM runner. The temporary xlarge selector has been removed; no paid job was dispatched, no validation repository created, and no repository moved. Further investigation stays within the existing implementation, workload and free CI; adding capacity is not a prerequisite for release. The later explicit task-table scope decision below supersedes the macOS task-table release obligation.
# Measurement scheduling correction

Run [40400641786](readiness-process-policy/ci-40400641786/README.md) confirmed
that the hosted runner's child processes inherited a Utility QoS clamp. An
owned temporary launchd job restores ordinary default application scheduling
for the entire measurement, including business traffic. The library never
sets thread priority. Exact before/after policy and cleanup evidence are
retained. Both official 300-second candidates passed the original gates;
both instrumented candidates still failed and remain recorded.

Baseline and task endurance now build once per OS on separate VMs, hash the
executables, then run the original OS/engine matrix on measurement VMs without
compiling. macOS coverage is explicitly pinned to the free macOS 26 ARM runner.
The original durations, repetitions, workload parameters and check-load rules
are unchanged. Linux runs directly; macOS uses the owned measurement launcher.
The existing smoke and sustained-load jobs use the same macOS launch policy.
This adds no CPU admission gate and does not make diagnostic instrumentation
an additional release gate. Long official-binary acceptance is still required.

## 0.1.0 task-table acceptance scope (2026-10-27)

The release owner explicitly limited the task-table endurance release gate to Linux sync/async. macOS remains covered by regular correctness/smoke CI and the four-cohort Linux/macOS two-hour baseline. All original Linux task durations, repetitions, traffic, 100ms wall deadlines, error and resource gates remain required. This is a disclosed acceptance-scope change, not a timeout fix or a passing macOS task result.

macOS task-table high-load acceptance has **not passed**. The original 600-second, 1,000-row/128-byte trials at 29b2f17 each recorded a publication Deadline despite ordinary default scheduling; their 5,000-row stage did not run. Historical failures and diagnostics remain in [attempt 5](readiness-task-attempt5/README.md) and [the last correlated investigation](readiness-process-policy/ci-40413152826/README.md). The remaining cross-process delay has no established specific root cause or verified fix. Applications using macOS task-table workloads must handle Deadline/Busy and retain the prior successful snapshot; no high-load success or latency guarantee is claimed for this release.
