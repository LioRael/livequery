# Alpha.4 incremental independent source review

Date: 2026-10-26. Reviewed release source: `28793e3c969b8536cc511c48b1eb39bdd1424571`, comparing the alpha.4 changes with `c006dc3dbbde5cd57a8329a6f8dbeb7dd90a8575`. Corrections are in the stable-readiness working tree based on documentation commit `e9898f5af687093e10483e9b407c79ee68febb89`; they are not retroactive fixes to the published alpha.4 crates. The corrections are recorded in behavior commit `75d83d5304ae5c046dd00dc478f4934863634f3c`; [validation status](stable-readiness-validation.md) separates local passes from pending CI endurance.

Two independent review agents inspected Core/Query and Agent/CLI lifecycle changes. A separate verifier checked all four candidate paths against the source and distinguished new defects from inherited behavior. These reviewers did not execute target code. Development regressions were implemented and run separately from the source review. This is a focused review, not third-party certification, a comprehensive audit, or a dynamic cross-UID attack demonstration.

## Scope and trust model

Reviewed shared snapshot ownership and version metadata, sorting and aggregate semantics/budgets, PublishedHandle and RunningAgent lifecycle, optional query metadata, interactive CLI integration and WATCH state. The underlying boundary remains local read-only diagnostics with root/same-UID processes and application serializers trusted. Remote requests must use explicitly published data. Deployment ACLs, privileged embedding and hostile same-user processes were not validated.

No new cross-principal boundary violation was substantiated in this increment. The findings below are correctness and resource-contract defects; they are not described as proven disclosure, authorization bypass or remote code execution.

## Confirmed findings and corrections

| Finding | Origin and affected path | Correction and regression |
|---|---|---|
| Detailed result omitted metadata from its byte-budget check | Alpha.4 `Query::execute_detailed` checked QueryResult before appending publication/revision. The complete returned object could exceed `Limits.bytes`; transport still enforced final response encoding limits. | Separate result-budget selection preserves the basic `execute` budget while checking the complete detailed object. `detailed_query_budgets_metadata_without_reducing_legacy_result_budget` checks the boundary. |
| Syntax position could identify the previous token | Alpha.4 location reporting used `pos - 1` even for failures that did not consume the unexpected token, such as missing FROM or an extra trailing token. | The parser distinguishes consumed, unconsumed and EOF positions. `syntax_errors_identify_the_unexpected_token_or_end_of_input` covers missing keywords, trailing tokens, invalid LIMIT, multiline input and EOF. |
| Missing metadata retained the previous Agent's snapshot age | Alpha.4 WATCH kept freshness across I/O retries and replies without publication metadata. A replacement protocol-2 Agent without metadata could later display the old age on Unchanged. | Retry and missing-metadata paths clear freshness. `missing_metadata_clears_previous_snapshot_age` and `interrupted_request_clears_cursor_and_result_before_retry` protect the transitions. |
| A session-only restart could suppress the new first result | Inherited from alpha.3: Changed replaced the cursor without comparing session identity; equal rows could be suppressed when restart completed between polls without an I/O error. | The CLI resets comparison state/freshness when Changed carries another session. `new_session_emits_identical_rows_without_an_io_failure` checks that both session first frames are emitted. |

Relevant regressions are in `crates/livequery/tests/query_readiness.rs` and `crates/livequery-cli/tests/watch_state.rs`. Regression success establishes these specified transitions, not a full security proof. Run the standard workspace checks on the final commit before release.

## Reviewed paths without a substantiated defect

- Publication reads pin one immutable version before deriving its snapshot, successful-publication metadata and revision. ObserveV2 uses the executed revision for changed results.
- SnapshotView retains immutable owned data. Retaining many views can keep old versions alive, but the inspected design does not itself keep an unbounded history after the last reader releases each version.
- Sorting validates matching scalar types before comparisons; NULL placement, stable ties and exact integer comparisons follow documented rules. MAX ignores NULL and rejects incompatible scalar types.
- The added query path did not introduce execution of application capture callbacks through remote requests.

These conclusions apply to the reviewed paths and trust model, not every possible future modification or deployment.

## Evidence still separate from this review

The four alpha.4 two-hour runs are recorded under [endurance-alpha4](endurance-alpha4/README.md). They used the previously published behavior source and a one-row diagnostic fixture. They do not validate behavior changes made during stable-readiness work, large task tables, hard timeout/shutdown guarantees, or dynamic permission attacks.

Representative task-table long-run results remain pending. A fresh registry-only alpha.4 installation walkthrough and current-CLI/registry-host walkthrough passed locally; final source-bound CI is tracked separately. Any changed runtime behavior needs validation on its final source revision. Real-project validation remains excluded by the user's scope. The original security boundary fixes and their source-only limitations are documented in [security-review-alpha3.md](security-review-alpha3.md).
