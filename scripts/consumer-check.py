#!/usr/bin/env python3
"""Check the public crate from a separate workspace, including macro diagnostics."""
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
subprocess.run(['cargo', 'run', '--locked', '--manifest-path', str(root / 'tests/consumer/Cargo.toml')], check=True)
cases = {
    'enum': ('#[derive(lq::Inspect)] enum Bad { A } fn main() {}', 'JsonSchema'),
    'tuple': ('#[derive(lq::Inspect)] struct Bad(u64); fn main() {}', 'Inspect requires named fields'),
    'attribute': ('#[derive(lq::Inspect)] struct Bad { #[inspect(secret)] id: u64 } fn main() {}', 'expected skip'),
    'non_send': ('#[derive(lq::Inspect)] struct Bad { #[inspect(skip)] value: std::rc::Rc<u64> } fn main() {}', 'cannot be'),
}
with tempfile.TemporaryDirectory(prefix='livequery-consumer-') as directory:
    directory = Path(directory)
    (directory / 'src').mkdir()
    (directory / 'Cargo.toml').write_text('[package]\nname="consumer-diagnostics"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nlq={package="livequery",path="' + str(root / 'crates/livequery') + '"}\n')
    for name, (source, diagnostic) in cases.items():
        (directory / 'src/main.rs').write_text(source)
        result = subprocess.run(['cargo', 'check', '--offline', '--manifest-path', str(directory / 'Cargo.toml')],
                                capture_output=True, text=True,
                                env={**os.environ, 'CARGO_TARGET_DIR': str(root / 'target/consumer-check')})
        assert result.returncode != 0, f'{name}: unexpectedly compiled'
        assert diagnostic in result.stderr, result.stderr
        print(f'compile-fail {name}: PASS')
