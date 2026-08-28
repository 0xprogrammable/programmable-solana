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
)

for required_file in "${required_files[@]}"; do
  if [[ ! -f "$required_file" ]]; then
    echo "Missing required file: $required_file" >&2
    exit 1
  fi
done

while IFS= read -r tracked_file; do
  case "$tracked_file" in
    .env | .env.* | *.key | *.pem | *-keypair.json | id.json)
      if [[ "$tracked_file" != ".env.example" ]]; then
        echo "Refusing tracked credential file: $tracked_file" >&2
        exit 1
      fi
      ;;
  esac
done < <(git ls-files)

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
