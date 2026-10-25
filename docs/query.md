# Query and protocol

```text
SELECT (* | field [, field ...]) FROM source
  [WHERE predicate] [LIMIT nonnegative_integer] [;]
predicate := comparison | field IS [NOT] NULL | NOT predicate
           | (predicate) | predicate AND predicate | predicate OR predicate
comparison := field (= | != | <> | < | <= | > | >=) literal
literal := integer | decimal | 'SQL escaped string' | TRUE | FALSE | NULL
```

NOT binds more tightly than AND, which binds more tightly than OR. Keywords are case-insensitive; source/field names are case-sensitive. Identifiers can be double-quoted. Single-quoted strings escape a quote as `''`. Field names must be ASCII identifiers at registration. Only one source and one statement are allowed. Unsupported clauses fail instead of being ignored.

Rows preserve source order. LIMIT affects returned rows, not snapshot size or scan cost. `truncated` indicates that additional matching rows existed. LIMIT 0 returns no rows but still validates/scans. Projection order is preserved in `columns`; JSON row objects use sorted keys. Unknown fields fail even on empty collections. Comparisons require compatible scalar values; arrays/objects can only be projected or tested for NULL. Signed and unsigned integers compare through i128 without f64 rounding. Float-to-float comparisons work; mixed float/integer comparisons fail to avoid lossy conversion (use `1.0` for a floating field). Non-finite floats follow serde_json behavior and become null. Rust type metadata is descriptive, not a pre-execution type checker.

NULL comparison produces unknown. WHERE retains only true; NOT unknown remains unknown, false AND unknown is false, true OR unknown is true. Use IS NULL/IS NOT NULL to test absence. Complex enum values follow their serde representation, not a LiveQuery-specific tagging convention.

## Local wire format, version 1

One newline-delimited JSON request and response per Unix connection; unknown request fields are rejected. No handshake or version negotiation yet. Source code types are in `livequery::local`.

```json
{"command":"schema"}
{"command":"query","sql":"SELECT id FROM players WHERE health < 20"}
```

A schema response contains a map from source name to columns (`name`, `rust_type`). A query response:

```json
{"status":"ok","data":{"columns":[{"name":"id","rust_type":"u64"}],"rows":[{"id":42}],"scanned":2,"truncated":false}}
```

Errors:

```json
{"status":"error","code":"busy","message":"source busy; retry later"}
```

Categories: `busy`, `limit`, `not_found`, `invalid`, `serialization`, `io`. The CLI returns a nonzero exit on errors; `--json` makes successful outputs machine-readable while diagnostics remain on stderr. `list --json` emits one JSON object per live discovered agent. Query/watch JSON is one result payload per line, without the transport envelope.

## WATCH

`livequery watch 'SELECT ...' --pid PID` issues sequential queries and sleeps after each completed request. Intervals must be 100–60000 ms. It emits a complete replacement result on the first poll and whenever rows, schema or truncation change. Scanned count alone does not trigger output. There is no row identity inference, added/removed event stream, stable timestamp or server history. Reordering counts as a change. Busy/error terminates the CLI watch with a diagnostic; application-level `Watch::poll` lets callers choose their own retry policy and retains its previous successful result on error.

`Watch` is a synchronous query helper, not a scheduler. Drop it to release the retained result. CLI Ctrl-C closes the current connection; the server has no stored subscription to cancel. Changes between snapshots, even a change followed by a reversal, can be missed. Slow consumers cannot build an event backlog. Each subscriber still adds independent full snapshot work; poll sparingly.
