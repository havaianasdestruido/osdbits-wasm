#!/usr/bin/env bash
# Bootstrap a Rust toolchain for osdbits-wasm *without* access to
# static.rust-lang.org / crates.io.
#
# Why this exists: the build environment used to develop osdbits-wasm
# only allows the npm registry (plus github.com/api.github.com).  The
# standard `rustup` installer and crates.io are unreachable there, so we
# assemble a normal Rust 1.88.0 toolchain out of airgap-friendly npm
# packages that ship the official rust-installer component trees
# (@rustbin/*).  The core crate has ZERO third-party dependencies, so no
# crate registry access is needed once the toolchain exists.
#
# On a normal machine with internet access you do NOT need this script:
#
#     rustup target add wasm32-unknown-unknown
#     rustup component add rustfmt clippy
#
# Both paths produce the same pinned toolchain (1.88.0); see
# rust-toolchain.toml for the toolchain the CI uses.
set -euo pipefail

TC="${OSDBITS_RUST_TOOLCHAIN:-$HOME/.cache/rust-toolchain}"
VERSION=1.88.0
HOST=x86_64-unknown-linux-gnu

if [ -x "$TC/bin/rustc" ] && [ -x "$TC/bin/cargo" ]; then
  echo "toolchain already present at $TC"
  "$TC/bin/rustc" --version
  exit 0
fi

if ! command -v npm >/dev/null 2>&1; then
  echo "error: npm is required to bootstrap the airgapped toolchain" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetching Rust $VERSION components from the npm registry..."
(
  cd "$TMP"
  npm init -y >/dev/null 2>&1
  npm install --no-audit --no-fund \
    "@rustbin/rustc-$VERSION-$HOST" \
    "@rustbin/cargo-$VERSION-$HOST" \
    "@rustbin/rust-std-$VERSION-$HOST" \
    "@rustbin/rust-std-$VERSION-wasm32-unknown-unknown" \
    "@rustbin/rustfmt-$VERSION-$HOST" \
    "@rustbin/clippy-$VERSION-$HOST"
)

mkdir -p "$TC/lib/rustlib"
for comp in rustc cargo rustfmt clippy; do
  pkg="$TMP/node_modules/@rustbin/$comp-$VERSION-$HOST"
  src="$(find "$pkg" -maxdepth 1 -mindepth 1 -type d | head -1)"
  cp -a "$src"/. "$TC"/
done
for target in "$HOST" wasm32-unknown-unknown; do
  pkg="$TMP/node_modules/@rustbin/rust-std-$VERSION-$target"
  src="$(find "$pkg" -maxdepth 1 -mindepth 1 -type d | head -1)"
  cp -a "$src/lib/rustlib/$target" "$TC/lib/rustlib/"
done

echo "toolchain assembled at $TC"
"$TC/bin/rustc" --version
"$TC/bin/cargo" --version
