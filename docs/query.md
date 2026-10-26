# Query and protocol

```text
SELECT (* | field [, field ...]) FROM source
  [WHERE predicate] [ORDER BY field [ASC | DESC]] [LIMIT nonnegative_integer] [;]
SELECT (COUNT(*) | MAX(field)) FROM source
  [WHERE predicate] [LIMIT nonnegative_integer] [;]
predicate := comparison | field IS [NOT] NULL | NOT predicate
           | (predicate) | predicate AND predicate | predicate OR predicate
comparison := field (= | != | <> | < | <= | > | >=) literal
literal := integer | decimal | 'SQL escaped string' | TRUE | FALSE | NULL
```

NOT binds more tightly than AND, which binds more tightly than OR. Keywords are case-insensitive; source/field names are case-sensitive. Identifiers can be double-quoted. Single-quoted strings escape a quote as `''`. Field names must be ASCII identifiers at registration. Only one source and one statement are allowed. Unsupported clauses fail instead of being ignored.

Without ORDER BY, rows preserve source order. LIMIT affects returned rows, not snapshot size or scan cost. `truncated` indicates that additional matching rows existed. LIMIT 0 returns no rows but still validates/scans. Projection order is preserved in `columns`; JSON row objects use sorted keys. Unknown fields fail even on empty collections. Comparisons require compatible scalar values; arrays/objects can only be projected or tested for NULL. Signed and unsigned integers compare through i128 without f64 rounding. Float-to-float comparisons work; mixed float/integer comparisons fail to avoid lossy conversion (use `1.0` for a floating field). Non-finite floats follow serde_json behavior and become null. Rust type metadata is descriptive, not a pre-execution type checker.

NULL comparison produces unknown. WHERE retains only true; NOT unknown remains unknown, false AND unknown is false, true OR unknown is true. Use IS NULL/IS NOT NULL to test absence. Complex enum values follow their serde representation, not a LiveQuery-specific tagging convention.

## Local wire format

One newline-delimited JSON request and response per Unix connection; unknown request fields are rejected. Protocol 2 clients first send `{"command":"hello","protocol":2}`. Success reports the version, Agent instance ID and capabilities. Unsupported versions return `incompatible_protocol`. Alpha.2 Agents do not implement Hello; the new CLI fails with an explicit compatibility error. Legacy version-one commands remain accepted. Protocol types are in `livequery_transport`; legacy exports remain in `livequery::local`.

```json
{"command":"schema"}
{"command":"query","sql":"SELECT id FROM players WHERE health < 20"}
```

A schema response contains a map from source name to columns (`name`, `rust_type`, `schema`). A query response:

```json
{"status":"ok","data":{"columns":[{"name":"id","rust_type":"u64"}],"rows":[{"id":42}],"scanned":2,"truncated":false}}
```

Errors:

```json
{"status":"error","code":"busy","message":"source busy; retry later"}
```

Categories: `deadline`, `busy`, `limit`, `not_found`, `invalid`, `serialization`, `io`. The CLI returns a nonzero exit on errors; `--json` makes successful outputs machine-readable while diagnostics remain on stderr. `list --json` emits one JSON object per live discovered agent. Query/watch JSON is one result payload per line, without the transport envelope.

## WATCH

`livequery watch 'SELECT ...' --pid PID` issues sequential conditional observations and sleeps after each completed request. Intervals must be 100–60000 ms. It emits a complete replacement result on the first poll and whenever rows, schema or truncation change. Scanned count alone does not trigger output. There is no row identity inference, added/removed event stream, stable timestamp or server history. Reordering counts as a change. Busy retries on the next poll; I/O interruptions use the bounded protocol-2 retry policy below, while other errors terminate the CLI watch with a diagnostic; application-level `Watch::poll` lets callers choose their own retry policy and retains its previous successful result on error.

`Watch` is a synchronous query helper, not a scheduler. Drop it to release the retained result. CLI Ctrl-C closes the current connection; the server has no stored subscription to cancel. Changes between snapshots, even a change followed by a reversal, can be missed. Slow consumers cannot build an event backlog. Each changed publication still adds independent filtering/sorting work per subscriber, but published input rows are shared; unchanged revisions do not clone or scan rows. Publication is an application checkpoint, not a guarantee of the freshest wall-clock state. A publisher can skip revisions between polls. Legacy v1/Watch revisions are sampled before execution, so a racing newer publication may cause one redundant subsequent scan; an older result is never labelled with a future revision.

Conditional observation request: `{"command":"observe","sql":"SELECT * FROM players","since":null}`. The response is `{"status":"observation","revision":{...},"data":{...}}`; send that opaque revision on the next poll. Equal revisions return `{"status":"not_modified","revision":{...}}`. Cursors belong to the same query, registry and agent lifetime; clear them when changing SQL or reconnecting to a restarted agent. They are an optimization hint, not authorization or persistent identity. The CLI keeps one fixed query per watch invocation. Registration identity prevents reusing a cursor after a source is replaced within a registry.


## Protocol 2 observation and diagnostics

Send `{"command":"observe_v2","sql":"SELECT * FROM players","since":null}`. A successful envelope contains `data: {"kind":"changed","cursor":{...},"result":{...}}`, or `data: {"kind":"unchanged","cursor":{...}}`. Echo the opaque cursor as `since`. The cursor binds to a UUID generated per Agent instance, the exact SQL byte digest, and the registration/publication revision. Recreating the Agent, changing SQL or replacing a source forces a full result. It is a cache hint, never an authentication credential.

`{"command":"status"}` returns published-source metadata: `sequence`, monotonic `age_ms`, wall-clock `published_at_unix_ms`, and cumulative failure counts by stable category. Empty initial Publications have null timestamps until a successful capture. Capture/publication failures do not replace the last successful snapshot; counters can increase without a revision change. Counters are independently sampled diagnostics, not a transactional event history. User-defined error messages are not exposed in these counters.

The v2 CLI retries at most three consecutive I/O failures, clears cached rows, cursor and freshness, and reports retries on stderr. A changed Agent session also clears the comparison baseline even when no I/O failure was observed: the new session emits a full first result even if its rows equal the previous session. Missing publication metadata clears any previously displayed age. Attempts count toward `--count`. Invalid frames and protocol errors terminate; it does not silently downgrade. Retries target the same socket address; a restart under a different PID requires an explicit new target. Version-one observation cursors keep their original same-query/same-session limitations; use v2 for restart safety.

## Ordering, aggregates and freshness

`ORDER BY field [ASC|DESC]` sorts matching rows before LIMIT; one scalar key is supported, including an unprojected field. NULL sorts last in either direction. Equal keys retain source snapshot order (not a globally stable identity order). Strings use Rust/Unicode scalar lexical ordering, booleans false before true, and integer comparison preserves all 64 bits. Mixed integer/float or other incompatible non-NULL scalar types are rejected. Objects and arrays are not sortable.

`SELECT COUNT(*) FROM table [WHERE ...]` returns one `count` value, including zero when no row matches. `SELECT MAX(field) ...` returns `max`, ignoring NULL and returning NULL for empty/all-NULL inputs. MAX uses the same homogeneous scalar comparison rules. Aggregates cannot mix with projections or ORDER BY; COUNT(field), GROUP BY and JOIN remain unsupported. LIMIT 0 omits the aggregate row and marks it truncated. LIMIT 1 does not reduce aggregate work: all source rows are counted in `scanned`, and `scan_rows`, byte and cooperative time budgets still apply. Sorting retains at most the bounded snapshot's row references, not another table copy; output still obeys result-row and byte budgets.

The optional `query_details` capability adds `QueryDetails` to protocol 2. Its payload preserves the QueryResult fields and adds publication/revision metadata for the actual immutable version used. ObserveV2 changed results also carry that metadata and the exact executed revision. Old Query requests and the public QueryResult struct are unchanged. Older protocol-2 Agents work with the new CLI's basic queries, which explicitly say when freshness is unavailable.

The CLI `session --pid PID` or `session --socket PATH` fixes a target and provides in-memory history and schema completion. `schema` refreshes completion. Session WATCH performs ten polls so the prompt is returned; shell WATCH remains unbounded unless `--count` is set. Human output distinguishes empty rows, unchanged publication/rows, Busy failures, never-published data and truncation. JSON stdout retains changed-result records; retry/Busy diagnostics go to stderr. Polling can miss intermediate states, and reconnect resets the comparison baseline. Key-based row diffs remain deferred.

## Resource and compatibility contracts

`Query::execute` applies the serialized-result byte budget to the existing `QueryResult`. `Query::execute_detailed` applies it to the entire `QueryExecution`, including publication and revision metadata. Both also enforce snapshot limits. Consequently a budget that fits a basic result may reject its detailed counterpart. Remote framing/envelopes have their own final encoding check; neither API promises that a payload at exactly the byte limit will fit inside a wire envelope.

`query_timeout` is cooperative, not a hard response-time bound. Checks occur during scanning and after result construction/encoding; a sort, string comparison, allocation or serialization operation already in progress is not preempted. Serialized byte limits do not cap heap use. See the [candidate stable contracts and version matrix](stable-contract.md) for supported combinations and lifecycle boundaries.

These descriptions include the stable-readiness fixes after alpha.4. The published alpha.4 CLI can retain an old age when metadata disappears and suppress identical rows after a session-only restart; its detailed query API checks result bytes before adding metadata. See the [incremental review](security-review-alpha4.md) for the corrections and regression coverage.
