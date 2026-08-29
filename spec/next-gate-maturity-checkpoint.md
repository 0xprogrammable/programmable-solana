# Next-gate code maturity checkpoint

Status: Draft evidence checkpoint for the next private architecture gate

Date: 2026-08-28

This checkpoint applies the Trail of Bits nine-category code-maturity framework
to the combined evidence from the authority-kernel, engine-generated-settlement,
and routed-callback-authentication experiments. Its target is promotion into the
next generic effect/capability experiment, not the narrow question answered by
any one disposable fixture.

This is not a security audit, an audit opinion, a release assessment, an accepted
public ABI, a deployment authorization, or a statement about product status. A
passing test proves only the exact code, runtime, inputs, and adversarial case
that the test exercised.

## Evidence identity and scoring rule

The code baseline is repository commit
`4b43d000c6ba032c9369f0514e7a1f7b1e4a9960`, tree
`02fb36e7dbfc25b83b07f7c062139942c0acabe8`. GitHub Actions run
[`33197777169`](https://github.com/0xprogrammable/programmable-solana/actions/runs/33197777169)
rebuilt that commit and passed repository policy, all three experiment jobs, the
checksum gates, and the aggregate protocol gate. The workflow makes each
experiment and the aggregate dependency explicit at
`.github/workflows/repository.yml:16-30`,
`.github/workflows/repository.yml:84-107`,
`.github/workflows/repository.yml:163-191`, and
`.github/workflows/repository.yml:247-299`.

The run logs report 26 root-workspace tests, 53 engine-generated tests, and 61
routed-callback tests, for 140 tests across the three workspaces. The two later
inventories are also recorded at
`spec/engine-generated-settlement-spike-results.md:189-193` and
`spec/routed-callback-auth-spike-results.md:136-145`; the workflow also rebuilds
and checksum-verifies the real SBF artifacts before running the suites.

Ratings use the framework's 0-to-4 scale and fail low when a control needed by
the assessed gate is absent. The score is not an average of the historical
17/36 authority-only checkpoint at `spec/maturity-checkpoint.md:23-37` and the
19/36 engine-generated experiment checkpoint at
`spec/engine-generated-settlement-spike-results.md:382-400`. Those scores answer
narrower experiment-level questions. This checkpoint asks whether the combined
evidence is ready to become a generic interface or custody system, so absent
generic and operational controls constrain the rating even when a fixed fixture
is strong.

## Corrected documentation baseline

The following documentation drifts were corrected in the current documentation
worktree after the code baseline above. They are not part of CI run
`33197777169`; they are evidence-hygiene improvements, not additional protocol
proofs and not score increases:

- The engine-boundary record now says that all three planned callback variants
  were measured, records Candidate A only as the next private-gate choice, and
  keeps the public shape undecided at `spec/engine-boundary-spike.md:3-16` and
  `spec/engine-boundary-spike.md:244-263`.
- The routed result now distinguishes the absent wallet authority and inherited
  wallet signature from the writable user token source and destination that
  Core still receives for settlement at
  `spec/routed-callback-auth-spike-results.md:150-155`. Any broader statement
  that the routed closure contains no user account would be false.
- The earlier maturity checkpoint is now explicitly labeled as the historical
  authority-kernel-only snapshot at `spec/maturity-checkpoint.md:3-12`, rather
  than a current repository score.
- The changelog now records all three experiments and limits Candidate A to the
  private next gate at `CHANGELOG.md:13-22`.
- The runtime record now includes the final revalidation observation, the active
  255 CPI-account-info limit, inactive transaction-v1 status, and the fact that
  the Instructions sysvar does not expose inner CPIs at
  `spec/runtime-baseline.md:10-24`, `spec/runtime-baseline.md:26-50`, and
  `spec/runtime-baseline.md:52-76`.

These corrections remove stale or overbroad prose. They do not establish the
generic effect algebra, production code identity, exits, claims, monitoring, or
release controls that remain absent.

## What the three experiments establish

| Experiment | Strongest exact evidence | Hard boundary of that evidence |
| --- | --- | --- |
| Authority kernel | A fixed Core-mediated A-to-B classic-SPL path gives the engine only its state and the Instructions sysvar, authenticates its receipt, enforces user bounds and accounted balances, and rolls all state back on failure (`spec/authority-kernel-spike-results.md:18-43`, `spec/authority-kernel-spike-results.md:45-97`). | Caller supplies the output, the domain is exclusive, engine identity is numeric rather than loader-backed, and there is no withdrawal or fee claim (`spec/authority-kernel-spike-results.md:144-170`). |
| Engine-generated settlement | The engine generates the output scalar; Core binds an ordered opaque capability closure, authenticates return data, applies checked settlement, and exercises hostile exact-SBF cases in 53 tests (`spec/engine-generated-settlement-spike-results.md:37-55`, `spec/engine-generated-settlement-spike-results.md:189-193`). | The entire generated "plan" is one `amount_out` scalar and proves neither a generic effect topology nor richer intent semantics (`experiments/engine-generated-settlement/README.md:37-40`, `spec/engine-generated-settlement-spike-results.md:402-416`). |
| Routed callback authentication | Top-level owner authorization, an exact one-shot spend delegate, permissionless routed execution, a phase-scoped Core-to-engine signer, normalized capabilities, receipt binding, replay rejection, and rollback are exercised; Candidate A is cheaper than Candidate B in the controlled fixture (`spec/routed-callback-auth-spike-results.md:49-90`, `spec/routed-callback-auth-spike-results.md:92-180`). | The result covers one classic-SPL A-to-B control. It does not establish a public ABI, multi-leg/stored/partial-fill semantics, loader identity, shared admission, claims, exits, or deployment (`spec/routed-callback-auth-spike-results.md:182-196`, `spec/routed-callback-auth-spike-results.md:226-244`). |

Together these are credible evidence that a narrow capability-separated control
path is executable. They are not evidence that one generic ABI can safely encode
unknown market effects or that persistent custody remains solvent and escapable.

## Nine-category scorecard

| Category | Rating | Score | Evidence and next-gate limiter |
| --- | --- | ---: | --- |
| Arithmetic | Moderate | 2 | Release overflow checks are enabled and the disposable experiment's explicitly non-production 30-BPS fixture uses checked `u128` arithmetic with ceiling rounding (`Cargo.toml:29-30`, `programs/core/src/constants.rs:31-33`, `programs/core/src/math.rs:5-17`). That historical fixture is not the accepted V1 five-BPS cumulative-floor constitution. The routed reference engine uses checked CPMM arithmetic and a 4,096-case independent model (`experiments/routed-callback-auth/test-programs/routed-plan-engine/src/lib.rs:274-327`, `experiments/routed-callback-auth/test-programs/routed-plan-engine/src/lib.rs:845-908`). The next gate still lacks an implemented generic effect-conservation, netting, fee-attribution, and multi-asset rounding model; the current receipt returns only one output scalar (`experiments/routed-callback-auth/crates/routed-callback-probe-wire/src/lib.rs:243-250`). |
| Auditing | Weak | 1 | Typed authorization and execution events include identities, digests, sequences, and post-accounting (`experiments/routed-callback-auth/programs/routed-callback-core/src/events.rs:27-77`), and a private vulnerability intake exists (`SECURITY.md:9-18`). There is no production event encoding, independent indexer implementation, gap detector, alert policy, runbook, or exercised incident response; event and checkpoint choices remain open (`spec/protocol-boundaries.md:101-120`, `spec/protocol-boundaries.md:133-150`). |
| Authentication and access control | Moderate | 2 | Spend and callback PDA namespaces are explicitly disjoint (`experiments/routed-callback-auth/programs/routed-callback-core/src/constants.rs:21-30`); authorization must be top-level and binds exact user terms (`experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/authorize_spend_v0.rs:38-74`); opaque signers, fixed-role aliases, Core accounts, and writable token accounts are rejected (`experiments/routed-callback-auth/programs/routed-callback-core/src/validation.rs:43-95`); the callback signer is engine, market, domain, intent, and phase scoped (`experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:663-819`). The market binds program and state addresses plus a self-declared numeric revision (`experiments/routed-callback-auth/programs/routed-callback-core/src/state.rs:5-32`), but not loader state, upgrade authority, immutable code, or shared-domain admission. |
| Complexity management | Weak | 1 | Math, validation, state, events, and wire code are separated, and isolated workspaces prevent an experiment from silently becoming the root interface (`experiments/routed-callback-auth/README.md:16-26`). However the routed execution handler spans `experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:161-443` before helpers, that source file is 1,101 lines, and the hand-maintained private wire module is 1,517 lines. Three intentionally copied workspaces now describe overlapping generations of the design, while no small accepted `engine-interface`, client, or compatibility surface exists (`spec/repository-boundaries.md:13-32`, `spec/repository-boundaries.md:38-80`). |
| Decentralization | Weak | 1 | Ordinary tested settlement has no global allowlist, admin signer, server, or indexer dependency, and the permissionless router is treated as untrusted (`spec/protocol-boundaries.md:52-77`, `spec/routed-callback-auth-spike-results.md:150-174`). Governance and review are nevertheless single-maintainer today (`.github/CODEOWNERS:1-12`); trusted acceptance is explicitly blocked until organizational ownership and independent multi-person review (`spec/repository-boundaries.md:109-116`). No deployment authority, immutability, migration, independent exit, or release-attestation policy is implemented (`VERSIONING.md:18-34`). |
| Documentation | Moderate | 2 | Architecture, threats, runtime assumptions, provenance, resource measurements, non-proofs, and experiment isolation are explicit, and the known drifts above are corrected. The repository still has no accepted intent/effect/event bytes, IDL, Rust/TypeScript client contract, compatibility fixtures, or user integration guide; the core choices remain open at `spec/protocol-boundaries.md:133-150`, and experiment bytes are expressly not public at `spec/repository-boundaries.md:46-60`. |
| Transaction ordering | Weak | 1 | The fixed route binds expiry, user debit/output limits, a one-shot authorization nonce, expected engine sequence, receipt sequence, and atomic rollback (`experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:37-50`, `experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:161-172`, `experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:293-330`). It does not model stored intents, concurrent partial fills, cancellation races, multi-intent netting, auctions, asynchronous settlement, or general MEV; authorization neutrality for stored and multi-intent use is an explicit pre-ABI gate (`spec/decisions/0002-core-mediated-capability-settlement.md:197-216`). |
| Low-level manipulation | Moderate | 2 | The fixed-width codec rejects wrong lengths before cursor reads and validates versions, phases, digests, and canonical padding (`experiments/routed-callback-auth/crates/routed-callback-probe-wire/src/lib.rs:399-435`, `experiments/routed-callback-auth/crates/routed-callback-probe-wire/src/lib.rs:475-500`, `experiments/routed-callback-auth/crates/routed-callback-probe-wire/src/lib.rs:668-717`). CPI metas and the callback signer are manually constructed and return data is read immediately from the authenticated setter (`experiments/routed-callback-auth/programs/routed-callback-core/src/instructions/execute_callback_authenticated_probe_v0.rs:742-819`). No unsafe Rust or assembly is present, but these private hand-maintained offsets and CPI builders have no parser fuzzing, cross-language differential suite, or accepted compatibility corpus. |
| Testing and verification | Moderate | 2 | The 140-test combined inventory, real SBF execution, hostile programs, direct/routed equivalence, rollback cases, resource measurements, pinned toolchains, and exact artifact checks are substantial experiment evidence (`.github/workflows/repository.yml:84-107`, `.github/workflows/repository.yml:163-191`, `.github/workflows/repository.yml:247-299`). There is no coverage gate, mutation testing, stateful invariant fuzzing, parser fuzzing, formal verification, Surfpool/fork suite, devnet evidence, independent review, second-builder release attestation, or onchain artifact comparison (`spec/engine-generated-settlement-spike-results.md:387-397`, `spec/runtime-baseline.md:115-125`, `spec/authority-kernel-spike-results.md:137-142`, `spec/engine-generated-settlement-spike-results.md:361-364`). |

**Overall: 14/36, or 1.6/4.0: Weak for the next-gate target.**

The strongest current evidence is capability separation, fixed-path arithmetic,
exact-SBF hostile testing, rollback, and reproducible experiment artifacts. The
score remains weak because a generic public interface would freeze semantics
that have not been defined or falsified across richer intents, while custody
would introduce claims and liveness obligations that do not exist in code.

## Hard gaps that test count cannot close

1. **No generic effect algebra.** The current engine output is a scalar. There is
   no accepted ordered effect representation, protected/opaque effect split,
   conservation and netting rule, fee attribution rule, failure semantics, or
   resource bound for multi-leg behavior.
2. **No authorization-neutral intent model.** Direct and permissionless-routed
   execution are equivalent only for one one-shot exact-delegate intent. Stored,
   detached, multi-party, partial-fill, cancel, replace, expiry-race, and
   multi-intent semantics remain unproved.
3. **No code-identity or shared-domain admission proof.** A numeric engine
   revision is metadata, not loader-aware identity. No executable descriptor
   binds loader, upgrade state, interface, capability profile, and a domain's
   local admission rule (`spec/engine-boundary-spike.md:58-62`,
   `spec/engine-boundary-spike.md:146-169`).
4. **No closed protected-asset boundary beyond classic SPL.** Token-2022 hooks,
   transfer fees, Permanent Delegates, custom asset authorities, and an external
   settlement driver require separate authority and accounting proofs
   (`spec/engine-boundary-spike.md:171-187`, `spec/runtime-baseline.md:94-106`).
5. **No provider or protocol claimant state machine.** Current experiment
   entrypoints initialize, deposit, authorize, and execute; there is no
   withdrawal, provider entitlement, position, or fee-claim instruction
   (`experiments/routed-callback-auth/programs/routed-callback-core/src/lib.rs:17-47`).
   `DomainV0` stores aggregate balances only
   (`experiments/routed-callback-auth/programs/routed-callback-core/src/state.rs:19-32`).
6. **No public compatibility surface.** The intended `engine-interface`, Rust
   client, TypeScript client, IDL, and compatibility fixtures are directory-plan
   entries, not implemented public packages (`spec/repository-boundaries.md:13-32`).
7. **No release or operating evidence.** Current checksum pins are experiment
   reproduction evidence. There is no signed release, second independent builder,
   deployment manifest, onchain ELF/IDL verification, independent indexer,
   alerting, incident exercise, or external assessment.

## Ordered gates before a generic public ABI

The following order is dependency-driven. Work inside later gates may be
researched in parallel, but no later gate can compensate for a failed earlier
one.

1. **Freeze the private input, not the public shape.** Use Candidate A only as
   the callback timing input to the next experiment. Keep every current intent,
   request, receipt, account layout, discriminator, and PDA namespace private.
   The current decision explicitly selects timing without accepting a public
   shape at `spec/engine-boundary-spike.md:259-263`.
2. **Define and falsify the generic effect/capability algebra.** Specify ordered
   Core-verified effects, opaque engine capabilities, participating domains,
   conservation/netting, fee assessment, failure and replay semantics, and hard
   packet/account/compute bounds. Exercise at minimum a delayed order or auction,
   a bidirectional multi-leg action, and a custom-authority asset counterexample.
   A scalar-output fixture cannot pass this gate.
3. **Prove authorization neutrality.** Run the same canonical effect commitment
   through direct, permissionless-routed, stored, multi-intent, partial-fill,
   cancel/revoke, expiry-race, and concurrent execution models. The plan boundary
   passes only if authorization transport does not change economic meaning or
   protected authority. This requirement is already explicit at
   `spec/decisions/0002-core-mediated-capability-settlement.md:212-214`.
4. **Bind code identity and domain admission.** Implement loader-aware mutable,
   pinned, and immutable engine policies; safe upgrade transitions; exact shared
   domain descriptors; and local admission proofs. Test substitution, upgrade,
   stale-loader, shared-state, cross-market, and cross-engine cases.
5. **Close the asset and settlement-driver boundary.** Accept exact Core-native
   Token/Token-2022 profiles individually and either prove an external protected
   settlement driver or keep unsupported assets structurally opaque and unable
   to receive Core custody claims. Do not add generic "token" flags to bypass
   extension-specific authority graphs.
6. **Only then design public bytes and packages.** Produce the reviewed engine
   interface, versioned IDL, canonical Rust and TypeScript clients, golden
   cross-language vectors, old/new compatibility fixtures, event schema, error
   contract, resource maxima, and migration semantics described at
   `spec/repository-boundaries.md:68-80` and `VERSIONING.md:6-16`.
7. **Earn compatibility confidence.** Add stateful invariant and parser fuzzing,
   mutation and coverage gates, cross-language differential tests, resource
   exhaustion sweeps, current-runtime and forked-cluster tests, and independent
   multi-person review. Two independent third-party engine implementations must
   integrate from the published package alone. Until those results agree, the
   interface remains experimental even if its version field says otherwise.

A generic ABI may deliberately exclude persistent custody profiles. If it
includes any such profile, all custody gates below become pre-ABI requirements,
not post-publication work.

## Additional ordered gates before any custody deployment

1. **Choose an exact exit class before accepting deposits.** Each domain must
   immutably select no persistent custody, an exact Core-verifiable
   engine-independent claim, or explicitly disclosed engine-liveness dependence.
   No persistent Core custody may be deployed or funded before the exact exit
   and claim path is independently proven
   (`spec/engine-generated-settlement-spike.md:501-519`,
   `spec/decisions/0002-core-mediated-capability-settlement.md:132-143`).
2. **Implement claimant accounting and prove solvency.** Provider entitlement,
   deposit shares or positions, withdrawals, protocol-fee liability and claims,
   donation treatment, rounding, underfunding, closure, and account lifecycle
   must be one executable state machine with stateful invariant tests. Aggregate
   vault balances are insufficient.
3. **Prove failure and third-party-upgrade liveness for that exit class.**
   Exercise a missing, frozen, malicious, and upgraded engine; expired or
   unavailable dependencies; token extension callbacks; and partial opt-in
   migrations. Separately prove that the Production Core has no upgrade or
   administrative state (`spec/decisions/0004-production-core-is-adminless.md`).
4. **Establish release identity.** Reproduce the canonical ELF with at least two
   independent builders, authenticate the release, publish an append-only
   manifest, and verify source commit, artifact hash, IDL hash, loader state,
   deployment transaction, slot, and onchain ELF as required by
   `VERSIONING.md:18-25`.
5. **Establish independent detection and response.** Run an independent indexer
   that authenticates successful Core invocation context and state checkpoints,
   alerts on gaps and solvency violations, and exercises incident, new-major,
   offchain-route-removal, recovery, and communication runbooks. Monitoring is not an
   exit path, but custody without detection is not ready.
6. **Obtain independent security review against the final release candidate.**
   Resolve findings against the exact source and artifact, rerun all invariant,
   adversarial, fork, and devnet suites, and invalidate the review if authority,
   loader, accounting, wire, or release inputs change.
7. **Use a staged but adminless deployment progression.** Verify the manifest
   and immutable onchain program first; any asset, domain, value, or time limit
   must be immutable, domain-local, or user-selected rather than a privileged
   Core control. Repository CI success alone never authorizes real assets.

## Next allowed conclusion

The combined evidence supports exactly one next move: a private generic
effect/capability experiment that starts from Candidate A and tries to break the
model with richer effects and authorization forms. It does not support publishing
the current bytes as a generic ABI, and it does not support deploying or funding
any persistent custody account. The 14/36 score is a gate-readiness checkpoint,
not an audit conclusion or product status.
