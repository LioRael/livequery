#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo check --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
python3 scripts/consumer-check.py
cargo build --workspace --bins --locked
python3 scripts/smoke.py
python3 scripts/jobs-smoke.py
cargo package -p livequery-core -p livequery-derive -p livequery-query -p livequery-transport -p livequery-agent -p livequery -p livequery-cli --locked --allow-dirty
