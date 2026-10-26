#!/usr/bin/env python3
"""Bounded CI diagnosis, not release acceptance. All library edits stay in an archive."""
import argparse
import ctypes
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tarfile
import time

from isolation import cpu_ticks, interval_busy_percent

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
BASE = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
p = argparse.ArgumentParser()
p.add_argument('action', choices=['prepare', 'package', 'run'])
p.add_argument('--seconds', type=int, default=300)
p.add_argument('--task-rows', type=int, default=5000)
p.add_argument('--string-bytes', type=int, default=96)
p.add_argument('--variant', choices=['official', 'diagnostic'], default='diagnostic')
p.add_argument('--engine', choices=['sync', 'async'], default='sync')
a = p.parse_args()
assert platform.system() == 'Darwin'
assert 1 <= a.seconds <= 600
out = ROOT / 'target/ci-offcpu'
src = out / 'source'
out.mkdir(parents=True, exist_ok=True)
patch = HERE / 'instrumentation.patch'

def run(cmd, **kwargs):
    return subprocess.run(cmd, check=True, **kwargs)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def processes():
    result = []
    for line in subprocess.check_output(['ps', '-axo', 'pid=,pcpu=,comm='], text=True).splitlines():
        fields = line.strip().split(None, 2)
        if len(fields) == 3:
            result.append({'pid': int(fields[0]), 'cpu_percent': float(fields[1]), 'executable': fields[2]})
    return result

# No arguments or environment are logged: they can contain runner credentials.
if a.action == 'prepare':
    src.mkdir(exist_ok=False)
    archive = subprocess.check_output(['git', 'archive', '--format=tar', BASE], cwd=ROOT)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(src)
    official = out / 'official-source'
    official.mkdir(exist_ok=False)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(official)
    run(['git', 'apply', '--check', str(patch)], cwd=src)
    run(['git', 'apply', str(patch)], cwd=src)
    (out / 'provenance.json').write_text(json.dumps({
        'official_base': BASE,
        'workflow_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'patch_sha256': digest(patch),
        'diagnostic_only': True,
        'runner_image': os.environ.get('ImageVersion'),
        'runner_os_image': os.environ.get('ImageOS'),
        'runner_arch': platform.machine(),
        'os': platform.platform(),
        'logical_cpus': os.cpu_count(),
        'seconds_per_engine': a.seconds,
        'limitations': 'Allocator clocks and 10ms sampling add overhead. A passing diagnostic does not meet final endurance gates. VM isolation does not establish exclusive physical-host CPU.'
    }, indent=2) + '\n')
    sys.exit(0)

if a.action == 'package':
    (out/'bin').mkdir(exist_ok=True)
    hashes = {'native-sampler': digest(out/'bin/native-sampler')}
    for variant, directory in [('official', out/'official-source'), ('diagnostic', src)]:
        binary = directory/'experiments/workload/target/release/livequery-workload'
        destination = out/'bin'/variant
        shutil.copy2(binary, destination)
        hashes[variant] = digest(destination)
    (out/'build-manifest.json').write_text(json.dumps({'binary_sha256': hashes, 'build_provenance': json.loads((out/'provenance.json').read_text())}, indent=2)+'\n')
    (out/'instrumentation.patch').write_bytes(patch.read_bytes())
    sys.exit(0)

binary = out/'bin'/a.variant
manifest = json.loads((out/'build-manifest.json').read_text())
assert digest(binary) == manifest['binary_sha256'][a.variant], 'Downloaded binary checksum mismatch'
binary.chmod(0o755)
assert manifest['build_provenance']['seconds_per_engine'] == a.seconds
(out/'measurement-provenance.json').write_text(json.dumps({
    'variant': a.variant, 'engine': a.engine, 'os': platform.platform(),
    'logical_cpus': os.cpu_count(), 'runner_image': os.environ.get('ImageVersion'),
    'runner_os_image': os.environ.get('ImageOS'), 'separate_build_job': True,
    'binary_sha256_verified': digest(binary), 'cpu_admission_gate': False,
    'workflow_head': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
}, indent=2)+'\n')
# Cooperative per-runner exclusion; it does not claim to lock other hosted VMs.
lock = open(Path(os.environ.get('RUNNER_TEMP', '/tmp')) / 'livequery-measurement.lock', 'w')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
blocked = {'cargo', 'rustc', 'xctrace', 'livequery-workload'}
active = [r for r in processes() if Path(r['executable']).name in blocked]
(out/'preflight.json').write_text(json.dumps({'conflicting_processes': active}, indent=2)+'\n')
if active:
    raise SystemExit('Measurement isolation preflight failed; no processes were killed and no load started')
(out/'binary-sha256.txt').write_text(digest(binary)+'\n')
(out/'instrumentation.patch').write_bytes(patch.read_bytes())
clocklib = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
class TS(ctypes.Structure):
    _fields_ = [('sec', ctypes.c_long), ('nsec', ctypes.c_long)]
def mono():
    t = TS()
    if clocklib.clock_gettime(6, ctypes.byref(t)) != 0:
        raise OSError(ctypes.get_errno(), 'CLOCK_MONOTONIC')
    return t.sec * 1_000_000_000 + t.nsec

verdicts = []
for engine in [a.engine]:
    case = out / engine
    (case/'events').mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env.pop('LQ_VALIDATE_PAUSE', None)
    env.pop('LQ_DIAGNOSTIC_DIR', None)
    if a.variant == 'diagnostic':
        env['LQ_DIAGNOSTIC_DIR'] = str(case/'events')
    before = mono()
    anchor = {'monotonic_before_ns': before, 'unix_ns': time.time_ns(), 'monotonic_after_ns': mono()}
    (case/'clock-anchor.json').write_text(json.dumps(anchor)+'\n')
    sampler = None
    with (case/'run.log').open('w') as log, (case/'sampler.log').open('w') as slog, (case/'host-activity.jsonl').open('w') as host:
        proc = subprocess.Popen([sys.executable, str(ROOT/'scripts/benchmark-service.py'), '--binary', str(binary), '--task-rows', str(a.task_rows), '--string-bytes', str(a.string_bytes), '--engine', engine, '--seconds', str(a.seconds), '--repeats', '1', '--output', str(case/'report.json')], env=env, stdout=log, stderr=log, start_new_session=True)
        started = time.monotonic()
        try:
            pid = None
            while time.monotonic()-started < 15 and proc.poll() is None:
                # Match the verified executable path; official binaries have no .tid probes.
                matches = [r['pid'] for r in processes() if r['executable'] == str(binary)]
                if len(matches) == 1:
                    pid = matches[0]
                    break
                time.sleep(.05)
            if pid is None:
                raise RuntimeError('No workload identity within 15 seconds')
            (case/'pid').write_text(str(pid)+'\n')
            if a.variant == 'diagnostic':
                native_sampler = out/'bin/native-sampler'
                assert digest(native_sampler) == manifest['binary_sha256']['native-sampler']
                native_sampler.chmod(0o755)
                sampler = subprocess.Popen([str(native_sampler), str(pid), str(min(a.seconds+15, 600)), str(case/'events'), str(case/'threads.jsonl')], stdout=slog, stderr=slog)
            previous_cpu = None
            while True:
                stamp = mono()
                ticks = cpu_ticks()
                row = {'monotonic_ns': stamp, 'unix_ns': time.time_ns(), 'cpu_ticks': ticks, 'loadavg': os.getloadavg(), 'processes': [r for r in processes() if r['cpu_percent'] >= 5 or Path(r['executable']).name in blocked]}
                if previous_cpu is not None:
                    row.update(interval_start_ns=previous_cpu[0], interval_busy_percent=interval_busy_percent(previous_cpu[1], ticks, os.cpu_count()))
                previous_cpu = (stamp, ticks)
                host.write(json.dumps(row)+'\n')
                host.flush()
                if proc.poll() is not None:
                    break
                if time.monotonic()-started > a.seconds+45:
                    raise TimeoutError('Workload completion exceeded bounded diagnostic duration')
                time.sleep(5)
            if proc.returncode:
                raise RuntimeError(f'Benchmark exited {proc.returncode}')
            if sampler is not None:
                sampler.wait(timeout=15)
                if sampler.returncode:
                    raise RuntimeError(f'Sampler exited {sampler.returncode}')
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGKILL)
                proc.wait()
            if sampler is not None and sampler.poll() is None:
                sampler.terminate()
                sampler.wait(timeout=10)
    if a.variant == 'diagnostic':
        run([sys.executable, str(HERE/'correlate.py'), str(case)])
    with (case/'gate.log').open('w') as gate:
        verdict = subprocess.run([sys.executable, str(ROOT/'scripts/check-load.py'), str(case/'report.json'), '--min-seconds', str(a.seconds), '--engine', engine], stdout=gate, stderr=gate).returncode
    verdicts.append({'engine': engine, 'original_gate_exit': verdict})
    (out/'verdicts.json').write_text(json.dumps(verdicts, indent=2)+'\n')
# A failed gate stays failed even though the diagnostic successfully collected it.
sys.exit(any(v['original_gate_exit'] for v in verdicts))
