# Code maturity checkpoint

Status: Historical draft assessment of the first authority-kernel experiment

Date: 2026-08-28

This checkpoint applies the Trail of Bits nine-category maturity model only to
the first authority-kernel experiment. It predates the engine-generated and
routed-callback experiments, is preserved as historical evidence, and must not
be read as the current repository score. The current next-gate assessment is in
[`next-gate-maturity-checkpoint.md`](next-gate-maturity-checkpoint.md). It is not
a security audit and must not be generalized to a future DEX implementation.

## Summary

The experiment is mature enough to answer its narrow authority question but is
weak as a production protocol. Its strongest properties are explicit authority
separation, checked fee arithmetic, a small state surface, and executable
hostile-program tests. Its largest gaps are the intentionally absent exit path,
unbound engine code identity, production monitoring and incident response,
ordering analysis, fuzzing, and release evidence.

| Category | Rating | Score | Evidence and limiting gap |
| --- | --- | ---: | --- |
| Arithmetic | Moderate | 2 | Checked `u128` fee math, explicit ceiling, exact delta checks, and edge tests; no property fuzzing or differential model. |
| Auditing | Weak | 1 | Typed events exist, but no production indexer, alert policy, incident runbook, or exercised response process exists. |
| Authentication and access control | Moderate | 2 | Fixed two-account engine closure, no forwarded signer, immutable domain relation, direct-call authentication, and hostile escalation test; loader-backed engine identity is deferred. |
| Complexity management | Satisfactory | 3 | Three instructions, one direction, no product enum, no global registry, and isolated codec/math/validation modules; execute remains the largest review surface. |
| Decentralization | Weak | 1 | No global administrator or offchain signer exists, but custody has no exit and engine upgrade trust is not bound. |
| Documentation | Satisfactory | 3 | Architecture, threats, runtime, security properties, limitations, and executable result are documented; no end-user or accepted ABI documentation exists. |
| Transaction ordering | Weak | 1 | Exact minimum output, maximum debit, fee ceiling, and expiry exist; MEV, replay across richer intents, auctions, and ordering interactions are not modeled. |
| Low-level manipulation | Moderate | 2 | Manual fixed-width codec and manual CPI are justified, fail closed, and tested; no unsafe Rust or assembly exists, but no differential fuzzing covers the codec. |
| Testing and verification | Moderate | 2 | Unit, real-SPL integration, CPI-tree, rollback, resource, formatting, clippy, lockfile, and CI gates exist; coverage, mutation, fuzz, Surfpool, devnet, and formal checks do not. |

**Overall:** 17/36, or 1.9/4.0. Under the framework's fail-low rule,
production maturity is **Weak** because several required production controls are
absent. That is compatible with the repository's explicit experiment status.

## Priority gates

### Critical before any custody deployment

1. Choose and prove an exit class: no persistent Core custody or an exact,
   engine-independent claim path.
2. Bind engine code and upgrade behavior with loader-aware evidence instead of a
   numeric revision label.
3. Complete the callback and authorization-neutral stored/multi-intent
   experiments before naming a public engine interface.

### High before accepting a protocol major

1. Add property-based and stateful tests for accounting, fee, alias, expiry,
   and rollback invariants.
2. Run the exact artifacts through an embedded current-runtime or forked
   scenario and then a separately reviewed devnet smoke release.
3. Re-pin and re-run the exact suite for the active SBF/SBPF compiler generation
   before treating a toolchain migration as compatible.
4. Specify monitoring events, independent indexer verification, incident
   response, deployment authority, reproducible artifacts, and an append-only
   release manifest.
5. Reproduce the canonical artifact with at least two independent builders in
   the same hermetic release environment; local cross-OS hashes currently
   differ.
6. Model ordering and MEV explicitly for each accepted engine/intent class.

### Separate experiments, not Probe V0 additions

1. Shared liquidity-domain admission.
2. Token-2022 extension classes and callback graphs.
3. External settlement drivers and custom protected assets.
4. Bidirectional settlement, positions, partial fills, and asynchronous intent
   lifecycles.

Keeping those experiments separate preserves failure attribution and prevents a
temporary wire format from becoming accidental protocol law.
