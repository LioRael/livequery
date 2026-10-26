# macOS 26: current CPU counters confirm residual activity

Run 40389493414, workflow source f3cd8d6, diagnostic base 12fa024 plus unchanged patch. macOS 26.6.2 / ARM64 / three logical CPUs / image 20261013.0424.1. Indexing isolation completed, followed by build and the fixed 180-second settling interval. No workload started.

HOST_CPU_LOAD_INFO deltas over 5.168, 5.164 and 5.108 seconds measured 157.568%, 49.838% and 52.157% of one logical CPU busy. All three exceeded the diagnostic admission budget of 25%. This measures actual interval activity, unlike load-average history or decaying per-process ps estimates. It includes monitoring overhead. The idle fraction was about 47.5%, 83.4%, 82.6%; it does not establish which individual background process used each tick, whether a future capture would time out, or whether 25% is a necessary performance condition.

Nearby process snapshots included softwareupdated, WebKit.WebContent, mds and other system agents. Per-process snapshots are only attribution hints, not CPU-tick allocation or proof that each process caused a historical Deadline. Do not disable security/system services opportunistically.

## Combined conclusion

- Original macOS 26 diagnostic 40383573185 ran the workload and recorded five capture Deadlines per engine. Phase/CPU/allocator/fault/state evidence supports insufficient CPU service as a mechanism, with sparse-sampling limitations.
- macOS 15 comparison 40388527325 never ran the workload. Its rejection was based on a flawed historical-load admission heuristic; it provides no evidence that macOS 15 fixes or reproduces LiveQuery timeouts.
- Corrected macOS 26 admission now records real residual CPU activity even after indexing interventions. The isolation correction is incomplete. The newly introduced 25% admission threshold is not an original release criterion and is not proof of a LiveQuery failure.
- No new uninstrumented success, no full acceptance and no release are justified. Production library/workload/error/resource gates remain unchanged. Do not launch further identical hosted runs hoping to find a quiet VM or silently switch final coverage to macOS 15.

The remaining task is a justified execution-isolation strategy with measured CPU availability during the actual workload, not further changes to the timeout or a claim that background activity alone proves every historical failure. Larger capacity would itself be an environment change and would still require original OS coverage and gates.
