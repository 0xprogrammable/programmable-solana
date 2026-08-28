#!/usr/bin/env bash

set -euo pipefail

readonly repository_root="$(git rev-parse --show-toplevel)"
cd "$repository_root"

readonly required_files=(
  ".github/CODEOWNERS"
  ".github/pull_request_template.md"
  ".github/workflows/repository.yml"
  "CHANGELOG.md"
  "CONTRIBUTING.md"
  "LICENSE"
  "NOTICE"
  "README.md"
  "SECURITY.md"
  "VERSIONING.md"
  "spec/README.md"
  "spec/protocol-boundaries.md"
  "spec/repository-boundaries.md"
  "spec/security-properties.md"
  "spec/threat-model.md"
)

for required_file in "${required_files[@]}"; do
  if [[ ! -f "$required_file" ]]; then
    echo "Missing required file: $required_file" >&2
    exit 1
  fi
done

while IFS= read -r -d '' tracked_file; do
  tracked_name="${tracked_file##*/}"
  case "$tracked_name" in
    .env | .env.* | *.key | *.pem | *-keypair.json | id.json)
      if [[ "$tracked_name" != ".env.example" ]]; then
        echo "Refusing tracked credential file: $tracked_file" >&2
        exit 1
      fi
      ;;
  esac
done < <(git ls-files -z)

python3 - <<'PY'
import json
import os
import subprocess
import sys


def is_solana_keypair(value: object) -> bool:
    return (
        isinstance(value, list)
        and len(value) == 64
        and all(type(byte) is int and 0 <= byte <= 255 for byte in value)
    )


tracked_paths = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
for raw_path in tracked_paths:
    if not raw_path:
        continue

    path = os.fsdecode(raw_path)
    if not path.lower().endswith(".json"):
        continue

    try:
        with open(path, "r", encoding="utf-8") as source:
            value = json.load(source)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        continue

    if is_solana_keypair(value):
        print(f"Refusing Solana keypair array: {path}", file=sys.stderr)
        sys.exit(1)
PY

readonly empty_tree="$(git hash-object -t tree /dev/null)"
git diff --check "$empty_tree" HEAD

if git grep -nI -E '^(<<<<<<<|=======|>>>>>>>)' -- .; then
  echo "Merge conflict marker found" >&2
  exit 1
fi

readonly local_home_prefix="/""Users/"
if git grep -nI -F "$local_home_prefix" -- .; then
  echo "Local absolute path found" >&2
  exit 1
fi

if ! grep -q 'Apache License' LICENSE; then
  echo "LICENSE does not contain the expected Apache text" >&2
  exit 1
fi

echo "Repository policy checks passed"
