#!/usr/bin/env python3
"""Read host CPU counters for evidence, not an admission gate."""
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
