#!/bin/sh

set -eu

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly experiment_root="$(dirname -- "${script_dir}")"

cd "${experiment_root}"
export NO_DNA=1

cargo fmt --all --check
cargo metadata --locked --no-deps --format-version 1 > /dev/null
cargo clippy --workspace --all-targets --locked -- -D warnings

if [ "${SKIP_SBF:-0}" = "1" ]; then
  cargo test --workspace --lib --locked
  cargo test -p generic-effect-private-wire --test frozen_vectors --locked
  cargo test -p generic-effect-private-wire --test properties --locked
  echo "Ran host-only unit, frozen-vector, and property tests; skipped every exact-SBF integration test because SKIP_SBF=1"
else
  ./scripts/build-sbf.sh
  cargo test --workspace --all-targets --locked
fi
