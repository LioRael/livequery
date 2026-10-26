# Diagnose a stuck worker

This repository fixture answers a concrete question: requests are still accepted, but which jobs stopped progressing, and are they concentrated on one worker? No code changes are needed during the investigation. The diagnostic endpoint remains read-only; repair is an explicit business command in the service terminal.

Build the unpublished candidate:

```sh
cargo build --workspace --bins
./target/debug/jobs
```

The service prints its PID/socket, accepts `submit` on stdin, and starts twelve jobs. Worker 2 is deliberately stalled; workers 1 and 3 finish after about 250 ms. In a second terminal, replace `PID` below with the printed number:

```sh
./target/debug/livequery session --pid PID
```

The target stays fixed. Tab completes table/field names and the arrow keys navigate up to 100 in-memory history entries. `schema` refreshes the completion catalog. History is never written to disk.

First inspect `schema` and `status`, then issue:

```sql
SELECT id, worker_id, elapsed_ms FROM jobs WHERE state = 'running' ORDER BY elapsed_ms DESC LIMIT 20;
SELECT COUNT(*) FROM jobs WHERE state = 'running';
SELECT MAX(elapsed_ms) FROM jobs WHERE state = 'running';
SELECT id, retry_count FROM jobs WHERE state = 'running' AND worker_id = 2;
```

After the initial 250 ms, four running jobs belong to worker 2. The result shows snapshot age; `status` distinguishes a never-published table from one whose last good publication is stale. An empty result is not an observation error. `LIMIT` can omit matching rows, so inspect the `limited` marker rather than treating the displayed rows as the full table.

In the first terminal, enter `submit`. Receiving `accepted` proves the application remains responsive despite the stuck worker. Then in the session:

```text
watch SELECT COUNT(*) FROM jobs WHERE state = 'running'
```

Session WATCH runs ten polls and returns to the prompt; repeat it if needed. For an unbounded separate-shell WATCH, use `livequery watch 'SELECT COUNT(*) FROM jobs WHERE state = '\''running'\''' --pid PID`. In the first terminal enter `repair 2`. WATCH should report the running count falling to zero. `SELECT id, state, retry_count FROM jobs WHERE worker_id = 2` confirms recovery and the retry counter. Enter `quit` in each terminal when finished. The Agent joins and removes its socket.

The service retains at most 128 active jobs and removes completed jobs when accepting new work. It captures every 100 ms. Refresh failure logs a diagnostic and retains the previous snapshot; it does not terminate the service or stop ingress. Copying a small view under the business lock precedes serialization. The fixture intentionally uses stdin for its business controls so there is no second management server to configure.

Run the reproducible acceptance test with `python3 scripts/jobs-smoke.py` after building. It uses the actual service and CLI to verify stalled-worker identification, sorted/aggregate queries, continued ingress, WATCH recovery, session error recovery and socket cleanup. This is repository fixture evidence, not real-project adoption evidence. Real-project integration remains outside the current acceptance scope.
