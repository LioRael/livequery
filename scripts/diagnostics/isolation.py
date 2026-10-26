#!/usr/bin/env python3
"""Fail closed on a busy macOS runner before starting measurements."""
import json
import os
from pathlib import Path
import subprocess
import time


def snapshot():
    rows = []
    output = subprocess.check_output(['ps', '-axo', 'pid=,pcpu=,comm='], text=True, timeout=5)
    for line in output.splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) == 3:
            rows.append({'pid': int(fields[0]), 'cpu_percent': float(fields[1]), 'executable': fields[2]})
    return {'unix_ns': time.time_ns(), 'loadavg': list(os.getloadavg()), 'processes': rows}


def assess(row, cpus, own_pid):
    # ps %CPU is a decaying estimate, not an interval CPU counter. Together
    # with load average this is a conservative admission heuristic only.
    external = [p for p in row['processes'] if p['pid'] != own_pid and Path(p['executable']).name != 'ps']
    cpu = sum(p['cpu_percent'] for p in external)
    conflicts = [p for p in external if Path(p['executable']).name in {'cargo', 'rustc', 'xctrace', 'livequery-workload'}]
    return {'admitted': not conflicts and row['loadavg'][0] <= cpus and cpu <= 25,
            'external_cpu_percent': cpu, 'conflicting_processes': conflicts}


def require_idle(path):
    """One 15-second observation, no wait-until-green loop or process killing."""
    records = []
    for index in range(4):
        if index:
            time.sleep(5)
        row = snapshot()
        row.update(assess(row, os.cpu_count() or 1, os.getpid()))
        records.append(row)
    result = {'purpose': 'environment admission, not the original workload acceptance gate',
              'logical_cpus': os.cpu_count(), 'max_external_cpu_percent': 25,
              'max_load1_per_logical_cpu': 1, 'samples': records,
              'admitted': all(r['admitted'] for r in records)}
    Path(path).write_text(json.dumps(result, indent=2) + '\n')
    if not result['admitted']:
        raise RuntimeError('Runner isolation unavailable; evidence saved, no measurement started, no automatic retry')
