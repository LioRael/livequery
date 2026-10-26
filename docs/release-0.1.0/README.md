# 0.1.0 release verification

Publication is pending final source-bound CI and merge. Native standard Cargo workspace check, format, all-feature strict Clippy, tests, warnings-denied rustdoc, external consumers/compile-fail checks, CLI/jobs fixtures and seven-package verification passed. Core raw_value/arbitrary_precision regressions, alpha.2 protocol compatibility and current CLI against the registry alpha.4 host also passed. Native Rust 1.88 workspace tests passed. Exact logs are retained. Final CI supplies independent Linux/macOS and MSRV, API/audit/fuzz checks.

The final package version change is metadata-only at source level: see version-equivalence.json. Runtime sources and external dependency versions/features match tested clean 29b2f17 (library behavior 5bc2613). Original long-run binaries remain labeled with their actual alpha.4 source and hashes; no stable binary identity is inferred. Stable packages are rebuilt and verified separately.

The original four-cohort two-hour baseline and both Linux task-table complete workloads passed independent raw-report checks; see ../readiness-baseline-29b2f17 and ../readiness-task-attempt5. The owner explicitly removed only macOS task-table long endurance from release acceptance. Its failures and unresolved cause remain disclosed in ../stable-contract.md and CHANGELOG.md. Regular macOS CI/smoke and baseline are retained. This is a scope decision, not a fix or a macOS high-load pass.

Registry preflight found no 0.1.0 version of any of the seven packages. Publication receipts, checksums, VCS identities and yank state must be recorded after actual publication. No real-project validation or production SLO is claimed.
