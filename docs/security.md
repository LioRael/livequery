# Security, ownership and consistency

## Exposure boundary

Only explicitly registered sources can be discovered. Deriving Inspect exposes all non-skipped named fields; it is not an automatic secret detector. `#[inspect(skip)]` removes a field from schema and data before invoking any serialization. A skipped field does not need Serialize. Review nested structs/enums: their serde implementations control their contents, and skipping fields at the outer level does not recursively redact them. Prefer a dedicated public diagnostic struct.

Sockets live in a per-user temporary directory created mode 0700 and verified to be a real directory owned by the program’s Unix real UID (via rustix). Socket mode is 0600. No existing socket is overwritten or unlinked during bind; duplicate agents fail. Server drop removes its bound socket only if the path still refers to the same filesystem object. Forced process termination can leave stale sockets; `list` probes and omits stale endpoints. Inspect ownership/process state before manually removing one. PID reuse may make a stale socket prevent a later bind; that fails closed rather than replacing an unknown endpoint.

Same-user processes and root are trusted. There is no per-source ACL, token authentication or encrypted transport. Do not use in setuid programs, with an untrusted parent directory, or as a multi-tenant access-control boundary. Custom socket locations are the embedding application's responsibility. No public TCP listener is provided. The Tokio example's separate TCP echo service binds loopback only.

## Bounds

| Budget | Default |
|---|---:|
| Source rows | 10,000 |
| Result rows | 1,000 |
| Serialized snapshot/result/response | 1 MiB |
| SQL bytes / tokens / nesting | 4,096 / 256 / 32 |
| Request frame | 8,192 bytes |
| Active clients | 16 |
| Server request read / response write deadline | 2 s / 2 s |
| Client connect / write / read deadline | 2 s / 2 s / 2 s |

Read and write deadlines are absolute, so trickling bytes does not reset the timeout. Up to sixteen clients are advanced through nonblocking read/write phases; a slow peer does not hold the only service slot. A single polling thread bounds thread count and processes trusted source callbacks serially. When all slots are occupied, further connections stay in the OS backlog and the client connect deadline limits waiting. Same-user clients can still exhaust capacity; this is not a multi-tenant denial-of-service boundary. Cancellation drops active connections on the next loop iteration (normally within 5 ms), unless a trusted callback is executing. Client connect, request write and response read each have a separate two-second deadline.

Schema conversion and response serialization use capped writers. A fixed small diagnostic may exceed an embedding application's unusually tiny response budget so the client can understand a limit failure. The active-client cap bounds retained response buffers to sixteen times the configured response budget, plus representation overhead. Custom schemas are still constructed by trusted application code before encoding.

Budgets are checked during derived field serialization and per-row snapshot construction, then again at the snapshot/result boundary. They bound serialized data, not exact heap usage: JSON values, BTreeMap nodes, row names and allocator capacity add overhead. Fields are serialized through a capped writer, then decoded into owned values. Custom Serialize implementations may allocate internally or perform arbitrary work and are trusted. A hand-written Inspect or Source must enforce limits during construction; validating its return value cannot undo excess allocation. Snapshot schema must match the registered adapter's schema and each row's keys.

## Host impact

`Table` holds one read lock for the complete snapshot; every row is from the same lock-protected state. It does not establish consistency between independently registered sources. Registry locks are not held while calling Source methods. Lock acquisition uses try_read/try_write; `Busy` is immediate. An external application writer using blocking RwLock::write can still wait for a snapshot reader. The maximum lock hold time depends on field serialization, even with a byte cap. Library code cannot safely forcibly terminate a Rust callback.

Run the agent on a caller-owned thread, not directly on an async executor worker. Keep serializers and custom sources bounded and nonblocking; use precomputed diagnostic views where a hard host latency budget is necessary. No production hard-real-time guarantee is made. Poisoned application locks indicate a broken invariant and panic rather than exposing potentially inconsistent data. The library never catches application panics and pretends it recovered.

Registrations own their source through an Arc internally. Dropping a guard performs no registry lock acquisition; expired weak entries are hidden from discovery and cleaned on later registration. In-flight snapshots retain the source until completion. WATCH retains only its last successful result. No implicit history, unbounded queue or background observer exists.
