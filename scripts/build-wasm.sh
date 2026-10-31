#!/usr/bin/env bash
# Build the simulation core for wasm32 and copy it next to the web app.
#
# Usage: bash scripts/build-wasm.sh [--debug]
# Requires the Rust toolchain pinned in rust-toolchain.toml (see
# scripts/bootstrap-rust.sh for installing it without rustup).
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found; run scripts/bootstrap-rust.sh first" >&2
  exit 1
fi

profile=release
cargo_flags=(--release)
if [[ "${1:-}" == "--debug" ]]; then
  profile=debug
  cargo_flags=()
fi

cargo build --offline --target wasm32-unknown-unknown "${cargo_flags[@]}" \
  --manifest-path crates/osdbits-core/Cargo.toml

out="crates/osdbits-core/target/wasm32-unknown-unknown/${profile}/osdbits_core.wasm"
mkdir -p web/public
cp "$out" web/public/osdbits_core.wasm

size=$(wc -c < web/public/osdbits_core.wasm)
echo "copied $out -> web/public/osdbits_core.wasm (${size} bytes)"
