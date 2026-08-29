#!/usr/bin/env python3

"""Verify the native repository's exact portable Protocol lock."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, NoReturn


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
LOCK_PATH = REPOSITORY_ROOT / "protocol" / "protocol-lock.json"
DEFAULT_PROTOCOL_ROOT = REPOSITORY_ROOT.parent / "PROGRAMMABLE-PROTOCOL"
ZERO_BYTE = b"\x00"

PINNED_REMOTE = "https://github.com/programmablehq/PROGRAMMABLE-PROTOCOL.git"
PINNED_COMMIT = "334bb26703a4dab18ce0fca8485c6275a879933a"
PINNED_TREE = "a0c4d7018eb810c35ac11cdd4e066cd92a6ee513"
PINNED_SPEC_ID = "programmable-protocol/0.1.0-draft.1"
PINNED_STATUS = "draft"
PINNED_PRODUCTION_ELIGIBLE = False
PINNED_ISSUE_URL = (
    "https://github.com/programmablehq/PROGRAMMABLE-PROTOCOL/issues/1"
)

PINNED_ALGORITHMS = {
    "constitution_id_v1": {
        "canonicalization": "RFC8785-JCS",
        "hash": "SHA-256",
        "domain_prefix": "programmable:constitution:v1",
        "separator_hex": "00",
        "excluded_fields": ["constitution_id"],
    },
    "json_artifact_digest_v1": {
        "canonicalization": "RFC8785-JCS",
        "hash": "SHA-256",
        "domain_prefix": "programmable:json-artifact:v1",
        "separator_hex": "00",
        "schema_source": "$schema",
    },
    "vector_set_digest_v1": {
        "canonicalization": "RFC8785-JCS",
        "hash": "SHA-256",
        "domain_prefix": "programmable:vector-set:v1",
        "separator_hex": "00",
        "ordering": "ascending_unicode_code_point_path",
    },
}

PINNED_RELEASE = {
    "inventory_path": "protocol-version.json",
    "inventory_raw_sha256": (
        "sha256:698b2877d3da69d1fe13e2eb8c1e47718559d2d6c943804842069fe4ba15aae5"
    ),
    "inventory_schema_id": "urn:programmable:schema:protocol-release:v1",
    "protocol_spec_id": PINNED_SPEC_ID,
    "status": PINNED_STATUS,
    "production_eligible": PINNED_PRODUCTION_ELIGIBLE,
    "constitution_document": "constitution/programmable-constitution-v1.json",
}

PINNED_CONSTITUTION = {
    "path": "constitution/programmable-constitution-v1.json",
    "schema_id": "urn:programmable:schema:constitution:v1",
    "constitution_id_v1": (
        "sha256:2715d9770de7b327c054c413a99f7cbba0933f2eabc9639a53948706237cd301"
    ),
    "raw_sha256": (
        "sha256:1d7e8863923f8fa47b942b4f6ef2dfd978e9788ad7975888015097d824d80e5d"
    ),
    "json_artifact_digest_v1": (
        "sha256:667a4dbe4d1c011075196fd6e0c56f968b3fc11299c64ebc2f29b4c26872e308"
    ),
}

PINNED_VECTOR_SET_DIGEST = (
    "sha256:d61a757f8d4c14d3e5ab0f92e77ab39bd54e7a91f4cc5d591819c58768481137"
)

PINNED_VECTOR_ARTIFACTS = [
    {
        "path": "vectors/bounded-session-v1.json",
        "schema_id": "urn:programmable:schema:bounded-session-vectors:v1",
        "raw_sha256": (
            "sha256:58536772b4caa130e6d4a41c723f85af7f2b01177c25bfe3fedeb3aa35e62165"
        ),
        "json_artifact_digest_v1": (
            "sha256:141068e3eb851a928df84309286fd51ae4f6838b1b3f3324d92c9ce282d9ac22"
        ),
    },
    {
        "path": "vectors/canonical-identifiers-v1.json",
        "schema_id": "urn:programmable:schema:canonical-identifiers-vectors:v1",
        "raw_sha256": (
            "sha256:650d3bfaa2935328a959b8a6ff620708d7297cf8813d0cc665e47fe342e8aa89"
        ),
        "json_artifact_digest_v1": (
            "sha256:f56fed4e24b368485d90130a92c9a5ec73f1f15f33b8229937746b91fed53735"
        ),
    },
    {
        "path": "vectors/evidence-v1.json",
        "schema_id": "urn:programmable:schema:evidence-vectors:v1",
        "raw_sha256": (
            "sha256:08bd3bd57fe7efe1655eaa91d10e7140915ca1ce7aa0d87b1017ebc4bcd72e80"
        ),
        "json_artifact_digest_v1": (
            "sha256:bac215a0490021081eb75b61e1d7cfc6611b2ed4256d119449d0cd5e5f78bb80"
        ),
    },
    {
        "path": "vectors/identifiers-v1.json",
        "schema_id": "urn:programmable:schema:identifier-vectors:v1",
        "raw_sha256": (
            "sha256:e2193dbb48d44e8e8de54eadf5e49b80595db595ef1272d7930842b753573044"
        ),
        "json_artifact_digest_v1": (
            "sha256:88dc64be6ee1a332f44506accd000fb7557e7843d942a10d300bf6111e9133cb"
        ),
    },
    {
        "path": "vectors/protected-effects-v1.json",
        "schema_id": "urn:programmable:schema:protected-effects-vectors:v1",
        "raw_sha256": (
            "sha256:f3ff6ce86a33eff5dd0a83cc0cbbac72a50b260f9382dee23dc0c0be775ad635"
        ),
        "json_artifact_digest_v1": (
            "sha256:46acc94498d89833d3383cf57482a8c7e93d9fe6ed7ce98942b54a62ba8507e8"
        ),
    },
    {
        "path": "vectors/protocol-assessment-v1.json",
        "schema_id": "urn:programmable:schema:protocol-assessment-vectors:v1",
        "raw_sha256": (
            "sha256:3a9e28ed4fdf859406a009b0e5d1f2eb88d44103001015b7ec7c4161561c2d1e"
        ),
        "json_artifact_digest_v1": (
            "sha256:8e0064bc5cc212c7c202e24dce0989345f9bc26b7498931f4c14b00728a30d02"
        ),
    },
]

PINNED_BLOCKER = {
    "terminal_state": "BLOCKED_BY_SPEC",
    "issue_url": PINNED_ISSUE_URL,
    "issue_number": "1",
    "issue_title": "Resolve identifier vector profile contradiction in CONF-006",
    "normative_requirement": "CONF-006",
    "counterexample_case_id": "identifier.batch_auction",
    "affected_vector": "vectors/identifiers-v1.json",
    "affected_schema": "schemas/identifier-v1-vectors.schema.json",
    "affected_release_scope": "portable_conformance_and_release_artifact_chain",
}

SAFE_REPOSITORY_PATH = re.compile(
    r"^(?!(?:\.{1,2})(?:/|$))[A-Za-z0-9._-]+"
    r"(?:/(?!(?:\.{1,2})(?:/|$))[A-Za-z0-9._-]+)*$"
)
LOWER_HEX_40 = re.compile(r"^[0-9a-f]{40}$")


class VerificationError(Exception):
    """A deterministic lock verification failure."""


def reject_json_number(value: str) -> NoReturn:
    raise VerificationError(f"portable JSON number is prohibited: {value}")


def reject_json_constant(value: str) -> NoReturn:
    raise VerificationError(f"non-finite JSON value is prohibited: {value}")


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    document: dict[str, Any] = {}
    for key, value in pairs:
        if key in document:
            raise VerificationError(f"duplicate JSON object key: {key}")
        document[key] = value
    return document


def validate_unicode_scalar(value: str, location: str) -> None:
    for character in value:
        if 0xD800 <= ord(character) <= 0xDFFF:
            raise VerificationError(f"{location}: unpaired UTF-16 surrogate")


def validate_portable_value(value: Any, location: str = "$") -> None:
    if value is None or type(value) is bool:
        return
    if isinstance(value, str):
        validate_unicode_scalar(value, location)
        return
    if isinstance(value, list):
        for index, child in enumerate(value):
            validate_portable_value(child, f"{location}[{index}]")
        return
    if isinstance(value, dict):
        for key, child in value.items():
            if not key.isascii():
                raise VerificationError(f"{location}: non-ASCII object key {key!r}")
            validate_unicode_scalar(key, f"{location}.<key>")
            validate_portable_value(child, f"{location}.{key}")
        return
    raise VerificationError(f"{location}: non-portable JSON value {value!r}")


def parse_json(source: bytes, location: str, *, portable: bool) -> Any:
    try:
        text = source.decode("utf-8", errors="strict")
        options: dict[str, Any] = {
            "object_pairs_hook": unique_object,
            "parse_constant": reject_json_constant,
        }
        if portable:
            options.update(
                parse_int=reject_json_number,
                parse_float=reject_json_number,
            )
        document = json.loads(text, **options)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise VerificationError(f"{location}: invalid JSON: {error}") from error
    if portable:
        validate_portable_value(document, location)
    return document


def jcs(value: Any, location: str) -> bytes:
    validate_portable_value(value, location)
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
    except (TypeError, UnicodeEncodeError) as error:
        raise VerificationError(f"{location}: JCS serialization failed: {error}") from error


def sha256_digest(*parts: bytes | str) -> str:
    digest = hashlib.sha256()
    for part in parts:
        digest.update(part if isinstance(part, bytes) else part.encode("utf-8"))
    return f"sha256:{digest.hexdigest()}"


def raw_sha256(source: bytes) -> str:
    return sha256_digest(source)


def require_equal(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        raise VerificationError(
            f"{label} mismatch: expected {expected!r}, found {actual!r}"
        )


def require_mapping(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise VerificationError(f"{label} must be a JSON object")
    return value


def require_exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require_equal(set(value), expected, f"{label} keys")


def validate_lock_pins(lock: Any) -> dict[str, Any]:
    document = require_mapping(lock, "protocol lock")
    require_exact_keys(
        document,
        {
            "format_id",
            "source",
            "algorithms",
            "release",
            "constitution",
            "portable_vectors",
            "blocked_by_spec",
        },
        "protocol lock",
    )
    require_equal(
        document["format_id"],
        "programmable-svm-protocol-lock/v1",
        "protocol lock format_id",
    )

    source = require_mapping(document["source"], "protocol lock source")
    require_exact_keys(
        source,
        {"repository_remote", "commit", "tree", "read_policy"},
        "protocol lock source",
    )
    require_equal(source["repository_remote"], PINNED_REMOTE, "Protocol remote")
    require_equal(source["commit"], PINNED_COMMIT, "Protocol commit")
    require_equal(source["tree"], PINNED_TREE, "Protocol tree")
    require_equal(
        source["read_policy"],
        "exact_commit_objects_only",
        "Protocol read policy",
    )
    if not LOWER_HEX_40.fullmatch(source["commit"]):
        raise VerificationError("Protocol commit must be a literal lowercase 40-hex ID")

    require_equal(document["algorithms"], PINNED_ALGORITHMS, "digest algorithms")
    require_equal(document["release"], PINNED_RELEASE, "release lock")
    require_equal(document["constitution"], PINNED_CONSTITUTION, "Constitution lock")

    vectors = require_mapping(document["portable_vectors"], "portable_vectors")
    require_exact_keys(
        vectors,
        {"vector_set_digest_v1", "artifacts"},
        "portable_vectors",
    )
    require_equal(
        vectors["vector_set_digest_v1"],
        PINNED_VECTOR_SET_DIGEST,
        "portable VectorSetDigestV1",
    )
    require_equal(
        vectors["artifacts"],
        PINNED_VECTOR_ARTIFACTS,
        "portable vector artifact lock",
    )
    require_equal(document["blocked_by_spec"], PINNED_BLOCKER, "specification blocker")
    return document


def git(protocol_root: Path, *arguments: str, input_bytes: bytes | None = None) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(protocol_root), *arguments],
        input=input_bytes,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        error = result.stderr.decode("utf-8", errors="replace").strip()
        raise VerificationError(
            f"git -C {protocol_root} {' '.join(arguments)} failed "
            f"with exit {result.returncode}: {error}"
        )
    return result.stdout


def validate_repository_path(relative_path: str) -> None:
    if not SAFE_REPOSITORY_PATH.fullmatch(relative_path):
        raise VerificationError(f"invalid pinned repository path: {relative_path!r}")


def read_commit_blob(protocol_root: Path, commit: str, relative_path: str) -> bytes:
    validate_repository_path(relative_path)
    entry = git(protocol_root, "ls-tree", commit, "--", relative_path).decode(
        "utf-8", errors="strict"
    ).rstrip("\n")
    fields = entry.split(maxsplit=3)
    if len(fields) != 4:
        raise VerificationError(f"missing committed file: {relative_path}")
    mode, object_type, _object_id, listed_path = fields
    if mode not in {"100644", "100755"} or object_type != "blob":
        raise VerificationError(f"committed path is not a regular file: {relative_path}")
    require_equal(listed_path, relative_path, f"committed path {relative_path}")
    return git(protocol_root, "cat-file", "blob", f"{commit}:{relative_path}")


def verify_git_identity(protocol_root: Path, source: dict[str, Any]) -> None:
    if not protocol_root.is_dir():
        raise VerificationError(f"Protocol repository is not a directory: {protocol_root}")
    actual_root = Path(
        git(protocol_root, "rev-parse", "--show-toplevel")
        .decode("utf-8", errors="strict")
        .strip()
    ).resolve()
    require_equal(actual_root, protocol_root, "Protocol repository root")

    remote_values = (
        git(protocol_root, "config", "--get-all", "remote.origin.url")
        .decode("utf-8", errors="strict")
        .splitlines()
    )
    require_equal(remote_values, [source["repository_remote"]], "Protocol origin URL")

    commit = source["commit"]
    object_type = (
        git(protocol_root, "cat-file", "-t", commit)
        .decode("ascii", errors="strict")
        .strip()
    )
    require_equal(object_type, "commit", "pinned Protocol object type")
    commit_bytes = git(protocol_root, "cat-file", "commit", commit)
    reconstructed_commit = (
        git(
            protocol_root,
            "hash-object",
            "-t",
            "commit",
            "--stdin",
            input_bytes=commit_bytes,
        )
        .decode("ascii", errors="strict")
        .strip()
    )
    require_equal(reconstructed_commit, commit, "reconstructed Protocol commit")

    first_line = commit_bytes.splitlines()[0].decode("ascii", errors="strict")
    if not first_line.startswith("tree "):
        raise VerificationError("pinned Protocol commit has no tree header")
    require_equal(first_line.removeprefix("tree "), source["tree"], "Protocol tree")


def json_artifact_digest(document: dict[str, Any], location: str) -> str:
    schema_id = document.get("$schema")
    if not isinstance(schema_id, str):
        raise VerificationError(f"{location}: missing string $schema")
    return sha256_digest(
        "programmable:json-artifact:v1",
        ZERO_BYTE,
        schema_id,
        ZERO_BYTE,
        jcs(document, location),
    )


def verify_release(
    protocol_root: Path, commit: str, release_lock: dict[str, Any]
) -> dict[str, Any]:
    path = release_lock["inventory_path"]
    source = read_commit_blob(protocol_root, commit, path)
    require_equal(raw_sha256(source), release_lock["inventory_raw_sha256"], path)
    document = require_mapping(parse_json(source, path, portable=True), path)
    require_equal(document.get("$schema"), release_lock["inventory_schema_id"], f"{path} $schema")
    require_equal(document.get("protocol_spec_id"), PINNED_SPEC_ID, f"{path} protocol_spec_id")
    require_equal(document.get("status"), PINNED_STATUS, f"{path} status")
    require_equal(
        document.get("production_eligible"),
        PINNED_PRODUCTION_ELIGIBLE,
        f"{path} production_eligible",
    )
    require_equal(
        document.get("constitution_document"),
        release_lock["constitution_document"],
        f"{path} constitution_document",
    )
    expected_paths = [artifact["path"] for artifact in PINNED_VECTOR_ARTIFACTS]
    require_equal(
        document.get("conformance_vectors"),
        expected_paths,
        f"{path} conformance_vectors",
    )
    return document


def verify_constitution(
    protocol_root: Path,
    commit: str,
    release: dict[str, Any],
    constitution_lock: dict[str, Any],
) -> None:
    path = constitution_lock["path"]
    require_equal(path, release["constitution_document"], "selected Constitution path")
    source = read_commit_blob(protocol_root, commit, path)
    require_equal(raw_sha256(source), constitution_lock["raw_sha256"], f"{path} raw SHA-256")
    document = require_mapping(parse_json(source, path, portable=True), path)
    require_equal(document.get("$schema"), constitution_lock["schema_id"], f"{path} $schema")
    require_equal(document.get("protocol_spec_id"), PINNED_SPEC_ID, f"{path} protocol_spec_id")
    require_equal(document.get("status"), PINNED_STATUS, f"{path} status")
    require_equal(
        document.get("constitution_id"),
        constitution_lock["constitution_id_v1"],
        f"{path} embedded Constitution ID",
    )

    hashable_document = dict(document)
    del hashable_document["constitution_id"]
    computed_constitution_id = sha256_digest(
        "programmable:constitution:v1",
        ZERO_BYTE,
        jcs(hashable_document, f"{path} without constitution_id"),
    )
    require_equal(
        computed_constitution_id,
        constitution_lock["constitution_id_v1"],
        "ConstitutionIdV1",
    )
    require_equal(
        json_artifact_digest(document, path),
        constitution_lock["json_artifact_digest_v1"],
        "Constitution JsonArtifactDigestV1",
    )


def verify_vectors(
    protocol_root: Path,
    commit: str,
    release: dict[str, Any],
    vector_lock: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    records: list[dict[str, str]] = []
    documents: dict[str, dict[str, Any]] = {}
    artifacts = vector_lock["artifacts"]
    require_equal(
        release["conformance_vectors"],
        [artifact["path"] for artifact in artifacts],
        "release vector inventory",
    )

    for artifact in artifacts:
        path = artifact["path"]
        source = read_commit_blob(protocol_root, commit, path)
        require_equal(raw_sha256(source), artifact["raw_sha256"], f"{path} raw SHA-256")
        document = require_mapping(parse_json(source, path, portable=True), path)
        documents[path] = document
        require_equal(document.get("$schema"), artifact["schema_id"], f"{path} $schema")
        require_equal(document.get("protocol_spec_id"), PINNED_SPEC_ID, f"{path} protocol_spec_id")
        artifact_digest = json_artifact_digest(document, path)
        require_equal(
            artifact_digest,
            artifact["json_artifact_digest_v1"],
            f"{path} JsonArtifactDigestV1",
        )
        records.append(
            {
                "artifact_digest": artifact_digest,
                "path": path,
                "schema_id": artifact["schema_id"],
            }
        )

    records.sort(key=lambda record: record["path"])
    vector_set_digest = sha256_digest(
        "programmable:vector-set:v1",
        ZERO_BYTE,
        jcs(records, "portable vector-set manifest"),
    )
    require_equal(
        vector_set_digest,
        vector_lock["vector_set_digest_v1"],
        "VectorSetDigestV1",
    )
    return documents


def verify_specification_blocker(
    protocol_root: Path,
    commit: str,
    blocker: dict[str, Any],
    vector_documents: dict[str, dict[str, Any]],
) -> None:
    require_equal(blocker["issue_url"], PINNED_ISSUE_URL, "blocker issue URL")

    conformance_source = read_commit_blob(
        protocol_root, commit, "spec/07-conformance.md"
    ).decode("utf-8", errors="strict")
    required_text = (
        "**[CONF-006]** Every shared vector case MUST list the exact portable\n"
        "conformance profiles that make it applicable."
    )
    if required_text not in conformance_source:
        raise VerificationError("CONF-006 blocker premise is absent or changed")

    affected_vector = blocker["affected_vector"]
    vector = vector_documents[affected_vector]
    cases = vector.get("cases")
    if not isinstance(cases, list):
        raise VerificationError(f"{affected_vector}: missing cases array")
    counterexample = next(
        (
            case
            for case in cases
            if isinstance(case, dict)
            and case.get("case_id") == blocker["counterexample_case_id"]
        ),
        None,
    )
    if counterexample is None:
        raise VerificationError("pinned blocker counterexample is absent")
    if "required_profiles" in counterexample:
        raise VerificationError("pinned blocker counterexample no longer reproduces")

    schema_path = blocker["affected_schema"]
    schema_source = read_commit_blob(protocol_root, commit, schema_path)
    schema = require_mapping(parse_json(schema_source, schema_path, portable=False), schema_path)
    try:
        case_shape = schema["properties"]["cases"]["items"]
        case_properties = case_shape["properties"]
        required_fields = case_shape["required"]
    except (KeyError, TypeError) as error:
        raise VerificationError(f"{schema_path}: unexpected case schema shape") from error
    require_equal(
        case_shape.get("additionalProperties"),
        False,
        f"{schema_path} case additionalProperties",
    )
    if "required_profiles" in case_properties or "required_profiles" in required_fields:
        raise VerificationError("pinned blocker schema contradiction no longer reproduces")

    checker_source = read_commit_blob(protocol_root, commit, "tools/check.mjs").decode(
        "utf-8", errors="strict"
    )
    if "requiredProfiles: [CORE_PROFILE_ID]" not in checker_source:
        raise VerificationError("pinned checker-side profile inference is absent or changed")


def expect_self_test_failure(action: Any, label: str) -> None:
    try:
        action()
    except VerificationError:
        return
    raise VerificationError(f"self-test failed to reject {label}")


def run_self_tests(lock: dict[str, Any]) -> None:
    canonical = jcs(
        parse_json(b'{"b":"2","a":"1"}', "JCS self-test", portable=True),
        "JCS self-test",
    )
    require_equal(canonical, b'{"a":"1","b":"2"}', "JCS self-test")

    invalid_documents = [
        (b'{"a":"1","a":"2"}', "duplicate object key"),
        (b'{"a":1}', "JSON number"),
        ('{"\u00e9":"value"}'.encode("utf-8"), "non-ASCII object key"),
        (b'{"a":"\\ud800"}', "unpaired surrogate"),
    ]
    for source, label in invalid_documents:
        expect_self_test_failure(
            lambda source=source: parse_json(source, label, portable=True), label
        )

    dynamic_ref = copy.deepcopy(lock)
    dynamic_ref["source"]["commit"] = "main"
    expect_self_test_failure(
        lambda: validate_lock_pins(dynamic_ref), "a dynamic Protocol reference"
    )

    drifted_digest = copy.deepcopy(lock)
    drifted_digest["portable_vectors"]["vector_set_digest_v1"] = (
        f"sha256:{'0' * 64}"
    )
    expect_self_test_failure(
        lambda: validate_lock_pins(drifted_digest), "a drifted vector-set digest"
    )

    promoted_release = copy.deepcopy(lock)
    promoted_release["release"]["status"] = "final"
    promoted_release["release"]["production_eligible"] = True
    expect_self_test_failure(
        lambda: validate_lock_pins(promoted_release), "an unpinned release promotion"
    )


def protocol_root_from_environment() -> Path:
    configured = os.environ.get("PROGRAMMABLE_PROTOCOL_ROOT")
    return Path(configured).resolve() if configured else DEFAULT_PROTOCOL_ROOT.resolve()


def verify(protocol_root: Path) -> dict[str, Any]:
    try:
        lock_source = LOCK_PATH.read_bytes()
    except OSError as error:
        raise VerificationError(f"cannot read {LOCK_PATH}: {error}") from error
    lock = validate_lock_pins(parse_json(lock_source, str(LOCK_PATH), portable=True))
    source = lock["source"]
    verify_git_identity(protocol_root, source)
    release = verify_release(protocol_root, source["commit"], lock["release"])
    verify_constitution(
        protocol_root,
        source["commit"],
        release,
        lock["constitution"],
    )
    vector_documents = verify_vectors(
        protocol_root,
        source["commit"],
        release,
        lock["portable_vectors"],
    )
    verify_specification_blocker(
        protocol_root,
        source["commit"],
        lock["blocked_by_spec"],
        vector_documents,
    )
    return lock


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the exact Programmable Protocol commit and portable digests."
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run deterministic rejection-path tests before verifying the lock",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    protocol_root = protocol_root_from_environment()
    try:
        lock_source = LOCK_PATH.read_bytes()
        lock = validate_lock_pins(parse_json(lock_source, str(LOCK_PATH), portable=True))
        if arguments.self_test:
            run_self_tests(lock)
            print("Protocol lock verifier self-test passed.")
        verified = verify(protocol_root)
    except (OSError, VerificationError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print("Programmable Protocol lock verified.")
    print(f"  protocol root: {protocol_root}")
    print(f"  protocol commit: {verified['source']['commit']}")
    print(f"  protocol tree: {verified['source']['tree']}")
    print(f"  protocol spec: {verified['release']['protocol_spec_id']}")
    print(f"  status: {verified['release']['status']}")
    print(
        "  production eligible: "
        f"{str(verified['release']['production_eligible']).lower()}"
    )
    print(
        "  Constitution ID: "
        f"{verified['constitution']['constitution_id_v1']}"
    )
    print(
        "  portable vector set: "
        f"{verified['portable_vectors']['vector_set_digest_v1']}"
    )
    print(
        "  release blocker: "
        f"{verified['blocked_by_spec']['terminal_state']} "
        f"({verified['blocked_by_spec']['issue_url']})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
