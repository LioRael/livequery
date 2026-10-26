# Alpha.3 independent security review

Date: 2026-10-26. Baseline: `1980016`. This is a focused, source-based review of LiveQuery's local diagnostic boundaries by two independent review agents, followed by a separate verifier. The reviewers did not implement the original hardening or execute attack probes. It is not a third-party certification or a comprehensive audit of deployment environments.

## Scope and assumptions

Reviewed Core, Query, Derive, Transport, Agent and CLI: opt-in publication, field omission, parser/query budgets, snapshot ownership, registration lifecycle, Unix path/permission checks, wire framing, connection limits, deadlines, cursors and reconnect behavior. Root and same-UID processes remain trusted; custom serializers and application callbacks are trusted. Standard Unix ownership/mode semantics are assumed. Deployment-specific ACLs, privileged/setuid embedding and hostile same-user applications are outside this supported boundary.

## Findings and fixes

1. **Ignored enum omission attributes.** `Inspect` accepted `#[inspect(skip)]` on enum fields/variants but the enum implementation serialized the entire value through serde. An explicitly excluded field could therefore enter the diagnostic snapshot and schema. The derive now rejects `inspect` attributes on containers and enum variants/fields; enum fields must use `#[serde(skip)]`. Four compile-fail cases cover these positions; a positive regression checks root and nested enums omit a dummy secret from both data and schema. Named-struct field omission remains supported. This is a source-confirmed confidentiality contract defect, not a cross-UID authorization bypass.
2. **Directory validation/use mismatch.** Transport validated a parent path, then resolved the original path again when binding/connecting (or reused the original path for discovery). An intermediate alias controlled outside the canonical trusted tree could change the actual target after validation. The validator now resolves once, verifies every canonical directory from root to leaf using non-following metadata, and returns that exact path for all three callers. Canonical leaf ownership/privacy is also checked. A deterministic regression changes the original alias after validation and verifies the checked destination is retained; a newly nonprivate leaf is rejected. The source-level gap and fix were independently confirmed. No cross-UID exploit or data disclosure was dynamically reproduced.
3. **Interrupted response prevented WATCH recovery.** EOF before a complete response was categorized as invalid input, bypassing bounded I/O retries. It now produces `UnexpectedEof` through the I/O error path. The end-to-end CLI regression accepts and closes a pending request, restores an Agent with new data and checks that WATCH retries successfully. Complete malformed JSON still fails closed. This is a reliability defect, not a security-boundary violation.

The separate verifier reviewed the implementation and agreed the two source-level control gaps are closed under the documented trust model. Other reviewed paths did not reveal a substantiated boundary violation. This is not a guarantee of vulnerability absence.

## Evidence limits

The security reviewers used read-only source inspection. Dynamic adversarial cross-UID races were not run: this workstation has no active isolated Linux container runtime providing the security skill's complete sandbox controls. Standard development regressions test the corrected contracts without impersonating another user or attacking shared services. A dedicated isolated two-UID race test remains additional defense-in-depth evidence, not a claimed result of this review.

Two-hour fixture endurance is tracked separately by the `Two-hour endurance` workflow, with one job per OS/engine, source commit and binary hash, raw metrics and toolchain artifacts. Its result must be checked for the actual reviewed implementation; starting a run is not completion evidence.
