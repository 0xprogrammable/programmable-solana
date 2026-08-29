# Protocol specification

This directory records the candidate and accepted Solana/SVM binding before it
is encoded in an onchain program. Portable Market, Domain, Engine, Effect,
Capability, Intent, fee, evidence, and major-version semantics belong to the
shared Programmable Protocol specification; this directory owns their
Solana-native PDA, account, CPI, SPL/Token-2022, SBF, event, and deployment
realization. It also contains explicitly private, disposable experiment
contracts and evidence; those records are not public interfaces or
compatibility promises.

## Documents

- [`protocol-boundaries.md`](protocol-boundaries.md) defines what the protocol
  does and does not control.
- [`developer-contract.md`](developer-contract.md) separates the candidate
  onchain contract, codecs, clients, and permissionless discovery metadata
  without accepting public ABI bytes.
- [`fee-constitution.md`](fee-constitution.md) defines which fee claims a
  generic Core can honestly enforce, fixes Production V1 at five basis points
  on `PrincipalFundedGrossDebitV1`, and defines the accounting gates required
  before its fee interface is accepted.
- [`engine-boundary-spike.md`](engine-boundary-spike.md) defines the smallest
  experiment that must succeed before a public engine ABI is designed.
- [`engine-generated-settlement-spike.md`](engine-generated-settlement-spike.md)
  defines a disposable, private experiment contract for engine-generated
  settlement. It is not a public engine ABI or compatibility promise.
- [`engine-generated-settlement-spike-results.md`](engine-generated-settlement-spike-results.md)
  records the executable result, measured capability boundary, and unresolved
  production gates.
- [`engine-generated-settlement-sbf-v0.sha256`](engine-generated-settlement-sbf-v0.sha256)
  pins the exact canonical Ubuntu binaries for that experiment.
- [`routed-callback-auth-spike.md`](routed-callback-auth-spike.md) defines the
  private experiment contract for exact routed intent authorization and
  phase-scoped Core-to-engine callback authentication. It is not a public ABI or
  deployment plan.
- [`routed-callback-auth-spike-results.md`](routed-callback-auth-spike-results.md)
  records the measured result, callback-shape decision, exact reproduction
  evidence, and unresolved production gates.
- [`routed-callback-auth-sbf-v0.sha256`](routed-callback-auth-sbf-v0.sha256)
  pins the four canonical Ubuntu binaries that CI must reproduce.
- [`generic-effect-capabilities-spike.md`](generic-effect-capabilities-spike.md)
  defines the next private falsification gate for a product-neutral protected
  effect graph and a separate opaque engine capability plane.
- [`authority-kernel-spike-results.md`](authority-kernel-spike-results.md)
  records the executable result and its non-production limits.
- [`authority-kernel-sbf-v0.sha256`](authority-kernel-sbf-v0.sha256) pins the
  exact canonical Ubuntu experiment binaries that CI must reproduce.
- [`maturity-checkpoint.md`](maturity-checkpoint.md) preserves the historical
  nine-category review of the first authority-kernel experiment.
- [`next-gate-maturity-checkpoint.md`](next-gate-maturity-checkpoint.md) applies
  the same framework to the complete three-experiment evidence before the
  generic effect/capability gate.
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
