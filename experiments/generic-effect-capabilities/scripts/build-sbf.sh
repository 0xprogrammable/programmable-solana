#!/bin/sh

set -eu

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly experiment_root="$(dirname -- "${script_dir}")"
mkdir -p "${experiment_root}/target"
readonly artifact_dir="${experiment_root}/target/deploy"
readonly expected_build_sbf_version="solana-cargo-build-sbf 3.1.10"

build_dir=""
publish_stage=""
publish_backup_root=""

cleanup() {
  if [ -n "${build_dir}" ] && [ -d "${build_dir}" ]; then
    rm -rf -- "${build_dir}"
  fi
  if [ -n "${publish_stage}" ] && [ -d "${publish_stage}" ]; then
    rm -rf -- "${publish_stage}"
  fi
  if [ -n "${publish_backup_root}" ] && \
    [ -d "${publish_backup_root}/deploy" ] && \
    [ ! -e "${artifact_dir}" ]
  then
    mv -- "${publish_backup_root}/deploy" "${artifact_dir}"
  fi
  if [ -n "${publish_backup_root}" ] && [ -d "${publish_backup_root}" ]; then
    rm -rf -- "${publish_backup_root}"
  fi
}
trap cleanup EXIT HUP INT TERM

if ! build_dir="$(mktemp -d /tmp/programmable-generic-effect-sbf.XXXXXX)"; then
  echo "Unable to create temporary SBF build directory" >&2
  exit 1
fi
if ! publish_stage="$(mktemp -d "${experiment_root}/target/.deploy-stage.XXXXXX")"; then
  echo "Unable to create temporary deploy-stage directory" >&2
  exit 1
fi
if ! publish_backup_root="$(mktemp -d "${experiment_root}/target/.deploy-backup.XXXXXX")"; then
  echo "Unable to create temporary deploy-backup directory" >&2
  exit 1
fi
if [ -z "${build_dir}" ] || [ ! -d "${build_dir}" ] || \
  [ -z "${publish_stage}" ] || [ ! -d "${publish_stage}" ] || \
  [ -z "${publish_backup_root}" ] || [ ! -d "${publish_backup_root}" ]
then
  echo "Invalid temporary SBF build or publish directory" >&2
  exit 1
fi
readonly build_dir publish_stage publish_backup_root

ensure_artifact_dir_is_pure() {
  checked_dir="$1"
  unexpected_entry="$(
    find "${checked_dir}" -mindepth 1 -maxdepth 1 \
      ! \( \
        -name 'programmable_generic_effect_core.so' -o \
        -name 'generic_effect_engine_probe.so' -o \
        -name 'replacement_effect_engine_probe.so' -o \
        -name 'hostile_router_probe.so' -o \
        -name 'callback_capability_probe.so' \
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
ensure_artifact_dir_is_pure "${artifact_dir}"

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
  if grep -E \
    'Stack offset of .* exceeded max offset|Estimated function frame .* exceeds maximum allowed stack offset|overwrites values in the frame' \
    "${build_log}" > /dev/null
  then
    echo "SBF stack-frame limit exceeded while building $1" >&2
    exit 1
  fi
}

build_program programs/generic-effect-core/Cargo.toml
build_program test-programs/effect-engine-probe/Cargo.toml
build_program test-programs/replacement-effect-engine-probe/Cargo.toml
build_program test-programs/hostile-router-probe/Cargo.toml
build_program test-programs/callback-capability-probe/Cargo.toml

"${script_dir}/check-sbf-undefined-symbols.sh" \
  "${build_dir}/programmable_generic_effect_core.so" \
  "${build_dir}/generic_effect_engine_probe.so" \
  "${build_dir}/replacement_effect_engine_probe.so" \
  "${build_dir}/hostile_router_probe.so" \
  "${build_dir}/callback_capability_probe.so"

for artifact in \
  programmable_generic_effect_core.so \
  generic_effect_engine_probe.so \
  replacement_effect_engine_probe.so \
  hostile_router_probe.so \
  callback_capability_probe.so
do
  test -f "${build_dir}/${artifact}"
  install -m 0644 "${build_dir}/${artifact}" "${publish_stage}/${artifact}"
done

ensure_artifact_dir_is_pure "${publish_stage}"
test "$(find "${publish_stage}" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')" = "5"

# Publish only a complete verified set. There is never a mixed old/new deploy
# directory: an interruption before the second rename leaves either the old
# set recoverable by the trap or no published set at all.
mv -- "${artifact_dir}" "${publish_backup_root}/deploy"
mv -- "${publish_stage}" "${artifact_dir}"
rm -rf -- "${publish_backup_root}/deploy"
ensure_artifact_dir_is_pure "${artifact_dir}"

for artifact in \
  programmable_generic_effect_core.so \
  generic_effect_engine_probe.so \
  replacement_effect_engine_probe.so \
  hostile_router_probe.so \
  callback_capability_probe.so
do
  shasum -a 256 "${artifact_dir}/${artifact}"
done
