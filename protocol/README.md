# Portable Protocol lock

This directory records the exact portable Protocol input selected by the native
Solana implementation. The machine-readable source of truth is
[`protocol-lock.json`](protocol-lock.json).

## Selected release

- Repository: `https://github.com/programmablehq/PROGRAMMABLE-PROTOCOL.git`
- Commit: `334bb26703a4dab18ce0fca8485c6275a879933a`
- Tree: `a0c4d7018eb810c35ac11cdd4e066cd92a6ee513`
- Protocol Spec ID: `programmable-protocol/0.1.0-draft.1`
- Status: `draft`
- Production eligible: `false`
- Constitution ID: `sha256:2715d9770de7b327c054c413a99f7cbba0933f2eabc9639a53948706237cd301`
- Portable vector-set digest: `sha256:d61a757f8d4c14d3e5ab0f92e77ab39bd54e7a91f4cc5d591819c58768481137`

The Constitution ID is the domain-separated `ConstitutionIdV1` value. It is not
the SHA-256 digest of the pretty-printed source bytes. The lock records that
semantic ID, the raw committed-file SHA-256 digest, and the full-document
`JsonArtifactDigestV1` independently.

The release is an implementation baseline, not Production evidence. A matching
lock proves deterministic portable artifact identity only. It does not prove
native conformance, a reproducible Solana Bytecode Format artifact, deployment,
live authority state, or production eligibility.

## Verification

Run the verifier from the native repository root:

```bash
python3 scripts/verify-protocol-lock.py
python3 scripts/verify-protocol-lock.py --self-test
```

The script resolves native artifacts relative to its own path, so it also works
from another directory when invoked by an absolute path.

By default it reads the sibling `PROGRAMMABLE-PROTOCOL` repository. An explicit
checkout may be selected without changing the lock:

```bash
PROGRAMMABLE_PROTOCOL_ROOT=/absolute/path/to/PROGRAMMABLE-PROTOCOL \
  python3 scripts/verify-protocol-lock.py
```

The verifier accepts only the pinned lowercase 40-hex commit and reads each
source with `git -C <root> cat-file`. It does not read the Protocol working tree,
`HEAD`, `main`, `origin/main`, or another dynamic reference. The selected commit
may remain verifiable after a branch advances, provided the exact commit object
is still present locally.

The verifier checks the repository remote, commit object identity, tree, release
inventory, release status, production gate, Constitution identities, every raw
vector digest, every `JsonArtifactDigestV1`, the sorted `VectorSetDigestV1`, and
the pinned specification blocker metadata. Any mismatch exits nonzero.
The issue URL is a pinned coordination reference; verification does not treat
mutable GitHub issue state as Protocol evidence or require network access.

## Specification blocker

Formal portable conformance and the release artifact chain are currently
[`BLOCKED_BY_SPEC`](BLOCKED_BY_SPEC-001.md). The coordinating Protocol issue is
[PROGRAMMABLE-PROTOCOL issue 1](https://github.com/programmablehq/PROGRAMMABLE-PROTOCOL/issues/1).
Unaffected native architecture, implementation, and testing may continue, but
the native repository must not invent a private applicability rule or rewrite
the pinned portable artifacts.
