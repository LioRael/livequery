#!/usr/bin/env python3
"""Read existing system records after failure; never record during measurement."""
import datetime
import json
import os
from pathlib import Path
import platform
import selectors
import subprocess
import sys
import time


def windows(case):
    anchor = json.loads((case / 'clock-anchor.json').read_text())
    offset = anchor['unix_ns'] - (anchor['monotonic_before_ns'] + anchor['monotonic_after_ns']) // 2
    ranges = []
    for path in sorted((case / 'events').glob('*.jsonl')):
        for line in path.read_text().splitlines():
            event = json.loads(line)
            if not event.get('deadline') or not event.get('points'):
                continue
            points = event['points']
            end = next((p for p in points if p['phase'] == 'deadline'), points[-1])
            ranges.append((points[0]['wall_ns'] + offset - 2_000_000_000,
                           end['wall_ns'] + offset + 2_000_000_000))
    merged = []
    for start, end in sorted(ranges):
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(end, merged[-1][1])
        else:
            merged.append([start, end])
    return merged, anchor['monotonic_after_ns'] - anchor['monotonic_before_ns']


def date(ns):
    return datetime.datetime.fromtimestamp(ns / 1e9, datetime.timezone.utc).strftime('%Y-%m-%d %H:%M:%S+0000')


def bounded(command, destination):
    cap = 2 * 1024 * 1024
    data = bytearray()
    reason = None
    started = time.monotonic()
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    with selectors.DefaultSelector() as selector:
        selector.register(proc.stdout, selectors.EVENT_READ)
        while True:
            if time.monotonic() - started > 20:
                reason = '20 second read limit'
                break
            ready = selector.select(.1)
            if not ready:
                continue
            block = os.read(proc.stdout.fileno(), min(65536, cap - len(data)))
            if not block:
                break
            data.extend(block)
            if len(data) == cap:
                reason = '2 MiB output limit'
                break
    if reason:
        proc.kill()
    proc.wait(timeout=5)
    proc.stdout.close()
    destination.write_bytes(data)
    return {'returncode': proc.returncode, 'truncated_reason': reason, 'bytes': len(data)}


def main(case):
    assert platform.system() == 'Darwin' and os.environ.get('GITHUB_ACTIONS') == 'true'
    assert os.environ.get('RUNNER_ENVIRONMENT') == 'github-hosted'
    ranges, uncertainty = windows(case)
    # Only kernel/power/policy records, not arbitrary application or runner logs.
    predicate = 'process IN {"kernel", "powerd", "runningboardd", "thermalmonitord", "watchdogd"}'
    records = []
    for index, (start, end) in enumerate(ranges[:4]):
        end = min(end, start + 10_000_000_000)
        start = start // 1_000_000_000 * 1_000_000_000
        end = (end + 999_999_999) // 1_000_000_000 * 1_000_000_000
        command = ['/usr/bin/log', 'show', '--style', 'ndjson', '--info', '--debug',
                   '--timezone', 'UTC', '--start', date(start), '--end', date(end),
                   '--predicate', predicate]
        result = bounded(command, case / f'failure-system-{index}.jsonl')
        records.append({'start_utc': date(start), 'end_utc': date(end), **result})
    (case / 'failure-system-log.json').write_text(json.dumps({
        'windows': records, 'omitted_windows': max(0, len(ranges) - 4),
        'anchor_width_ns': uncertainty, 'predicate': predicate,
        'limitations': 'Postmortem existing logs only. Missing records cannot exclude unlogged scheduling or hypervisor delays. A nearby event alone is not causation. Initial clock anchor does not rule out later wall-clock adjustment.'
    }, indent=2) + '\n')


if __name__ == '__main__':
    main(Path(sys.argv[1]))
