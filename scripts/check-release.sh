#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
python3 scripts/consumer-check.py
cargo build --workspace --examples --bins --locked
python3 scripts/smoke.py
cargo package --workspace --locked --allow-dirty
