# Task endurance attempt 2 — failed acceptance

Run: https://github.com/LioRael/livequery/actions/runs/40278386255
Behavior source: `176710777440897b581437123a8056bff9dfc04d`.

All four 5,000-row trials completed 7,200 seconds with zero business errors. Linux sync/async passed the unchanged gate. macOS sync had 26 publication Deadline errors in 124,384 attempts; async had 29 Deadline and two Busy errors in 122,131 attempts. This run does not satisfy release acceptance.

Raw reports are retained beside summary.json. Original artifacts, including cost comparisons and toolchains, are available on the run. The visitor optimization did not eliminate macOS capture deadlines. Published::publish does not emit Deadline; the workload calls Snapshot::capture first, whose loop and final elapsed-time checks can emit it. Current timing measures capture plus publication, so these reports cannot distinguish row inspection, validation and scheduling delay. Further phase measurements are required before attributing a root cause. Do not rerun unchanged code merely to obtain a green result.

The separate baseline run 40278381395 passed all four jobs; that does not override the task-table failure. PR CI run 40278378962 attempt 1 also retained a single macOS sync capture deadline; diagnostic attempt 2 passed without source changes.

## Follow-up measurement and candidate

A local 500-iteration isolated probe (5,000 rows, 96-byte detail; instrumented Inspect duration) measured mean capture 7.291 ms, field inspection 3.031 ms and publication 2.550 ms. A 60-second local sync load with four observers had zero errors; this did not reproduce the CI failure. A 10-second macOS sample during that load found 433 of 1,773 capture stack samples in the row JSON byte-buffer pass, 768 in inspection and 521 in final validation. Sampling is diagnostic and is not an unbiased CPU-time or scheduling attribution.

The follow-up candidate counts row JSON bytes through a capped writer instead of retaining a temporary Vec. It keeps serde_json serialization, identical byte budgets and final snapshot validation. A corresponding 500-iteration probe measured 6.571 ms mean capture (approximately 10% lower), 3.121 ms inspection and 2.613 ms publication. These are exploratory local measurements, not a controlled CI comparison or proof that deadlines are eliminated. Raw probe samples and source are retained here; the baseline library source is 1767107, and the counting candidate is the code change accompanying this document. No byte or deadline limits were relaxed.

Native workspace check/test/clippy, rustdoc and serde arbitrary_precision/raw_value regressions passed for the candidate. Full baseline and task-table endurance must run again on its committed source before acceptance.
