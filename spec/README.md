# Protocol specification

This directory records the public contract of Programmable Solana before that
contract is encoded in an onchain program.

## Documents

- [`protocol-boundaries.md`](protocol-boundaries.md) defines what the protocol
  does and does not control.
- [`engine-boundary-spike.md`](engine-boundary-spike.md) defines the smallest
  experiment that must succeed before a public engine ABI is designed.
- [`authority-kernel-spike-results.md`](authority-kernel-spike-results.md)
  records the executable result and its non-production limits.
- [`authority-kernel-sbf-v0.sha256`](authority-kernel-sbf-v0.sha256) pins the
  exact canonical Ubuntu experiment binaries that CI must reproduce.
- [`maturity-checkpoint.md`](maturity-checkpoint.md) applies a nine-category
  maturity review to the current disposable experiment.
- [`runtime-baseline.md`](runtime-baseline.md) pins the observed Solana limits
  and semantics that the candidate design must survive.
- [`competitive-baseline.md`](competitive-baseline.md) records the narrow,
  source-backed product gap against the reviewed Solana DEX interfaces.
- [`repository-boundaries.md`](repository-boundaries.md) maps code ownership,
  dependency direction, and future repository splits.
- [`security-properties.md`](security-properties.md) lists the properties the
  implementation and tests must establish.
- [`threat-model.md`](threat-model.md) names each trust boundary and the damage a
  compromised actor can cause.
- [`decisions/`](decisions/) contains architecture decisions that should not be
  changed implicitly in a code review.

## Status language

Every consequential design passes through four states:

1. **Draft** — alternatives and failure cases are still being evaluated.
2. **Accepted** — the interface and acceptance criteria are agreed.
3. **Implemented** — code and tests exist at a named commit.
4. **Verified** — the release artifact and, when applicable, the onchain program
   have been independently matched to that commit.

An accepted document is not evidence that the property is implemented. A
verified build proves artifact provenance, not protocol safety.

The words **must**, **must not**, **should**, and **may** are normative only in an
accepted specification. Draft sections describe requirements under evaluation.
