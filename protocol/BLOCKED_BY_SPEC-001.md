# BLOCKED_BY_SPEC-001: identifier vector profile applicability

- Terminal state: `BLOCKED_BY_SPEC`
- Protocol commit: `334bb26703a4dab18ce0fca8485c6275a879933a`
- Normative requirement: `CONF-006`
- Counterexample: `identifier.batch_auction`
- Tracking issue: [PROGRAMMABLE-PROTOCOL issue 1](https://github.com/programmablehq/PROGRAMMABLE-PROTOCOL/issues/1)

## Contradiction

At the pinned commit, `spec/07-conformance.md:65-68` requires every shared vector
case to list the exact portable conformance profiles that make it applicable.
The eight cases in `vectors/identifiers-v1.json:10-58` do not contain a
`required_profiles` field.

The corresponding shape in
`schemas/identifier-v1-vectors.schema.json:25-42` sets
`additionalProperties` to `false` and neither defines nor requires
`required_profiles`. Adding the field demanded by `CONF-006` therefore makes the
case schema-invalid. `tools/check.mjs:1138-1144` supplies
`["portable-core-v1"]` out of band, which lets the Protocol repository check
pass but does not repair the committed vector or override the numbered normative
specification.

The smallest counterexample is:

```json
{
  "case_id": "identifier.batch_auction",
  "purpose": "Locks the portable identity of the batch-auction example.",
  "document_path": "examples/batch-auction.json",
  "expected_id": "sha256:84d1dfe684b8bfbaa91717c25a56a5db797e842ea8f517cec8c4ec7f6e03aa9a"
}
```

It provides no normative profile-applicability value. A native Conformance
Report generator would have to infer one privately or lack the information
needed to apply `CONF-005` and `CONF-006` exactly.

## Impact boundary

This contradiction blocks a formally conforming portable Conformance Report,
Binding Release, and any deployment claim that must resolve that release chain.
It does not prevent unaffected native architecture, implementation, local
testing, or an explicitly disposable non-production deployment from continuing.

Production is independently unavailable because the same pinned release has
`status: "draft"` and `production_eligible: false`. Promotion requires a new
exact Protocol commit whose release inventory says `final` and `true`.

Resolution belongs in the portable Protocol repository. The native Solana
implementation must not patch, reinterpret, or silently replace the selected
portable semantics. Any repair that changes a listed vector also changes
`VectorSetDigestV1` and requires a new exact lock.
