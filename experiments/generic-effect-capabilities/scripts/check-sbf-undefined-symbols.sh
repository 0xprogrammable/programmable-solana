#!/bin/sh

set -eu

readonly script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
readonly expected_platform_tools_version="v1.52"
readonly expected_platform_tools_manifest_sha256="851c4d2be2cc6a20594232180aaefabf5b694d15ad67505b75d725fb5721327c"
readonly expected_rustc_version="rustc 1.89.0-dev"
readonly expected_llvm_version="LLVM version 20.1.7-rust-dev"
readonly allowlist="${script_dir}/sbpfv0-syscalls-agave-v3.1.10.txt"
readonly verifier="${script_dir}/verify-sbf-undefined-symbols.py"

if [ "$#" -eq 0 ]; then
  echo "Usage: $0 ARTIFACT.so [...]" >&2
  exit 2
fi

if [ -n "${SBF_SDK_PATH:-}" ]; then
  sbf_sdk="${SBF_SDK_PATH}"
else
  cargo_build_sbf="$(command -v cargo-build-sbf || true)"
  if [ -z "${cargo_build_sbf}" ]; then
    echo "Unable to locate cargo-build-sbf and its pinned SBF SDK" >&2
    exit 1
  fi
  sbf_sdk="$(dirname -- "${cargo_build_sbf}")/platform-tools-sdk/sbf"
fi
readonly sbf_sdk

readonly platform_tools_marker="${sbf_sdk}/dependencies/platform-tools-${expected_platform_tools_version}.md"
if [ ! -f "${platform_tools_marker}" ]; then
  echo "Expected pinned ${expected_platform_tools_version} marker: ${platform_tools_marker}" >&2
  exit 1
fi

platform_tools_link="${sbf_sdk}/dependencies/platform-tools"
if ! platform_tools="$(CDPATH= cd -- "${platform_tools_link}" && pwd -P)"; then
  echo "Unable to resolve active platform-tools directory: ${platform_tools_link}" >&2
  exit 1
fi
readonly platform_tools

readonly platform_tools_manifest="${platform_tools}/version.md"
if [ ! -f "${platform_tools_manifest}" ]; then
  echo "Missing platform-tools manifest: ${platform_tools_manifest}" >&2
  exit 1
fi
if command -v shasum > /dev/null 2>&1; then
  if ! manifest_hash_output="$(shasum -a 256 "${platform_tools_manifest}")"; then
    echo "Unable to hash ${platform_tools_manifest}" >&2
    exit 1
  fi
elif command -v sha256sum > /dev/null 2>&1; then
  if ! manifest_hash_output="$(sha256sum "${platform_tools_manifest}")"; then
    echo "Unable to hash ${platform_tools_manifest}" >&2
    exit 1
  fi
else
  echo "shasum or sha256sum is required to verify platform-tools" >&2
  exit 1
fi
manifest_hash="${manifest_hash_output%% *}"
if [ "${manifest_hash}" != "${expected_platform_tools_manifest_sha256}" ]; then
  echo "Expected platform-tools ${expected_platform_tools_version}; manifest hash ${manifest_hash} found at ${platform_tools}" >&2
  exit 1
fi

if [ ! -x "${platform_tools}/rust/bin/rustc" ]; then
  echo "Pinned rustc is unavailable under ${platform_tools}" >&2
  exit 1
fi
if ! actual_rustc_version="$("${platform_tools}/rust/bin/rustc" --version)"; then
  echo "Unable to read pinned rustc version under ${platform_tools}" >&2
  exit 1
fi
if [ "${actual_rustc_version}" != "${expected_rustc_version}" ]; then
  echo "Expected ${expected_rustc_version}; found ${actual_rustc_version} at ${platform_tools}" >&2
  exit 1
fi

if [ -x "${platform_tools}/llvm/bin/llvm-readelf" ]; then
  readelf="${platform_tools}/llvm/bin/llvm-readelf"
elif [ -x "${platform_tools}/llvm/bin/readelf" ]; then
  readelf="${platform_tools}/llvm/bin/readelf"
else
  echo "Pinned ${expected_platform_tools_version} llvm-readelf/readelf is unavailable under ${platform_tools}" >&2
  exit 1
fi
readonly readelf
if ! readelf_version="$("${readelf}" --version 2>&1)"; then
  echo "Unable to read pinned llvm-readelf/readelf version: ${readelf}" >&2
  exit 1
fi
case "${readelf_version}" in
  *"${expected_llvm_version}"*) ;;
  *)
    echo "Expected ${expected_llvm_version} at ${readelf}" >&2
    exit 1
    ;;
esac

if [ ! -s "${allowlist}" ]; then
  echo "Missing repository-owned SBPFv0 syscall allowlist: ${allowlist}" >&2
  exit 1
fi
if [ ! -f "${verifier}" ]; then
  echo "Missing undefined-symbol verifier: ${verifier}" >&2
  exit 1
fi
python3="$(command -v python3 || true)"
if [ -z "${python3}" ]; then
  echo "python3 is required for unambiguous llvm-readelf JSON parsing" >&2
  exit 1
fi
readonly python3

readonly vendor_syscalls="${sbf_sdk}/syscalls.txt"
if [ -f "${vendor_syscalls}" ] && [ ! -s "${vendor_syscalls}" ]; then
  echo "Note: ${vendor_syscalls} is empty; using ${allowlist}" >&2
fi

check_dir=""
cleanup() {
  if [ -n "${check_dir}" ] && [ -d "${check_dir}" ]; then
    rm -rf -- "${check_dir}"
  fi
}
trap cleanup EXIT HUP INT TERM
if ! check_dir="$(mktemp -d /tmp/programmable-sbf-symbol-check.XXXXXX)"; then
  echo "Unable to create temporary SBF symbol-check directory" >&2
  exit 1
fi
if [ -z "${check_dir}" ] || [ ! -d "${check_dir}" ]; then
  echo "Invalid temporary SBF symbol-check directory" >&2
  exit 1
fi
readonly check_dir

status=0
artifact_number=0
for artifact in "$@"; do
  artifact_number=$((artifact_number + 1))
  if [ ! -f "${artifact}" ]; then
    echo "Missing SBF artifact: ${artifact}" >&2
    status=1
    continue
  fi

  readelf_json="${check_dir}/readelf.${artifact_number}.json"
  readelf_error="${check_dir}/readelf.${artifact_number}.err"
  if ! LC_ALL=C "${readelf}" \
    --elf-output-style=JSON \
    --dyn-symbols \
    --wide \
    "${artifact}" > "${readelf_json}" 2> "${readelf_error}"
  then
    echo "Unable to read dynamic ELF symbols from ${artifact}:" >&2
    cat "${readelf_error}" >&2
    status=1
    continue
  fi
  if ! "${python3}" "${verifier}" "${allowlist}" "${artifact}" "${readelf_json}"; then
    status=1
  fi
done

exit "${status}"
