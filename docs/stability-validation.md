# Alpha.3 hardening validation

Development validation on 2026-10-26, macOS arm64 / Apple M2 Pro. This is an unpublished candidate, not a stable release. Real-project integration is excluded from the acceptance scope at the user's request.

- Standard Cargo check, fmt, strict Clippy, 41 tests/doc-tests and rustdoc; native Rust 1.88 MSRV tests; external facade and direct-component consumers; four compile-fail cases; seven-crate package verification.
- Existing alpha.2 CLI successfully queries/watches the candidate Agent; the candidate CLI explicitly rejects an alpha.2 Agent instead of silently using an unsupported protocol.
- Reconnection test replaces a live socket, observes a retry, and verifies fresh data from the replacement Agent.
- API check against `v0.1.0-alpha.2`: five libraries passed the patch-compatibility check (229 checks per library, 31 skipped). Macro behavior is covered separately by consumers/compile-fail tests.
- RustSec snapshot `21d909e4c8bd4e14c7aea58bf980c881a0053ab1`, last updated 2026-10-24: all six active lockfiles passed with zero advisories and zero warnings (root: 99 dependencies). CI refreshes the database; this is not an assurance against unknown vulnerabilities.
- AddressSanitizer/libFuzzer query/wire run: 577,872 executions in 61 seconds, no reported crash. The target and seed corpus are committed. A one-minute run is a smoke check, not exhaustive fuzz coverage.

## Classified sustained load

Each engine ran two minutes with four TCP clients, four observers and 10 ms requested publication. Errors are no longer collapsed into one undiagnosed count. Latency samples are bounded to the last 100,000 completed requests; results are not a new baseline-relative performance claim. Resource samples are taken every five seconds. These are development-trial observations; final-commit CI repeats the workload and resource gates on both Linux and macOS.

| Engine | Requests | Application errors | Observation errors | Publication errors | RSS after warmup | Threads | FDs |
|---|---:|---:|---|---|---|---|---|
| sync | 2,578,635 | 0 | {'busy': 4} | {} | 2.56–3.16 MiB | 7–7 | 9–13 |
| async | 2,704,181 | 0 | {'busy': 4} | {} | 2.75–3.36 MiB | 5–5 | 12–16 |

Both trials passed the correctness/resource gate: no business-request failure; only Busy observation failures, below 1%; no failed publication; no growing thread count or accumulating FDs in these samples. This identifies the failures in these runs, not the unclassified failures from the historical alpha.2 run. RSS ranges are not leak certification. Raw data: [stability-load-macos.json](stability-load-macos.json).

The initial supported workload envelope and thresholds are in [stability.md](stability.md). Multi-hour endurance and an independent adversarial security review remain release gates. The workflows provide two-minute push/PR trials and ten-minute manual/weekly trials. No external multi-hour or independent-review result is claimed here.

Tools: [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks), [Rust fuzzing guide](https://rust-fuzz.github.io/book/cargo-fuzz.html), [RustSec](https://rustsec.org/).
