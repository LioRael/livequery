#!/usr/bin/env python3
"""Real TCP load with classified diagnostics and bounded latency/resource sampling."""
import argparse
import asyncio
from collections import Counter, deque
import json
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import time
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
SAMPLES = 100_000


def quantile(values, q):
    return sorted(values)[int((len(values) - 1) * q)] if values else None


def resources(pid):
    """Read the host only; unsupported measurements are null, never zero."""
    sample = {"rss_bytes": None, "threads": None, "fds": None}
    try:
        sample["rss_bytes"] = int(subprocess.check_output(
            ["ps", "-o", "rss=", "-p", str(pid)], text=True, timeout=2).strip()) * 1024
        if sys.platform == "darwin":
            sample["threads"] = len(subprocess.check_output(
                ["ps", "-M", "-p", str(pid)], text=True, timeout=2).splitlines()) - 1
            output = subprocess.check_output(
                ["/usr/sbin/lsof", "-a", "-p", str(pid), "-Ff", "-n", "-P"],
                text=True, timeout=2)
            sample["fds"] = sum(line[1:].isdigit() for line in output.splitlines() if line.startswith("f"))
        else:
            sample["threads"] = len(list(Path(f"/proc/{pid}/task").iterdir()))
            sample["fds"] = len(list(Path(f"/proc/{pid}/fd").iterdir()))
    except (OSError, ValueError, subprocess.SubprocessError):
        pass
    return sample


async def drive(meta, mode, seconds, subscribers, poll_ms, task_rows=0):
    start = time.perf_counter()
    finish = start + seconds
    latency, query_latency = deque(maxlen=SAMPLES), deque(maxlen=SAMPLES)
    errors, query_errors = Counter(), Counter()
    completed = observations = unchanged = 0
    samples = []
    query_kinds = Counter()

    async def load():
        nonlocal completed
        reader, writer = await asyncio.open_connection("127.0.0.1", meta["port"])
        try:
            while time.perf_counter() < finish:
                before = time.perf_counter()
                try:
                    writer.write(b"x" * 64)
                    await writer.drain()
                    assert await asyncio.wait_for(reader.readexactly(64), 1) == b"x" * 64
                    completed += 1
                    latency.append((time.perf_counter() - before) * 1e6)
                except Exception as error:
                    errors[type(error).__name__] += 1
                    break
        finally:
            writer.close()
            await writer.wait_closed()

    async def observe():
        nonlocal observations, unchanged
        cursor = None
        last_completed = 0
        last_sequence = 0
        turn = 0
        while time.perf_counter() < finish:
            before = time.perf_counter()
            writer = None
            try:
                reader, writer = await asyncio.open_unix_connection(meta["socket"])
                request = {"min_completed": 0} if mode == "manual" else {
                    "command": "observe_v2", "sql": "SELECT completed, bytes FROM counters WHERE completed > 0", "since": cursor}
                kind = ["sort", "count", "max"][turn % 3]
                if mode == "jobs":
                    sql = {"sort": "SELECT id, elapsed_ms, retry_count FROM jobs WHERE state = 'running' ORDER BY elapsed_ms DESC LIMIT 20", "count": "SELECT COUNT(*) FROM jobs WHERE state = 'running'", "max": "SELECT MAX(elapsed_ms) FROM jobs WHERE state = 'running'"}[kind]
                    request = {"command": "query_details", "sql": sql}
                writer.write(json.dumps(request).encode() + b"\n")
                await writer.drain()
                reply = json.loads(await asyncio.wait_for(reader.readline(), 1))
                if mode == "jobs":
                    if reply["status"] == "error":
                        query_errors[reply["code"]] += 1
                        continue
                    result = reply["data"]
                    assert result["scanned"] == task_rows, "incomplete task scan"
                    sequence = result["publication"]["sequence"]
                    assert sequence >= last_sequence and sequence > 0, "publication regressed/uninitialized"
                    last_sequence = sequence
                    rows = result["rows"]
                    if kind == "count":
                        assert rows == [{"count": task_rows}], "incorrect count"
                    elif kind == "max":
                        assert isinstance(rows[0]["max"], int) and rows[0]["max"] >= 0
                    else:
                        assert len(rows) == min(task_rows, 20)
                        assert len({r["id"] for r in rows}) == len(rows)
                        assert all(0 <= r["id"] < task_rows and r["retry_count"] == r["elapsed_ms"] // 100 for r in rows), "incoherent task fields"
                        assert rows == sorted(rows, key=lambda r: (-r["elapsed_ms"], r["id"])), "incorrect/stability order"
                        assert result["truncated"] == (task_rows > 20)
                    query_kinds[kind] += 1
                    turn += 1
                if mode == "published":
                    if reply["status"] == "error":
                        query_errors[reply["code"]] += 1
                        continue
                    observed = reply["data"]
                    cursor = observed["cursor"]
                    unchanged += observed["kind"] == "unchanged"
                    if observed["kind"] == "changed":
                        rows = observed["result"]["rows"]
                        if rows:
                            current = rows[0]["completed"]
                            assert current >= last_completed, "published counters regressed"
                            assert rows[0]["bytes"] == current * 64, "incoherent snapshot"
                            last_completed = current
                observations += 1
                query_latency.append((time.perf_counter() - before) * 1e6)
            except Exception as error:
                query_errors[type(error).__name__] += 1
            finally:
                if writer is not None:
                    writer.close()
                    await writer.wait_closed()
                # Errors also back off instead of turning contention into a busy loop.
                await asyncio.sleep(poll_ms / 1000)

    async def monitor():
        while time.perf_counter() < finish:
            sample = await asyncio.to_thread(resources, meta["pid"])
            samples.append({"elapsed_seconds": time.perf_counter() - start,
                            "requests": completed, "request_errors": sum(errors.values()),
                            "observations": observations, "observation_errors": sum(query_errors.values()), "query_kinds": dict(query_kinds), **sample})
            if len(samples) % 12 == 1:
                print(json.dumps({"progress_seconds": round(time.perf_counter() - start),
                                  "requests": completed, "request_errors": dict(errors),
                                  "observation_errors": dict(query_errors), **sample}), flush=True)
            await asyncio.sleep(max(0, min(5, finish - time.perf_counter())))

    await asyncio.gather(*(load() for _ in range(4)), *(observe() for _ in range(subscribers)), monitor())
    elapsed = time.perf_counter() - start
    return dict(elapsed_seconds=elapsed, requests=completed, requests_per_second=completed / elapsed,
                p50_us=quantile(latency, .5), p95_us=quantile(latency, .95), p99_us=quantile(latency, .99),
                latency_samples=len(latency), request_errors=sum(errors.values()), request_error_codes=dict(errors),
                observations=observations, not_modified=unchanged, observation_errors=sum(query_errors.values()),
                observation_error_codes=dict(query_errors), observation_p99_us=quantile(query_latency, .99),
                resource_samples=samples, query_kinds=dict(query_kinds))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--seconds", type=int, default=2)
    parser.add_argument("--output", default="docs/service-benchmark.json")
    parser.add_argument("--binary", default="experiments/workload/target/release/livequery-workload")
    parser.add_argument("--only", choices=["baseline", "soak"])
    parser.add_argument("--engine", choices=["sync", "async", "both"], default="both")
    parser.add_argument("--task-rows", type=int, default=0)
    parser.add_argument("--string-bytes", type=int, default=32)
    parser.add_argument("--publish-ms", type=int, default=10)
    parser.add_argument("--subscribers", type=int, default=4)
    parser.add_argument("--poll-ms", type=int, default=10)
    args = parser.parse_args()
    if not 0 <= args.task_rows <= 10000 or not 0 <= args.string_bytes <= 4096 or not 1 <= args.publish_ms <= 60000 or not 0 <= args.subscribers <= 16 or not 1 <= args.poll_ms <= 60000:
        parser.error("invalid workload bounds")
    if args.seconds < 1 or args.repeats < 1:
        parser.error("seconds and repeats must be positive")
    started_at = datetime.now(timezone.utc).isoformat()
    source_ref = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    engines = ["sync", "async"] if args.engine == "both" else [args.engine]
    binary = ROOT / args.binary
    rows = []
    for repeat in range(args.repeats):
        for engine in engines:
            cases = [("baseline", 0, 10), ("manual", 1, 10), ("published", 0, 10),
                     ("published", 1, 10), ("published", 4, 10), ("published", 4, 50)]
            if args.only == "baseline":
                cases = [("baseline", 0, 10)]
            if args.only == "soak":
                cases = [("published", 4, 10)]
            if args.task_rows and args.only != "baseline":
                cases = [("jobs", args.subscribers, args.publish_ms)]
            cases = cases[repeat:] + cases[:repeat]
            for mode, subscribers, publish_ms in cases:
                proc = subprocess.Popen([str(binary), engine, mode, str(args.seconds + 3), str(publish_ms), str(args.task_rows), str(args.string_bytes)],
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                try:
                    meta = json.loads(proc.stdout.readline())
                    time.sleep(.1)
                    metrics = asyncio.run(drive(meta, mode, args.seconds, subscribers, args.poll_ms, args.task_rows))
                    _, status, usage = os.wait4(proc.pid, 0)
                    proc.returncode = os.waitstatus_to_exitcode(status)
                    if proc.returncode:
                        raise RuntimeError(proc.stderr.read())
                    summary = json.loads(proc.stdout.readline())
                    row = dict(repeat=repeat, engine=engine, mode=mode, subscribers=subscribers,
                               publish_ms=publish_ms, poll_ms=args.poll_ms, diagnostic_snapshot_bytes=meta.get("diagnostic_snapshot_bytes", 0), task_rows=args.task_rows, string_bytes=args.string_bytes, host_cpu_seconds=usage.ru_utime + usage.ru_stime,
                               host_peak_rss_bytes=usage.ru_maxrss * (1 if sys.platform == "darwin" else 1024),
                               **metrics, **summary)
                    rows.append(row)
                    print(json.dumps(row), flush=True)
                finally:
                    if proc.poll() is None:
                        proc.terminate()
                        proc.wait()
    output = ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(dict(platform=sys.platform, source_ref=source_ref, source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()), task_rows=args.task_rows, string_bytes=args.string_bytes, started_at=started_at, finished_at=datetime.now(timezone.utc).isoformat(), engines=engines, binary=args.binary, binary_bytes=binary.stat().st_size, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                                     repeats=args.repeats, load_seconds=args.seconds, load_clients=4, payload_bytes=64,
                                     latency_policy="last 100000 completed requests; bounded memory, not a whole-run tail",
                                     results=rows), indent=2) + "\n")


if __name__ == "__main__":
    main()
