#!/usr/bin/env python3
"""Disable observed indexing agents only in a disposable GitHub-hosted VM."""
import json
import os
from pathlib import Path
import platform
import plistlib
import subprocess


def main():
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or platform.system() != 'Darwin'):
        raise SystemExit('Refusing to change services outside a GitHub-hosted macOS job')
    output = Path('target/ci-offcpu/service-isolation.json')
    output.parent.mkdir(parents=True, exist_ok=True)
    records = []
    failed = False
    for label in ['com.apple.corespotlightd', 'com.apple.helpd']:
        plist = Path('/System/Library/LaunchAgents') / (label + '.plist')
        with plist.open('rb') as f:
            assert plistlib.load(f)['Label'] == label
        for domain in [f'gui/{os.getuid()}', f'user/{os.getuid()}']:
            target = f'{domain}/{label}'
            before = subprocess.run(['launchctl', 'print', target], capture_output=True, text=True)
            record = {'target': target, 'before_exit': before.returncode}
            if before.returncode == 0:
                actions = []
                for action in ['disable', 'bootout']:
                    result = subprocess.run(['launchctl', action, target], capture_output=True, text=True)
                    actions.append({'action': action, 'exit': result.returncode, 'stderr': result.stderr})
                after = subprocess.run(['launchctl', 'print', target], capture_output=True, text=True)
                record.update(actions=actions, after_exit=after.returncode)
                failed |= any(a['exit'] for a in actions) or after.returncode == 0
            else:
                record['stderr'] = before.stderr
            records.append(record)
    output.write_text(json.dumps({'agents': records, 'configuration_failed': bool(failed)}, indent=2)+'\n')
    if failed:
        raise SystemExit('Service isolation refused or ineffective; do not bypass OS protection')


if __name__ == '__main__':
    main()
