#!/usr/bin/env python3
"""Fail closed on a busy macOS runner before starting measurements."""
import ctypes
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


def cpu_ticks():
    # SDK mach/host_info.h: HOST_CPU_LOAD_INFO=3, four natural_t counters.
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
    lib.mach_host_self.restype = ctypes.c_uint
    lib.host_statistics.argtypes = [ctypes.c_uint, ctypes.c_int,
                                   ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint)]
    ticks = (ctypes.c_uint * 4)()
    count = ctypes.c_uint(4)
    status = lib.host_statistics(lib.mach_host_self(), 3, ticks, ctypes.byref(count))
    if status != 0 or count.value != 4:
        raise RuntimeError(f'HOST_CPU_LOAD_INFO unavailable: {status}, {count.value}')
    return list(ticks)


def interval_busy_percent(before, after, cpus):
    delta = [(b - a) % (1 << 32) for a, b in zip(before, after)]
    total = sum(delta)
    if len(delta) != 4 or total == 0:
        raise ValueError('Missing or nonadvancing CPU ticks')
    # CPU_STATE_IDLE=2. 100% means one logical CPU, as in ps.
    return 100 * cpus * (total - delta[2]) / total


def conflicts(row):
    return [p for p in row['processes'] if Path(p['executable']).name in
            {'cargo', 'rustc', 'xctrace', 'livequery-workload'}]


def require_idle(path):
    """One 15-second observation, no wait-until-green loop or process killing."""
    records = []
    cpus = os.cpu_count() or 1
    previous = None
    for index in range(4):
        if index:
            time.sleep(5)
        row = snapshot()
        row['cpu_ticks'] = cpu_ticks()
        row['monotonic_ns'] = time.monotonic_ns()
        row['conflicting_processes'] = conflicts(row)
        if previous is not None:
            row['interval_busy_percent'] = interval_busy_percent(previous['cpu_ticks'], row['cpu_ticks'], cpus)
            row['interval_seconds'] = (row['monotonic_ns'] - previous['monotonic_ns']) / 1e9
            row['admitted'] = (not row['conflicting_processes']
                               and 0 < row['interval_seconds'] <= 10
                               and row['interval_busy_percent'] <= 25)
        records.append(row)
        previous = row
    result = {'purpose': 'environment admission, not the original workload acceptance gate',
              'logical_cpus': cpus, 'max_interval_busy_percent': 25,
              'measurement': 'HOST_CPU_LOAD_INFO tick deltas; includes observer overhead; loadavg and ps CPU advisory only',
              'samples': records,
              'admitted': not records[0]['conflicting_processes'] and all(r['admitted'] for r in records[1:])}
    Path(path).write_text(json.dumps(result, indent=2) + '\n')
    if not result['admitted']:
        raise RuntimeError('Runner isolation unavailable; evidence saved, no measurement started, no automatic retry')
