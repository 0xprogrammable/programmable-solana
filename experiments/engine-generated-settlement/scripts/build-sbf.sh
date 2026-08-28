#!/bin/sh

set -eu

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly experiment_root="$(dirname -- "${script_dir}")"
readonly artifact_dir="${experiment_root}/target/deploy"
readonly build_dir="$(mktemp -d /tmp/programmable-generated-settlement-sbf.XXXXXX)"
readonly expected_build_sbf_version="solana-cargo-build-sbf 3.1.10"

cleanup() {
  rm -rf -- "${build_dir}"
}
trap cleanup EXIT HUP INT TERM

ensure_artifact_dir_is_pure() {
  unexpected_entry="$(
    find "${artifact_dir}" -mindepth 1 -maxdepth 1 \
      ! \( \
        -name 'programmable_generated_settlement_core.so' -o \
        -name 'generated_plan_engine.so' -o \
        -name 'opaque_capability_probe.so' \
      \) \
      -print -quit
  )"
  if [ -n "${unexpected_entry}" ]; then
    echo "Unexpected artifact entry: ${unexpected_entry}" >&2
    exit 1
  fi
}

cd "${experiment_root}"
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
  build_log="${build_dir}/$(basename "$(dirname "$1")").log"
  if [ "${PROGRAMMABLE_SBF_TOOLS_PINNED:-0}" = "1" ]; then
    if ! NO_DNA=1 cargo build-sbf \
      --arch v0 \
      --manifest-path "$1" \
      --sbf-out-dir "${build_dir}" \
      --skip-tools-install \
      --tools-version v1.52 \
      -- --locked > "${build_log}" 2>&1; then
      cat "${build_log}"
      exit 1
    fi
  else
    if ! NO_DNA=1 cargo build-sbf \
      --arch v0 \
      --manifest-path "$1" \
      --sbf-out-dir "${build_dir}" \
      --tools-version v1.52 \
      -- --locked > "${build_log}" 2>&1; then
      cat "${build_log}"
      exit 1
    fi
  fi
  cat "${build_log}"
  if grep -E 'Stack offset of .* exceeded max offset' "${build_log}" > /dev/null; then
    echo "SBF stack-frame limit exceeded while building $1" >&2
    exit 1
  fi
}

build_program programs/generated-settlement-core/Cargo.toml
build_program test-programs/generated-plan-engine/Cargo.toml
build_program test-programs/opaque-capability-probe/Cargo.toml

install -m 0644 \
  "${build_dir}/programmable_generated_settlement_core.so" \
  "${artifact_dir}/programmable_generated_settlement_core.so"
install -m 0644 \
  "${build_dir}/generated_plan_engine.so" \
  "${artifact_dir}/generated_plan_engine.so"
install -m 0644 \
  "${build_dir}/opaque_capability_probe.so" \
  "${artifact_dir}/opaque_capability_probe.so"

ensure_artifact_dir_is_pure
