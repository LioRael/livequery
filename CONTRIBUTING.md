# Contributing

Use Rust 2024. The minimum supported compiler is Rust 1.88. CI checks that compiler separately from stable; local validation also covers 1.88.

Run the commands in README before submitting changes. Add public-behavior tests for new query syntax and reject unsupported SQL explicitly. Treat Source and serializer implementations as trusted code; never hide their blocking behavior or convert poisoning into success. Preserve field filtering before serialization and retain exact integer comparisons.

Keep runtime and framework dependencies optional. Do not add history, transports or UI endpoints without a concrete use case, resource budgets and documented consistency/security semantics. Record benchmark environment and raw data; avoid claims based on a single favorable sample. Run `bash scripts/check-release.sh` before tagging. The first release is source-only; package publication and precompiled artifacts are separate actions.
