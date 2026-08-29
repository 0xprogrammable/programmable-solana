#!/usr/bin/env python3

import json
import re
import sys
from pathlib import Path


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


if len(sys.argv) != 4:
    fail(f"Usage: {sys.argv[0]} ALLOWLIST ARTIFACT READELF_JSON")

allowlist_path = Path(sys.argv[1])
artifact = sys.argv[2]
readelf_json_path = Path(sys.argv[3])

allowed: set[str] = set()
for line_number, raw_line in enumerate(
    allowlist_path.read_text(encoding="utf-8").splitlines(), start=1
):
    line = raw_line.strip()
    if not line or line.startswith("#"):
        continue
    if not re.fullmatch(r"(?:abort|sol_[A-Za-z0-9_]+)", line):
        fail(f"Malformed syscall at {allowlist_path}:{line_number}")
    if line in allowed:
        fail(f"Duplicate syscall at {allowlist_path}:{line_number}: {line}")
    allowed.add(line)

if not allowed:
    fail(f"Empty SBPFv0 syscall allowlist: {allowlist_path}")

try:
    document = json.loads(readelf_json_path.read_text(encoding="utf-8"))
except (OSError, UnicodeError, json.JSONDecodeError) as error:
    fail(f"Invalid llvm-readelf JSON for {artifact}: {error}")

if not isinstance(document, list) or len(document) != 1 or not isinstance(document[0], dict):
    fail(f"Unexpected llvm-readelf document for {artifact}")

file_document = document[0]
summary = file_document.get("FileSummary")
if not isinstance(summary, dict):
    fail(f"Missing ELF file summary for {artifact}")
if summary.get("Format") != "elf64-sbf" or summary.get("Arch") != "sbf":
    fail(f"Expected an ELF64 SBF artifact: {artifact}")

dynamic_symbols = file_document.get("DynamicSymbols")
if not isinstance(dynamic_symbols, list) or not dynamic_symbols:
    fail(f"Missing dynamic ELF symbol table in {artifact}")

undefined: set[str] = set()
for index, entry in enumerate(dynamic_symbols):
    try:
        symbol = entry["Symbol"]
        name = symbol["Name"]["Name"]
        section = symbol["Section"]["Name"]
        binding = symbol["Binding"]["Name"]
    except (KeyError, TypeError):
        fail(f"Malformed dynamic symbol {index} in {artifact}")

    if not all(isinstance(value, str) for value in (name, section, binding)):
        fail(f"Malformed dynamic symbol {index} in {artifact}")
    if section != "Undefined":
        continue
    if not name:
        if index == 0 and binding == "Local":
            continue
        fail(f"Unnamed undefined dynamic symbol {index} in {artifact}")
    undefined.add(name)

foreign = sorted(undefined - allowed)
if foreign:
    print(f"Foreign undefined symbols in {artifact}:", file=sys.stderr)
    for symbol in foreign:
        print(f"  {json.dumps(symbol, ensure_ascii=True)}", file=sys.stderr)
    raise SystemExit(1)
