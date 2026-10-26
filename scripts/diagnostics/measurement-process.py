#!/usr/bin/env python3
"""Run a measurement in an owned, temporary launchd job on a hosted Mac."""
import argparse
import json
import os
from pathlib import Path
import platform
import plistlib
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def policy(probe, destination):
    probe.chmod(0o755)
    result = subprocess.run([str(probe)], check=True, capture_output=True, text=True)
    rows = [json.loads(line) for line in result.stdout.splitlines()]
    destination.write_text(json.dumps(rows, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--child', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--timeout', type=int, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command or args.timeout <= 0:
        parser.error('a command and positive timeout are required')
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or platform.system() != 'Darwin'):
        raise SystemExit('Only disposable GitHub-hosted macOS jobs are supported')
    output = args.output.resolve()
    probe = args.probe.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.child:
        status = output / 'measurement-exit.json'
        code = 1
        try:
            policy(probe, output / 'process-policy-inside.json')
            code = subprocess.run(command, cwd=ROOT).returncode
        finally:
            temporary = status.with_suffix('.tmp')
            temporary.write_text(json.dumps({'exit_code': code}) + '\n')
            temporary.replace(status)
        return code

    policy(probe, output / 'process-policy-outside.json')
    status = output / 'measurement-exit.json'
    if status.exists():
        raise SystemExit('Refusing to reuse an already completed measurement directory')
    label = 'org.livequery.measurement.' + uuid.uuid4().hex
    domain = f'gui/{os.getuid()}'
    record = {'label': label, 'domain': domain, 'process_type': 'Interactive',
              'scope': 'Only this measurement and its children; no library QoS changes',
              'library_limits_changed': False}
    # Explicit allowlist: runner credentials and arbitrary environment variables
    # must never be copied into a launchd plist or saved artifact.
    names = ['PATH', 'HOME', 'TMPDIR', 'RUNNER_TEMP', 'RUNNER_ENVIRONMENT',
             'GITHUB_ACTIONS', 'ImageVersion', 'ImageOS']
    environment = {key: os.environ[key] for key in names if key in os.environ}
    with tempfile.TemporaryDirectory(prefix='livequery-measurement-',
                                     dir=os.environ['RUNNER_TEMP']) as directory:
        plist = Path(directory) / 'measurement.plist'
        plist.write_bytes(plistlib.dumps({
            'Label': label,
            'ProgramArguments': [sys.executable, str(Path(__file__).resolve()),
                                 '--child', *sys.argv[1:]],
            'WorkingDirectory': str(ROOT), 'EnvironmentVariables': environment,
            'ProcessType': 'Interactive', 'RunAtLoad': True,
            'StandardOutPath': str(output / 'measurement-stdout.txt'),
            'StandardErrorPath': str(output / 'measurement-stderr.txt'),
        }))
        attempted = False
        try:
            attempted = True
            result = subprocess.run(['launchctl', 'bootstrap', domain, str(plist)],
                                    capture_output=True, text=True)
            record['bootstrap_exit'] = result.returncode
            record['bootstrap_stderr'] = result.stderr
            if result.returncode:
                raise RuntimeError('Could not start owned measurement job')
            deadline = time.monotonic() + args.timeout
            while not status.exists():
                if time.monotonic() >= deadline:
                    raise TimeoutError('Owned measurement job exceeded its workflow time budget')
                time.sleep(.25)
            code = json.loads(status.read_text())['exit_code']
            record['measurement_exit'] = code
            return code
        finally:
            if attempted:
                result = subprocess.run(['launchctl', 'bootout', f'{domain}/{label}'],
                                        capture_output=True, text=True)
                record['bootout_exit'] = result.returncode
                record['bootout_stderr'] = result.stderr
                check = subprocess.run(['launchctl', 'print', f'{domain}/{label}'],
                                       capture_output=True, text=True)
                record['job_absent_after_cleanup'] = check.returncode != 0
            (output / 'measurement-launch.json').write_text(json.dumps(record, indent=2) + '\n')
            if attempted and not record['job_absent_after_cleanup']:
                raise RuntimeError('Owned measurement job remained after cleanup')


if __name__ == '__main__':
    sys.exit(main())
