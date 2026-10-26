# Measurement process scheduling policy

The 5bc2613 CI thread samples contain a previously unexamined signal: idle
inspection threads and the main thread repeatedly report current priority 20.
The corresponding local samples usually report 31. Current priority alone
does not identify a QoS clamp, niceness, or the effective task policy.

Apple's [scheduler constants](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/sched.h)
and [QoS policy mapping](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/kern/thread_policy.c)
map Utility to base priority 20 and default QoS to 31. This is a concrete
environment hypothesis, not proof that all prior Deadline errors share a cause.
In particular, local failures also occurred with priority 31.

## Controlled mechanism check

`local-launch-check.json` records a no-workload check. A process launched under
`taskpolicy -c utility` requested default QoS for a new pthread, but both its
base and maximum priority remained 20. Starting an owned temporary launchd job
with `ProcessType=Interactive` from that same parent produced default QoS 21,
base priority 31, maximum priority 63, and nice 0. The job was removed and its
absence verified. No other process or persistent service was modified.

The `Interactive` launchd process type here selects ordinary application
resource policies; the measured thread QoS is **default**, not user-interactive
or real-time. This distinction is checked by the probe rather than inferred
from the launchd label. Merely using `taskpolicy -a` beneath a Utility-clamped
parent did not clear the inherited clamp in the controlled check.

## Targeted hosted verification

The existing free macOS 26 ARM diagnostic workflow now launches only its
measurement process through an owned temporary launchd job. It saves the
outside and inside process policies, while existing workload thread sampling
also retains base and maximum priority (no additional sampling calls).
The library does not set thread priorities. The original binary hashes,
5000 rows, 96-byte strings, four clients, four observers, 300-second candidate
duration, and 100ms/error/resource gates remain unchanged. Both the business
driver and service receive the same process policy, rather than favoring
inspection over business traffic.

Prediction: if inherited Utility clamping contributes to the hosted failures,
the outside probe will show a cap of 20, the owned job will restore default
policy, and actual inspection threads will no longer have that cap. Only the
original workload result can determine whether this resolves the candidate's
timeouts. A policy change alone or a short passing candidate is not final
endurance acceptance. If the outside probe was not clamped, that attribution
must be rejected even if the candidate passes.

The launcher refuses local and self-hosted use, copies only an allowlisted
non-secret environment, records the real child exit status, removes only its
UUID-named job, and records cleanup. No CPU admission gate or paid resource
was added. The four final baseline/task endurance cohorts remain required.
