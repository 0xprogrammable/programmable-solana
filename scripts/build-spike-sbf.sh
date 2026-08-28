#!/bin/sh

set -eu

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly repository_root="$(dirname -- "${script_dir}")"
readonly artifact_dir="${repository_root}/target/deploy"
readonly build_dir="$(mktemp -d /tmp/programmable-spike-sbf.XXXXXX)"
readonly expected_build_sbf_version="solana-cargo-build-sbf 3.1.10"

cleanup() {
  rm -rf -- "${build_dir}"
}
trap cleanup EXIT HUP INT TERM

ensure_artifact_dir_is_pure() {
  unexpected_entry="$(
    find "${artifact_dir}" -mindepth 1 -maxdepth 1 \
      ! \( \
        -name 'programmable_core.so' -o \
        -name 'programmable_spike_engine.so' \
      \) \
      -print -quit
  )"
  if [ -n "${unexpected_entry}" ]; then
    echo "Unexpected artifact entry: ${unexpected_entry}" >&2
    exit 1
  fi
}

cd "${repository_root}"
mkdir -p "${artifact_dir}"

readonly actual_build_sbf_version="$(cargo build-sbf --version | sed -n '1p')"
if [ "${actual_build_sbf_version}" != "${expected_build_sbf_version}" ]; then
  echo "Expected ${expected_build_sbf_version}; found ${actual_build_sbf_version}" >&2
  exit 1
fi

if find "${artifact_dir}" -maxdepth 1 -name '*-keypair.json' -print -quit | grep -q .; then
  echo "Remove retained deployment keypairs before building" >&2
  exit 1
fi
ensure_artifact_dir_is_pure

build_program() {
  if [ "${PROGRAMMABLE_SBF_TOOLS_PINNED:-0}" = "1" ]; then
    NO_DNA=1 cargo build-sbf \
      --arch v0 \
      --manifest-path "$1" \
      --sbf-out-dir "${build_dir}" \
      --skip-tools-install \
      --tools-version v1.52 \
      -- --locked
  else
    NO_DNA=1 cargo build-sbf \
      --arch v0 \
      --manifest-path "$1" \
      --sbf-out-dir "${build_dir}" \
      --tools-version v1.52 \
      -- --locked
  fi
}

build_program programs/core/Cargo.toml
build_program test-programs/spike-engine/Cargo.toml

install -m 0644 "${build_dir}/programmable_core.so" "${artifact_dir}/programmable_core.so"
install -m 0644 \
  "${build_dir}/programmable_spike_engine.so" \
  "${artifact_dir}/programmable_spike_engine.so"

ensure_artifact_dir_is_pure
