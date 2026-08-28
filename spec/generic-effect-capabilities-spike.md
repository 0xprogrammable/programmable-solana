# Generic effect capabilities spike

Status: Proposed private experiment; no implementation and no public interface

Date: 2026-08-28

This document authorizes one disposable architecture experiment after the routed
callback authentication result. It does not accept a public engine ABI, Core
account layout, SDK, product resource limit, deployment artifact, custody
design, or fee schedule.

## Decision to falsify

The experiment tests one narrow hypothesis:

> One permissionless engine can perform arbitrary engine-owned and opaque state
> transitions in a single authenticated pre-settlement callback, return a
> canonical product-neutral graph of protected-value moves indexed into a
> Core-validated settlement-capability table, and let Core execute those moves,
> mandatory fees, and domain-local accounting atomically without giving the
> engine any user, custody, fee, claim, or other protected authority.

The hypothesis includes routed execution. A permissionless router may invoke
Core without receiving a user signer. The engine callback contains only one
fixed read-only callback signer followed by an ordered opaque account tail. The
public design must not freeze one `engine_state` account: zero, one, or many
engine-owned state accounts are positions in that opaque tail and are bound by
their actual landing-time capabilities.

The hypothesis is false if a representative market requires any of the
following:

- a Core product or action enum;
- a fixed engine-state account in the engine interface;
- a user, vault, fee, claim, admission, or upgrade signer in the engine CPI;
- a protected writable asset account in the engine CPI;
- an arbitrary settlement-driver call carrying Core authority;
- a market declaration that grants itself access to an existing domain;
- an engine- or caller-selected protocol-fee policy or assessable label;
- authorization-specific fields in the engine effect result;
- unverifiable netting across protected accounts, assets, profiles, or domains;
- partial, zero-debit, or multi-intent execution that can replay without
  explicit Core state;
- future transaction-v1, account-lock, or CPI-stack capacity; or
- a transaction that exceeds the predeclared experimental resource headroom.

Passing this spike permits a decision about the next private architecture gate.
It does not name the candidate wire `v1`, make it stable, or authorize real
funds.

## Product-neutral boundary

The Core may understand protected authority facts. It must not understand why
an engine requested an effect.

The following belong to engines and their optional schemas:

- curves, price functions, bonding schedules, and spreads;
- auctions, matching, orders, inventory selection, and allocation;
- provider economics, rewards, points, referrals, and creator economics;
- game state, eligibility, oracle interpretation, and time phases;
- engine-owned positions, claims, receipts, and lifecycle state; and
- unnamed future market semantics.

The first protected effect under test is an exact classic SPL Token move. It is
an authority primitive, not a declaration that an action is a swap, deposit,
withdrawal, sale, royalty, or claim. Mint, burn, compressed-asset mutation,
Token-2022 behavior, Core-native positions, external custody, and custom
settlement drivers remain separate authority-profile decisions.

Arbitrary semantics and arbitrary protected authority are deliberately not the
same promise. An engine that needs authority outside an accepted protected
profile may still own or compose that authority in the opaque engine plane, but
the resulting effect is `EngineAttested`, not `CoreVerified`.

## Isolation rules

Any later implementation authorized by this document must live in a new
isolated nested workspace under `experiments/generic-effect-capabilities/`.

That workspace must:

- have its own workspace manifest and lockfile;
- remain outside root workspace membership and the default build;
- mark every crate `publish = false`;
- use disposable program IDs, discriminators, seeds, account layouts, codecs,
  limits, and fixture names;
- import no candidate wire through the canonical Core or public engine-interface
  crates;
- expose no maintained IDL, SDK, code generator, release tag, or compatibility
  promise;
- contain no deployment keypair, cluster address, deploy command, upgrade
  authority, or real-fund configuration; and
- execute only in an in-process or local test runtime. A forked local runtime is
  evidence about runtime behavior, not a deployment.

This document itself adds no implementation. Any implementation is a later
reviewable change and may be deleted after its result is recorded.

## Terminology

### Protected plane

The protected plane contains user funding accounts, Core-custodied domain
assets, exact credit recipients, protocol-fee accounts, domain admission state,
authorization state, Core accounting, and every signer or writable account that
can create a `CoreVerified` value effect or right.

### Opaque engine plane

The opaque engine plane contains the selected engine program, a rightless
callback-authentication signer, engine-owned state, and external accounts or
programs explicitly supplied to the engine. Core authenticates this plane's
actual capability closure but does not certify its economics, data, ownership
model, solvency, or side effects.

### Engine instance

A market binds an opaque 32-byte `engine_instance_id`. It is an identity input,
not an account address and not proof of code identity. The engine decides which
zero, one, or many opaque-tail accounts represent that instance. Core never
requires a fixed `engine_state` account in the callback prefix.

### Settlement capability

A settlement capability is a Core-internal indexed right to request one bounded
protected effect. It is not a PDA bearer token, signer, delegate, or account
forwarded to the engine.

### Opaque capability

An opaque capability is an actual ordered `AccountInfo` position passed to the
engine CPI after landing-time owner, executable state, and effective privileges
have been validated and committed.

### Canonical private hashing

The experiment uses SHA-256 for every candidate digest. To remove concatenation
ambiguity, the private helper is exactly:

```text
H(label, parts...) = SHA256(
  "programmable/private-effect-capabilities/2026-08-28" ||
  u16_le(byte_length(label)) || label ||
  u16_le(part_count) ||
  for each part in order: u32_le(byte_length(part)) || part
)
```

Labels are lower-case ASCII and are distinct for protected and opaque
capabilities, intent identity and Core terms, authorization views, protected
execution, domains, assets, market binding, loader state, admission, callback
seed, engine request, canonical effects, fee assessment, and evidence.
A list digest is `H(label, u32_le(count), row_0, ..., row_n)`;
there is no implicit sorting except where this document explicitly requires it.
The complete encoded row, including its original numeric position and zero
reserved bytes, is the list element. Empty lists therefore have unambiguous
roots.

All roots are recomputed by Core from the landed instruction and current
accounts. A client-supplied root is an equality assertion, never an authority
source. These choices are disposable private test vectors, not a public hashing
contract.

## Two capability tables

The experiment has two disjoint tables. Combining them, forwarding one as the
other, or using one root for both is a falsification.

### Settlement-capability table

Core derives an ordered table `S = [S_0, ..., S_n]` from the actual protected
accounts, market and domain state, authorization witnesses, and accepted
settlement profile. Core commits its table root inside
`protected_execution_root`. The engine receives that composite root and selected
numeric context, never these `AccountInfo`s or the table root as an independent
authority.

Each private descriptor commits to at least:

```text
position
core program and experimental major
market
settlement profile
asset identity and asset program
exact endpoint account and landing-time owner
domain descriptor and revision, or an explicit no-domain marker
domain admission proof, when a domain is present
authority class and debit/credit/reserved-fee rights
authorization slot or domain-accounting slot
maximum engine debit and terminal minimum credit relevant to that slot
fee class and policy revision
fee-shard index or explicit no-shard marker
lifecycle and profile facts required by the exact asset profile
```

The exact landing-time digest row is deliberately separate from the compact
caller row:

```text
ProtectedCapabilityDigestRowCandidateV0 {       // exactly 368 bytes
  position: u8
  asset_index: u8
  domain_index_or_none: u8
  authorization_slot_or_none: u8
  authority_class: u8
  fee_class: u8
  fee_shard_index_or_none: u8
  flags: u8
  rights_bits: u16
  domain_accounting_slot_or_none: u8
  spend_authority_control_offset_or_none: u8
  endpoint_executable: u8
  endpoint_effective_signer: u8
  endpoint_effective_writable: u8
  reserved: u8
  endpoint_key: [u8; 32]
  endpoint_owner: [u8; 32]
  transfer_authority_key_or_zero: [u8; 32]
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
  domain_descriptor_or_zero: [u8; 32]
  domain_admission_digest_or_zero: [u8; 32]
  lifecycle_digest: [u8; 32]
  domain_revision: u64
  maximum_engine_debit: u64
  maximum_total_debit: u64
  minimum_credit: u64
  maximum_protocol_fee: u64
  fee_policy_revision: u64
  accounted_before_or_zero: u128
}

protected_capability_set_digest = H(
  "protected-capability-set-v0",
  u32_le(settlement_capability_count),
  every complete 368-byte row in contiguous position order
)
```

The 16-byte prefix, nine 32-byte fields, six little-endian `u64` values, and
final little-endian `u128` total exactly 368 bytes. The three endpoint privilege
bytes are canonical booleans, and `reserved` is zero. A no-domain row requires
the domain index and accounting slot to be `255` and every domain-only field,
including `accounted_before_or_zero`, to be zero. The lifecycle digest commits
the exact classic-SPL endpoint state and any accepted profile-specific facts.

For the first protected profile that lifecycle state is typed, not raw token
account bytes:

```text
ClassicSplEndpointStateRowCandidateV0 {          // exactly 224 bytes
  wire_version: u8
  account_state: u8
  delegate_present: u8
  native_present: u8
  close_authority_present: u8
  reserved: [u8; 3]
  endpoint_key: [u8; 32]
  token_program: [u8; 32]
  mint: [u8; 32]
  token_owner_authority: [u8; 32]
  delegate_or_zero: [u8; 32]
  close_authority_or_zero: [u8; 32]
  amount: u64
  delegated_amount: u64
  native_reserve_or_zero: u64
}

lifecycle_digest = H(
  "classic-spl-endpoint-state-v0",
  complete 224-byte lifecycle row
)
```

The three presence fields are canonical booleans. An absent option requires its
dependent key or value and delegated amount to be zero; a present option may
carry an all-zero key because its explicit tag removes sentinel ambiguity. The
token program is exact classic SPL Token and must equal the endpoint's account
owner already committed by the protected row. This initial profile admits only
the initialized account state; uninitialized, frozen, and unknown states fail
before callback. Native wrapping is encoded exactly but remains outside the
accepted fixture unless its separate profile rules pass.

The list digest includes every original position. It is one input to the later
`protected_execution_root`; it is not sufficient on its own because it does not
describe mutable authorization state. Protected public keys are unique in this
table. One source account used for both a market debit and the Core-derived fee
is one capability with multiple outgoing moves, not two aliased semantic roles.

The first experiment may use private authority classes equivalent to:

- intent-funded debit;
- domain-accounted debit or credit;
- exact external credit recipient; and
- Core-reserved fee credit.

These are authority facts for the classic-SPL experiment, not public product
types. Unknown classes fail closed. A later authority primitive requires a
separate decision even if an engine can describe it in opaque bytes.

Every settlement capability is validated before the engine callback. At least:

- its public key, owner, executable state, expected PDA or authorization
  relation, mint, asset program, and lifecycle are exact;
- its actual outer signer and writable privileges are no greater than the fixed
  Core route permits;
- duplicate public keys do not create two protected roles;
- a domain capability names one exact admitted domain and accounting slot;
- a user debit capability has one valid direct, exact-delegate, or stored
  authorization witness;
- an exact credit capability binds its recipient before the engine runs; and
- a fee capability is reserved to Core and cannot be referenced in the engine
  result.

### Ordered opaque engine-capability table

The engine CPI account metas are exactly:

```text
0. callback authority: read-only signer, non-executable
1..N. ordered opaque capability tail
```

The selected engine program is the CPI target. It is not a fixed engine-state
account. The opaque tail may contain zero, one, or many engine-owned state
accounts, arbitrary read-only programs, and explicitly risk-accepted external
state.

For every opaque-tail position Core records:

```text
position || key || landing-time owner || executable
         || effective signer || effective writable

OpaqueCapabilityDescriptorCandidateV0 {         // exactly 68 bytes
  position: u8
  key: [u8; 32]
  landing_time_owner: [u8; 32]
  executable: u8
  effective_signer: u8
  effective_writable: u8
}

opaque_capability_root = H(
  "opaque-capability-set-v0",
  u32_le(opaque_capability_count),
  every complete 68-byte row in contiguous position order
)
```

The three privilege fields are canonical booleans.

Order and multiplicity are preserved. Security validation is by public key:
every occurrence of one key receives the union of signer and writable privilege
visible anywhere in the complete landing-time instruction. Static and
address-lookup-table resolution must not create distinct security identities for
the same key.

Before the engine CPI, Core rejects an opaque-tail key that:

- has effective signer privilege;
- aliases any fixed Core control account or any settlement capability;
- is owned by the experimental Core;
- is executable and effectively writable;
- is effectively writable and owned by classic SPL Token or Token-2022;
- aliases an authorization, domain-admission, fee, loader-policy, callback, or
  other protected account; or
- exceeds the private account or payload limits.

The engine may combine the surviving capabilities, forward them, invoke supplied
programs, and introduce signers for its own PDAs. Those are real opaque-plane
rights. Core does not treat a declared nested CPI script as a sandbox.

### Cross-table disjointness

Disjointness is checked after privilege union and before either hash is accepted.
It includes ancillary accounts introduced by the selected protected profile.
A future Token-2022 transfer-hook extra, for example, may not silently alias an
engine state, callback authority, domain account, fee account, or another
protected role.

The callback signer is not part of either economic capability table. It is a
fixed authentication-plane account bound separately to the exact callback.

## Market and engine binding

The candidate market binding is exact:

```text
MarketBindingRowCandidateV0 {                    // exactly 332 bytes
  Core program: [u8; 32]
  Core experimental major: u32
  market_key: [u8; 32]
  market_descriptor_revision: u64
  engine_program: [u8; 32]
  engine_interface_id: [u8; 32]
  engine_instance_id: [u8; 32]
  engine_admission_policy_digest: [u8; 32]
  domain_admission_profile_digest: [u8; 32]
  protected_capability_profile_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
  opaque_schema_digest: [u8; 32]
}

market_binding_digest = H(
  "market-binding-v0",
  exact encoded 332-byte market binding
)
```

Fee revision is already committed by `fee_policy_digest` and is not duplicated
in this row.

Persistent intent and market terms use an asset binding whose first byte is a
wire version, not a transient global asset index:

```text
AssetBindingRowCandidateV0 {                     // exactly 100 bytes
  wire_version: u8
  asset_flags: u8
  decimals: u8
  reserved: u8
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
}

asset_binding_digest = H("asset-v0", encoded 100-byte asset binding)

asset_set_digest = H(
  "asset-set-v0",
  u32_le(asset_count),
  every complete 100-byte persistent binding in execution asset-index order
)
```

Execution asset indices are assigned by strictly increasing
`asset_binding_digest`; duplicate digests fail. The set root therefore has one
canonical order rather than accepting caller-selected permutations.

The classic-SPL fixture requires the candidate wire version, zero flags and
reserved byte, and exact mint, program, decimals, and settlement-profile facts.
This row is not interchangeable with the later 100-byte `EngineAssetRow`, whose
first byte is an execution-local `asset_index`. Core validates both against the
same landed asset facts but hashes them in their own domains.

`engine_instance_id` replaces a fixed `engine_state` address. Engine-owned
account addresses and privileges are instead committed by the opaque-capability
root for the exact execution.

Permissionless means any developer can deploy an executable engine and create a
new compatible market and domain under deterministic public rules. It does not
mean an engine may select a weaker protected profile, forge an existing domain's
admission, or receive undeclared authority.

The experimental Core configuration may identify supported interface majors,
loader parsers, settlement profiles, and fee-policy roots. It may not enumerate
approved engine program IDs, require a platform co-signer for market or domain
creation, or maintain a writable global execution registry. Domain-local
admission is the only engine allow/deny decision in an ordinary settlement.

## Loader-aware engine policy gate

A numeric engine revision is experiment metadata, not code identity. Before the
callback, Core validates the selected program against the exact loader-aware
policy bound by the market and every participating domain.

The experiment separates a long-lived admission policy from the exact
Loader-v3 state observed for one execution. A ProgramData slot and controller
are not an ELF hash, do not prove controller history, and are not an exact code
identity across competing forks. Collapsing policy and observation into one
hash would still either make a mutable-controller policy accidentally pinned or
let a user intent float across loader-state changes.

The three private admission policies are:

1. **Immutable release evidence** — a permissionless, Core-owned release PDA is
   captured at a strictly later slot after Core proves the exact Loader-v3
   Program/ProgramData relation and `authority == None`. The evidence records
   the observed ProgramData last-modified slot, data length, admission-policy
   digest, and loader-state-snapshot digest. Ordinary execution loads this
   read-only evidence and the executable Program, but omits ProgramData. There
   is no platform allowlist or writable release registry in the hot path.
2. **Pinned mutable loader state, rejected as a public policy** — program ID,
   loader, ProgramData address, last-modified slot, and exact current controller
   are pinned for the hostile fixture. Loader-v3 `ExtendProgram` can be funded
   by an unrelated party for mutable ProgramData, changes its slot, and can
   increase its loaded data toward the loader cap without changing economic
   code. The fixture therefore demonstrates a targeted liveness and resource
   denial; it must not pass as a strong public admission class.
3. **Explicit mutable-controller risk** — program ID, loader, ProgramData
   address, and one exact visible controller are admitted, while future
   last-modified slots under that controller are deliberately accepted by the
   domain policy. Every individual intent still binds one exact loader-state
   snapshot. This class explicitly trusts controller behavior and mutable
   ProgramData liveness; it is never described as pinned, immutable, or exact
   code identity.

Core derives both facts from parsed onchain loader state:

```text
EngineAdmissionPolicyCandidateV0 {
  policy_kind: u8
  reserved: [u8; 7]
  engine_program: [u8; 32]
  loader_program: [u8; 32]
  program_data_or_zero: [u8; 32]
  expected_controller_or_zero: [u8; 32]
  captured_programdata_slot_or_zero: u64
}

EngineLoaderStateSnapshotCandidateV0 {
  engine_program: [u8; 32]
  loader_program: [u8; 32]
  program_data_or_zero: [u8; 32]
  observed_programdata_slot: u64
  observed_controller_or_zero: [u8; 32]
}

engine_admission_policy_digest = H(
  "engine-admission-policy-v0",
  encoded admission policy
)

engine_loader_state_snapshot_digest = H(
  "engine-loader-state-snapshot-v0",
  encoded execution snapshot
)
```

The admission policy is exactly 144 bytes and the loader-state snapshot is
exactly 136 bytes. Private policy kinds `0`, `1`, and `2` mean immutable release,
rejected pinned-mutable fixture, and mutable-controller risk. For kind `2`,
`captured_programdata_slot_or_zero` is zero, so its admission digest does not
change on an accepted controller modification. Kinds `0` and `1` record the
observed slot, including valid slot zero; the policy kind removes any zero-value
ambiguity. Unknown kinds, nonzero reserved bytes, impossible controller
combinations, and unsupported loaders fail closed.

The immutable capture commits this exact 208-byte observation row:

```text
ImmutableEngineReleaseObservationCandidateV0 {
  engine_program: [u8; 32]
  loader_program: [u8; 32]
  canonical_program_data: [u8; 32]
  captured_programdata_slot: u64
  observed_controller_or_zero: [u8; 32]
  captured_programdata_data_len: u64
  engine_admission_policy_digest: [u8; 32]
  loader_state_snapshot_digest: [u8; 32]
}

immutable_release_observation_digest = H(
  "immutable-engine-release-observation-v0",
  Core program,
  u32_le(Core experimental major),
  complete 208-byte observation row
)
```

The release account is the canonical PDA for the exact Core and engine program.
Capture is top-level-only when it spends a payer's lamports, validates the exact
Loader-v3 ID before any write, and is idempotent only for byte-identical existing
evidence. Onchain capture proves a later-slot observation, not finality. Release
tooling must separately wait for the finalized fork before publishing it as
stable evidence.

The private capture instruction discriminator is `e3646e8c56a7c312`; its exact
144-byte argument is the already defined admission-policy row, without a second
wrapper or alternate encoding. Capture accepts only immutable policy kind zero
and rejects trailing data.

The market and every participating domain bind
`engine_admission_policy_digest`. The top-level envelope, every intent, and the
engine request bind the exact `engine_loader_state_snapshot_digest` used for
that execution. An immutable execution must resolve that digest through the
canonical Core-owned release PDA. A mutable execution must derive it from the
current ProgramData account. Thus a domain may accept a controller's future
loader states without silently extending an already signed user intent to one.

For upgradeable-loader v3, the Program state accepts every loader-valid account
length at least 36 bytes and parses the canonical prefix; ProgramData must be
longer than its 45-byte metadata prefix. ProgramData `authority == None` ignores
stale authority bytes left after the option tag by the official serializer.
The Program's embedded address and the canonical Loader-v3 PDA derivation must
both select the supplied ProgramData.

Immutable ordinary execution requires the exact read-only Core-owned release
PDA and omits ProgramData. Mutable execution requires Program and ProgramData
to be effectively read-only after duplicate-key union inside Core. In addition,
Core scans every top-level instruction meta through the authenticated
Instructions sysvar, because a router can downgrade writable CPI privilege while
another top-level instruction retains it. `Clock.slot` must be strictly greater
than the observed ProgramData last-modified slot. A same-slot deploy, upgrade,
extend, or execute attempt fails before callback. Loader, ProgramData, release,
and Instructions-sysvar controls are never opaque capabilities.

The immutable policy relies on current Solana runtime and Loader-v3 semantics
that make `authority == None` irreversible and reject `ExtendProgram` for an
immutable ProgramData account. A future cluster or loader semantic change at
the same loader ID is outside Core's control. `Some(controller)` proves only the
immediate authority pubkey, not the governance or code behind a PDA controller.
An A-to-B-to-A authority round trip with no program modification is not
historically detectable from the current tuple.

The experiment must measure the account, loaded-data, and compute cost of these
checks, including a mutable ProgramData extended toward its loader cap. It does
not hash an entire deployed ELF on every settlement and therefore never calls a
mutable loader-state tuple an exact code hash. Source, artifact, ELF, deployment,
fork finality, and onchain program-data identity remain distinct release-evidence
axes in addition to this executable admission gate. Strong engine versioning in
this candidate uses a new immutable program ID and release PDA per version; that
changes upgrade operations, not the semantics developers may implement.

## Domain-local admission proof

A domain identity includes at least:

```text
controller program and controller identity
domain descriptor revision
namespace or instance identity
custody profile
asset profile
accounting profile
exit class
admission rule
protected-capability profile
```

Core commits those facts through one exact descriptor codec:

```text
DomainDescriptorRowCandidateV0 {                 // exactly 304 bytes
  wire_version: u8
  rule_kind: u8
  reserved: [u8; 6]
  controller_program: [u8; 32]
  controller_identity: [u8; 32]
  domain_revision: u64
  namespace_or_instance: [u8; 32]
  custody_profile_digest: [u8; 32]
  asset_profile_digest: [u8; 32]
  accounting_profile_digest: [u8; 32]
  exit_class_digest: [u8; 32]
  admission_rule_digest: [u8; 32]
  protected_profile_digest: [u8; 32]
}

domain_descriptor_digest = H(
  "domain-descriptor-v0",
  Core program,
  u32_le(Core experimental major),
  complete 304-byte descriptor row
)
```

The account bump and stored self-digest are not part of this row. Unknown rule
kinds and nonzero reserved bytes fail. No caller or Core module may hash an
Anchor account serialization as a substitute for this codec.

The only rule kinds in this candidate are `DOMAIN_RULE_OPEN = 0` and
`DOMAIN_RULE_CLOSED = 1`. The open descriptor must carry exactly:

```text
open_domain_rule_digest = H("open-domain-rule-v0")
```

The closed descriptor must carry a nonzero digest different from that open-rule
constant. Every other rule kind, zero rule digest, or mismatched combination
fails closed.

Every domain debit or accounted credit in the settlement table requires a proof
owned or otherwise authorized by that domain's own admission rule. The proof
binds the exact relation among:

```text
domain descriptor and revision
market
engine program
engine interface ID
engine admission-policy digest
settlement-capability profile
```

A market, engine receipt, caller, plan, manifest, or global registry cannot
self-declare that relation. A domain may deliberately choose an open deterministic
rule. That remains a choice by the domain's liquidity providers, not approval by
Programmable.

The closed-rule fixture uses one Core-owned, domain-authorized private record:

```text
DomainAdmissionCandidateV0 {
  wire_version: u8
  reserved: [u8; 7]
  domain_descriptor: [u8; 32]
  domain_revision: u64
  market: [u8; 32]
  engine_program: [u8; 32]
  engine_interface_id: [u8; 32]
  engine_instance_policy_digest: [u8; 32]
  engine_admission_policy_digest: [u8; 32]
  settlement_profile_digest: [u8; 32]
  admission_rule_digest: [u8; 32]
  active_from_slot: u64
  expires_at_slot_or_zero: u64
  revoked_at_slot_or_zero: u64
}
```

The private record payload is exactly 296 bytes. Reserved bytes must be zero.
The first closed-rule profile supports one typed instance policy only:

```text
exact_engine_instance_policy_digest = H(
  "exact-engine-instance-policy-v0",
  Core program,
  u32_le(Core experimental major),
  engine_program,
  engine_interface_id,
  engine_instance_id
)
```

Core derives this digest from the authenticated market binding and requires it
to equal `engine_instance_policy_digest`. Direct equality with the opaque
`engine_instance_id` is forbidden: an identity byte string is not also a typed
policy digest. Any prefix, family, wildcard, controller-selected, or otherwise
broader instance policy requires a separately specified policy kind and hash
domain; unknown policy encodings fail closed.

```text
closed_domain_admission_digest = H(
  "domain-admission-record-v0",
  complete exact 296-byte admission row as one framed part
)
```

Its candidate address is:

```text
PDA(
  experimental Core program,
  "domain-admission-v0",
  H(
    "domain-admission-address-v0",
    domain_descriptor,
    u64_le(domain_revision),
    market,
    engine_program,
    engine_interface_id,
    engine_instance_policy_digest,
    engine_admission_policy_digest,
    settlement_profile_digest,
    admission_rule_digest
  )
)
```

The one digest seed keeps every PDA seed at most 32 bytes. Creation or revision
requires the exact authority described by the domain descriptor; this may be a
direct controller signer or a controller-program CPI, but never the market,
engine, router, callback signer, or a platform-wide signer. Ordinary settlement
parses the domain descriptor again, derives the relation and address, verifies
the record fields and active interval, and includes the complete record digest
in `protected_capability_set_digest` and `domain_set_digest`; both are then
committed by `protected_execution_root`.

An open-rule fixture contains its deterministic predicate and policy revision in
the domain descriptor. Core evaluates that predicate from the same engine,
market, code, and profile facts. "Open" therefore means the domain chose an open
predicate; it does not mean that the market can omit the proof or choose the
predicate at execution time.

Core normalizes either admission path into one exact execution row:

```text
DomainExecutionRowCandidateV0 {                 // exactly 208 bytes
  domain_index: u8
  admission_kind: u8
  reserved: [u8; 6]
  domain_descriptor_key: [u8; 32]
  domain_descriptor_digest: [u8; 32]
  domain_revision: u64
  admission_account_or_zero: [u8; 32]
  admission_digest: [u8; 32]
  accounting_account: [u8; 32]
  accounting_profile_digest: [u8; 32]
}

domain_execution_digest = H(
  "domain-execution-v0",
  market_binding_digest,
  complete 208-byte execution row
)

domain_set_digest = H(
  "domain-set-v0",
  u32_le(domain_count),
  each complete 32-byte domain_execution_digest in contiguous domain-index order
)
```

`ADMISSION_OPEN = 0` and `ADMISSION_CLOSED = 1` are the only admission kinds and
must equal the descriptor rule kind. For an open rule,
`admission_account_or_zero` is zero and Core derives:

```text
admission_digest = H(
  "open-domain-admission-v0",
  domain_descriptor_digest,
  market_binding_digest
)
```

It accepts only a Core-valid market and selected engine whose authenticated
admission and protected-profile facts match the descriptor. For a closed rule,
the admission account is nonzero, is the exact canonical PDA above, and its
complete authenticated record digest is the row's `admission_digest`; Core also
checks its matching facts, revision, active interval, and non-revocation. Domain
indices are assigned by strictly increasing `domain_descriptor_digest` and are
contiguous; duplicate descriptor digests fail. All identity digests and
accounting keys are nonzero, and reserved bytes are zero.

Multiple admitted markets may share one domain. They then deliberately share
reserves, locks, engine risk, economics, and liveness risk. A non-participating
domain cannot be debited, closed, redirected, or have its accounted rights
changed. An unsolicited raw credit remains possible but creates no accounted
liquidity, fee liability, or claim.

Admission proofs are domain-local and read-only on ordinary execution unless the
selected admission profile explicitly requires a local mutable replay or quota
fact. No global writable registry participates in settlement.

## Authorization-neutral internal model

The engine result and settlement algebra must not encode how user authority was
obtained. Immutable identity, immutable Core-enforced terms, and mutable
execution state are three separate things.

The exact inline identity is:

```text
InlineIntentIdentityRowCandidateV0 {             // exactly 80 bytes
  actor: [u8; 32]
  engine_terms_commitment: [u8; 32]
  authorization_nonce: u64
  expires_at_slot_exclusive: u64
}
```

`engine_terms_commitment` must be nonzero. It is opaque to Core and is where an
engine schema may commit developer-defined semantics, its own nonce domain,
payload rules, or any other engine-only term. `authorization_nonce` is the Core
identity nonce and is never mutable fill state.

Core-enforced local capability terms have an exact persistent encoding:

```text
IntentCapabilityTermCandidateV0 {                // exactly 136 bytes
  local_index: u8
  authority_class: u8
  fee_class: u8
  flags: u8
  rights_bits: u16
  reserved: [u8; 2]
  endpoint_key: [u8; 32]
  asset_binding_digest: [u8; 32]
  required_domain_descriptor_digest_or_zero: [u8; 32]
  maximum_engine_debit: u64
  maximum_total_debit: u64
  minimum_credit: u64
  maximum_protocol_fee: u64
}

intent_local_capability_terms_root = H(
  "intent-capability-terms-v0",
  u32_le(term_count),
  every complete 136-byte term in contiguous local-index order
)
```

Term flag bit zero is `FEE_FUNDING`; bit one is
`ALLOW_UNCONSTRAINED_STORED_DEBIT`; every other bit and both reserved bytes must
be zero. Exactly one signed source local term per fee rounding group carries the
fee-funding bit, and its later SettlementCapability mapping must mirror both
recognized bits. That term's
`maximum_protocol_fee` and `maximum_total_debit` bound collection. Each term
binds an exact endpoint, persistent 100-byte asset-binding digest, optional
required domain-descriptor identity, authority facts, and bounds. A nonzero
`required_domain_descriptor_digest_or_zero` must equal one exact authenticated
`domain_descriptor_digest` in the landed domain-execution set; zero imposes no
Core-level domain-membership restriction. This gives an intent an enforceable
opt-in counterparty-domain guard without binding permissionless matching to an
unknown future global table. It does not store an
execution-global asset or domain index, authorization slot,
settlement-capability index, account offset, fee-shard index, or witness kind.

The exact persistent credit constraint is:

```text
CreditConstraintCandidateV0 {                    // exactly 64 bytes
  constraint_index: u8
  credit_local_index: u8
  flags: u8
  reserved: [u8; 3]
  debit_source_bitmap: u16
  debit_group_root: [u8; 32]
  minimum_credit_numerator: u64
  nonzero_debit_denominator: u64
  terminal_absolute_minimum: u64
}

debit_group_root = H(
  "intent-debit-group-v0",
  u32_le(source_count),
  each strictly increasing unique local source index as one one-byte part
)

credit_constraints_root = H(
  "intent-credit-constraints-v0",
  u32_le(constraint_count),
  every complete 64-byte constraint in contiguous constraint-index order
)
```

Flags and reserved bytes are zero in this experiment. Every local index resolves
inside the same intent; a constraint cannot refer directly to a later global
settlement row. The bitmap is nonzero, uses only bits below the signed term
count, excludes the credit term itself, and reveals the exact source membership
needed to execute a stored intent. Core recomputes `debit_group_root` from its
strictly increasing set bits. A source may participate in more than one
constraint, allowing one debit to require several independent credits without a
single-pair bottleneck.

At stored activation, every intent-funded debit is exactly one of:

- present in at least one constraint bitmap with a positive numerator and valid
  credit term; or
- explicitly signed with `ALLOW_UNCONSTRAINED_STORED_DEBIT`.

Both at once fail canonicalization. The flag is invalid for direct or
exact-delegate witnesses, credit terms, domains, and Core-reserved fee
destinations. This keeps intentional grants and one-sided programmable flows
possible without turning an accidentally unconstrained reusable debit into an
implicit default.

Core derives the immutable terms root and identity exactly as follows:

```text
core_terms_root = H(
  "intent-core-terms-v0",
  u32_le(maximum_successful_fills),
  intent_local_capability_terms_root,
  credit_constraints_root
)

intent_digest = H(
  "intent-v0",
  Core program,
  u32_le(Core experimental major),
  market_binding_digest,
  engine_loader_state_snapshot_digest,
  fee_policy_digest,
  exact encoded 80-byte identity,
  core_terms_root
)
```

`intent_digest` is the unique immutable principal, stored-account identity,
canonical multi-intent sort key, and replay binding. Direct and exact-delegate
identities require
`maximum_successful_fills == 1`. A stored identity loads a nonzero immutable
maximum from its Core account.

For a direct or exact-delegate witness, Core reconstructs the complete local
term list from that slot's landed SettlementCapability rows: local indices are
contiguous, exact endpoint and asset/domain facts replace transient indices, and
the one-shot absolute minimum on each credit term is sufficient. The one-shot
fixture therefore has an empty cumulative-credit-constraint list. A
stored witness instead loads its exact persistent 136-byte terms and 64-byte
constraints first, then proves the landed rows are a complete one-to-one mapping
of those local records. No executor supplies an uncommitted Core term through
opaque engine bytes.

The local terms commit only the actor's exact funding sources, exact credit
recipients, any explicitly required domain-descriptor membership, asset
bindings, settlement-profile facts,
engine-debit ceilings, total source-debit ceilings including fees, fee-funding
relation, and credit constraints. They do not commit unknown future
counterparties or the final global settlement table. Core proves a complete,
one-to-one mapping from every persistent local term into the final table.
Binding every stored intent to that final table would make permissionless later
matching impossible; omitting local terms would permit recipient or funding
substitution.

After identity validation, Core normalizes every valid witness into a private
in-memory view:

```text
AuthorizationView {
  intent_digest
  actor
  core_terms_root
  canonical local-to-global capability mapping
  remaining engine debit by source
  remaining total source debit by source
  cumulative credit by recipient
  cumulative fee basis by assessment group
  cumulative assessed fee by assessment group
  fee ceilings by assessment group
  successful and remaining fill count
  expires_at_slot_exclusive
  fill_sequence
  status
}
```

The current normalized state and ordered set are committed exactly as:

```text
authorization_state_digest = H(
  "authorization-state-v0",
  intent_digest,
  lifecycle as one-byte part,
  u32_le(fill_sequence),
  u32_le(successful_fill_count),
  u32_le(remaining_fill_count),
  capability_state_root,
  fee_state_root,
  stored_authorization_key_or_zero
)

AuthorizationViewRowCandidateV0 {                // exactly 72 bytes
  authorization_slot: u8
  reserved: [u8; 7]
  intent_digest: [u8; 32]
  authorization_state_digest: [u8; 32]
}

authorization_view_set_digest = H(
  "authorization-view-set-v0",
  u32_le(intent_count),
  every complete 72-byte view row in contiguous slot order
)
```

The two mutable subroots have exact rows:

```text
CapabilityStateRowCandidateV0 {                 // exactly 88 bytes
  local_term_index: u8
  reserved_0: u8
  flags: u8
  reserved: [u8; 5]
  initial_maximum_engine_debit: u64
  initial_minimum_credit: u64
  initial_maximum_total_debit: u64
  remaining_total_debit: u64
  cumulative_engine_debit: u128
  cumulative_fee_debit: u128
  cumulative_credit: u128
}

capability_state_root = H(
  "authorization-capability-state-v0",
  u32_le(capability_state_count),
  every complete 88-byte row in contiguous local-term-index order
)

FeeStateRowCandidateV0 {                        // exactly 80 bytes
  rounding_group_digest: [u8; 32]
  funding_local_term_index: u8
  fee_class: u8
  flags: u8
  reserved: [u8; 5]
  cumulative_basis: u128
  cumulative_assessed_fee: u128
  maximum_fee: u64
}

fee_state_root = H(
  "authorization-fee-state-v0",
  u32_le(fee_state_count),
  every complete 80-byte row in strict rounding-group-digest order
)
```

Capability flag bit zero is `FEE_FUNDING`; bit one is the mirrored
`ALLOW_UNCONSTRAINED_STORED_DEBIT`; all other bits and every reserved byte,
including `reserved_0`, fail. Initial values
must equal the immutable 136-byte term, and each local index is unique and
contiguous. Fee-state flags and reserved bytes are zero; group digests are
unique, and each funding local term, fee class, and maximum fee must match the
immutable terms and authenticated policy.

Fee-state rows are not caller-supplied during staged creation. An active stored
authorization begins with zero fee-state rows. On the first authenticated use of
a `FEE_FUNDING` term, Core derives the only possible group from the intent
principal, the term's authenticated asset-binding preimage, fee class, and
policy revision, inserts a zero-before row, and applies the fill atomically.
Later fills must resolve that exact row. Existing plus newly derived rows are
stored in strict rounding-group-digest order; empty fixed-array slots are all
zero. Thus partial-fill rounding state remains persistent without asking a
creator to provide fee-group preimages that are absent from the 136-byte term.

Only a credit local-term row carries authoritative `cumulative_credit`;
non-credit rows keep it zero. Credit rows keep cumulative engine and fee debit
zero. Constraint bitmaps are the sole membership relation. For each immutable
credit constraint, Core resolves its authenticated bitmap, sums the referenced
source rows' cumulative engine debit with checked arithmetic, and enforces after
every committed prefix:

```text
credit_row.cumulative_credit
  >= ceil(group_cumulative_engine_debit * numerator / denominator)
```

Terminal state additionally requires the signed terminal absolute minimum and
credit-term minimum. A direct one-shot intent may use its absolute destination
minimum with an empty cumulative-constraint list. Multi-source-to-one-credit
vectors prove that one observed credit is not counted once per source.

Direct and exact-delegate views derive ephemeral rows with a zero stored-account
key, sequence zero, and one remaining fill. Stored accounts contain the exact
immutable 136-byte term and 64-byte constraint arrays plus these mutable rows;
none may contain an execution-global capability/domain/authorization index,
account offset, fee-shard index, or witness kind. Stored views use the exact
writable account key and landed pre-execution state. Lifecycle values outside
the explicit stored state machine fail closed.

Witness kind is excluded from this normalized view. The field names are
explanatory, not an accepted account layout. Core derives
`authorization_view_set_digest` from the views in canonical authorization-slot
order.
The engine sees only `intent_set_digest`, `protected_execution_root`, and the
context required for its economics; it receives no direct, delegate, stored,
partial, order, or auction discriminator.

The composite protected root is:

```text
protected_execution_root = H(
  "protected-execution-v0",
  Core program,
  u32_le(Core experimental major),
  market_binding_digest,
  engine_loader_state_snapshot_digest,
  domain_set_digest,
  intent_set_digest,
  fee_policy_digest,
  asset_set_digest,
  authorization_view_set_digest,
  fee_shard_set_digest,
  protected_capability_set_digest
)
```

This root binds both current replay state and the exact landing-time protected
closure. The two final digest parts are distinct list roots over the exact
256-byte fee-shard rows and 368-byte protected-capability rows. Every callback,
request, receipt, fee identity, and evidence record uses this composite root
rather than any one table root alone.

### Direct authorization

A direct actor authorizes exact capabilities and bounds in the current Core
invocation. The actor and writable user asset accounts remain in the protected
Core plane and are never forwarded to the engine. Core constructs an ephemeral
`AuthorizationView`; no engine byte indicates whether a wallet or program actor
authorized it.

Core accepts exactly two non-overlapping direct-authority shapes:

1. An on-curve wallet actor is valid only when `get_stack_height()` is the
   transaction-level stack height and the fixed read-only Instructions sysvar
   reports the current top-level instruction as this exact Core program, exact
   landed instruction data, exact ordered account keys and requested
   privileges, and the exact actor meta as a signer.
2. An off-curve program actor is valid only in one direct CPI from the current
   top-level parent: Core's stack height is exactly transaction level plus one,
   the Instructions sysvar's current top-level program is not Core, the actor
   appears in that parent's effective account metas, and the actor AccountInfo
   received by Core is a signer. The actor must not equal the callback authority
   or any Core-derived spend, execution, accounting, fee, or control authority
   participating in the invocation. At this exact depth, an off-curve signer
   could only have been produced by the immediate top-level program's
   `invoke_signed`, so that program is the actor's authorization policy.

Solana's Instructions sysvar does not expose CPI instruction bytes or an
authenticated immediate-caller record. Core therefore never describes the
program-actor branch as an exact landed-CPI-byte proof. Any nested router,
inherited off-curve signer, callback reentry, on-curve signer through CPI, or
arbitrary deeper call fails before the engine runs. Multi-hop routing uses an
exact delegate or stored authorization; a future multi-hop program-actor path
requires a distinct authenticated adapter protocol rather than weakening this
rule.

The same dual wallet/program-actor rule applies to Core instructions that
create, write, activate, cancel, or replace a stored authorization and to the
canonical Core delegate-approval helper when they rely on an actor. Direct
authorization is authorization for one invocation, not a stateful one-shot
nonce claim.

The experiment may also retain the predecessor's exact one-shot classic-SPL
delegate as a direct or routed one-shot fixture. Delegate amount alone is replay
state, not semantic authorization. Core must first recompute the immutable intent
digest and derive one source-specific spend address:

```text
intent_spend_seed = H(
  "intent-spend-seed-v0",
  intent_digest,
  exact source token account
)

intent_spend_authority = PDA(
  experimental Core program,
  "intent-spend-v0",
  intent_spend_seed
)
```

The canonical approval helper has discriminator `04cf33c35d503375`
and exactly 40 argument bytes: `intent_digest: [u8; 32]` followed by nonzero
`amount: u64`. It rejects trailing data and proves the complete direct wallet or
program-actor call before issuing Classic SPL `ApproveChecked`.

For every delegated source, the token owner must equal the intent actor, the
delegate must equal that source's exact intent-spend authority, its positive
allowance must equal that source's observed engine debit plus Core-derived fee
debit, and success must leave `delegate = None` and `delegated_amount = 0` on
every source. All sources attributed to one exact-delegate intent must resolve
the same actor. A declared maximum, generic delegate, leftover allowance,
variable-amount debit, partial fill, zero-debit execution, or consumption spread
over a later fill is rejected. Reapproving the same source-specific authority is
an explicit reauthorization of the same still-unexpired intent; clients use a
fresh nonce when old queued execution must remain dead.

### Stored authorization

A user-created Core authorization account separates immutable identity and Core
terms from mutable execution state. Its candidate address is:

```text
stored_authorization = PDA(
  experimental Core program,
  "stored-authorization-v0",
  intent_digest
)
```

The 32-byte intent digest is the one identity seed and is independent of mutable
fill state. A maximum-size authorization cannot carry all 12 terms and 12
constraints in one Solana transaction, so creation is an explicit staged state
machine rather than an impossible one-shot ABI. Initialization validates a
pre-funded PDA safely rather than assuming it has zero lamports. Its account
payload is exactly 4,776 bytes, or 4,784 bytes including the account
discriminator:

```text
StoredAuthorizationHeaderCandidateV0 {          // exactly 16 bytes
  wire_version: u8
  lifecycle: u8
  bump: u8
  term_count: u8
  constraint_count: u8
  fee_state_count: u8
  flags: u8
  reserved: u8
  term_written_bitmap: u16
  constraint_written_bitmap: u16
  fill_sequence: u32
}

StoredAuthorizationCandidateV0 payload =
  header                                         16
  explicit Core-owned identity                  312
  pending_execution_digest                       32
  IntentCapabilityTermCandidateV0[12]          1,632
  CreditConstraintCandidateV0[12]                768
  CapabilityStateRowCandidateV0[12]            1,056
  FeeStateRowCandidateV0[12]                     960
                                                -----
                                                4,776 bytes
```

The exact 312-byte identity and 4,776-byte payload are one Core-owned account
storage codec, not a second shared request-wire codec. The private Wire crate
owns the 16-byte header, immutable row codecs, control arguments, and security
hash preimages; Core alone owns the Anchor discriminator, fixed-array storage,
lifecycle mutation, and exact 4,784-byte account serializer. Golden offset,
round-trip, and trailing-byte tests prove the composition without maintaining a
duplicate mutable-state serializer in two crates.

The explicit 312-byte identity retains the Core program and experimental major
even though the PDA owner is also checked; cross-program data cannot be mistaken
for this Core's identity. Unused fixed-array rows are all zero. Header flags and
reserved bytes are zero. Draft bitmaps may contain only expected indices; an
active or later account has exactly the complete masks implied by its counts.

The control ABI is frozen independently from execution:

```text
initialize_stored_authorization discriminator = 76987db8b7400e4e
write_stored_authorization_chunk discriminator = bb97761e70f00ad6
activate_stored_authorization discriminator = 914d2e6337527a33
replace_stored_authorization discriminator = 5f1f92773ed93c7d
cancel_stored_authorization discriminator = 5b1eda991f5246e7

InitializeStoredAuthorizationArgsCandidateV0 {  // exactly 312 bytes
  wire_version: u8
  term_count: u8
  constraint_count: u8
  flags: u8
  maximum_successful_fills: u32
  identity: InlineIntentIdentityRowCandidateV0  // 80 bytes
  market_binding_digest: [u8; 32]
  engine_loader_state_snapshot_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
  intent_capability_terms_root: [u8; 32]
  credit_constraints_root: [u8; 32]
  core_terms_root: [u8; 32]
  intent_digest: [u8; 32]
}

StoredAuthorizationChunkHeaderCandidateV0 {     // exactly 8 bytes
  wire_version: u8
  chunk_kind: u8       // 0 = 136-byte term, 1 = 64-byte constraint
  start_index: u8
  row_count: u8        // 1..4
  reserved: [u8; 4]
}
```

Initialize recomputes every supplied root relationship and creates only a
non-executable `Draft`. Every chunk instruction requires the same exact direct
wallet or program-actor authority defined above, has exact length, writes one to
four contiguous previously unwritten rows, and rejects overlap, gaps outside the
declared count,
unknown kinds, or nonzero padding. Activation likewise requires the actor,
requires both bitmaps complete, recomputes all row roots and the intent digest,
derives every initial capability-state row from the immutable term, and only
then changes `Draft` to `Active`. The caller never supplies initial mutable
capability or fee state.

Initialization separates authority from rent funding. Its actor is the identity
committed by the intent and satisfies the dual wallet/program-actor rule; a
distinct writable signer may pay account creation rent without gaining any
authorization right. The payer and actor may be the same wallet only through
the explicitly validated duplicate-key privilege union at transaction root.
Solana's checked Instructions-sysvar reconstruction exposes the resolved global
signer/writable privileges, not the original lower privilege requested at each
duplicate occurrence. The same wallet therefore appears signer+writable in both
payer and actor positions, and Core accepts that exact effective shape without
ever writing through the actor role. A program actor cannot use this alias and
no other control-account alias is accepted. This keeps program-PDA actors and
sponsored creation usable without treating payment as intent authority or
claiming an unobservable per-position privilege distinction.

The account then tracks:

```text
immutable
  exact 80-byte intent identity and intent_digest
  market binding, loader-state snapshot, fee policy, and core_terms_root
  exact ordered IntentCapabilityTerm and CreditConstraint rows
  maximum_successful_fills

mutable
  status = Draft | Active | Executing | Cancelled | Consumed
  fill_sequence, which is also successful_fill_count
  remaining engine debit by source
  remaining total source debit by source
  cumulative credit by recipient
  cumulative fee basis and assessed fee by intent-principal group
  cumulative total fee debit
  pending_execution_digest or zero
```

Lifecycle bytes are exact: `Draft = 0`, `Active = 1`, `Executing = 2`,
`Consumed = 3`, and `Cancelled = 4`. The pending digest is nonzero if and only
if lifecycle is `Executing`; execution accepts only `Active`.

Every counter increment and amount transition is checked. The accepted private
state machine is:

```text
Uninitialized -> Draft
Draft -> Active | Cancelled
Active -> Executing(pending_execution_digest)
Executing -> Active(next fill sequence) | Consumed
Active -> Cancelled
```

Before the untrusted callback, Core validates every participating authorization
against the caller's expected sequence and the recomputed pre-state view, then
computes the canonical engine request and marks all of them `Executing` with
`pending_execution_digest = request_digest`. Core serializes every participating
state transition before the untrusted CPI; an in-memory-only flag is
insufficient against reentry. After return, the pending digest must remain
byte-identical. Core does
not decrement amounts or advance cumulative fee state before the callback,
because the engine has not returned the actual moves. After the receipt, Core
derives exact per-authorization moves and fees from the immutable pre-state,
checks every bound and cumulative predicate, then applies all checked updates and
clears the pending digest. Any later failure rolls the entire transaction back to
the exact pre-execution state. `Executing` rejects semantic reentry independently
of current runtime stack behavior.

Every successful stored execution increments `fill_sequence`, consumes one
bounded successful-fill count, and either leaves a valid `Active` state or enters
`Consumed`. This includes an explicitly authorized zero-protected-debit engine
transition; without a finite fill count, such a transition would have no replay
boundary. Cancellation, expiry, exhaustion, wrong sequence, non-`Active` status,
or terminal replay fails before the engine callback.

Each participating authorization must receive or fund at least one non-fee
protected delta attributed to its slot. A zero-debit but credit-bearing
authorization is valid and still consumes one successful-fill count; a pure
zero-effect participant is rejected. This prevents an executor from burning an
unrelated stored authorization's fills by inserting it as a no-op participant.

Creation, chunk writing, activation, cancellation, and replacement are
direct wallet- or program-actor-authorized Core instructions. A fill and
cancellation serialize on the same writable
authorization account: cancellation first makes the fill fail; fill first may
commit before cancellation stops only the remainder. The protocol promises no
cancellation priority over a transaction that lands first. Replacement never
mutates immutable terms in place. The replacement instruction accepts only a
complete new `Draft`, then atomically cancels the old `Active` authorization and
activates the different `intent_digest`, normally created with a fresh nonce or
different engine-terms commitment. Both accounts must commit the same exact
actor, supplied once under the dual wallet/program-actor rule. Replacement is
not an authority-transfer or two-party novation primitive. A different actor
creates its own authorization and the old actor cancels separately; any future
atomic cross-actor novation requires a distinct, explicit consent protocol.
Cancelled and consumed accounts remain tombstones for this experiment and
cannot be closed or recreated at sequence zero.

A token delegate may still provide the asset-program authority needed to execute
a stored classic-SPL debit. Each such source supplies the same exact
source-specific intent-spend PDA control used by one-shot delegation. For an
exact-delegate witness, success consumes the complete positive allowance. For a
stored witness, an allowance may remain across fills, but Core signs only while
the exact stored authorization is `Active` and within its remaining counters;
cancellation, consumption, expiry, or wrong sequence prevents later signing.
Delegate state alone is never replay or semantic authorization.

### Product-neutral partial-fill credit constraint

`remaining minimum credit` by itself is ambiguous under partial fills. The
private experiment instead binds one or more authorization-level cumulative
inequalities equivalent to:

```text
cumulative_credit_after >= ceil(
  cumulative_debit_after * minimum_credit_numerator / debit_denominator
)
```

Here `cumulative_debit_after` is the sum over the exact local source indices in
the constraint's `debit_group_root`, and `cumulative_credit_after` belongs to its
exact `credit_local_index`. The constraint never accepts a global capability
index chosen by a later executor.

Core evaluates every accepted inequality after each proposed fill using checked
`u128` cumulative amounts and exact unsigned 256-bit multiplication before
ceiling division and checked downcast. A zero denominator, overflow, or unknown
constraint form fails closed. Core requires every terminal absolute minimum
before entering `Consumed`. These are protected authority inequalities over
capability deltas, not a price, swap, order, auction, or product enum. A zero
numerator is valid only when the signed terminal minimum and successful-fill
bound make the intended zero-credit behavior explicit.

### Multi-intent authorization

One settlement may combine several direct, exact-delegate, or stored intents.
Core sorts them lexicographically by `intent_digest`, requires
`authorization_slot` to equal the resulting position, rejects a duplicate
digest, stored account, or protected public key, and derives:

```text
IntentSetRowCandidateV0 {                        // exactly 32 bytes
  intent_digest: [u8; 32]
}

intent_set_digest = H(
  "intent-set-v0",
  domain_set_digest,
  u32_le(intent_count),
  each complete 32-byte row as a separate part in strict digest order
)
```

Every debit and required credit is attributed to one exact authorization slot.
Fees are attributed to the source capability and user that fund them. One user's
surplus or ceiling cannot satisfy another user's deficit or fee.

The first experiment rejects the same protected public key appearing in multiple
authorization slots in one envelope. Supporting intentional aggregation over one
funding account is a later alias and attribution decision.

### Authorization-neutrality test

The same semantic fixture is executed from cloned state through:

- one direct authorization;
- one stored authorization;
- one partially filled stored authorization; and
- a multi-intent set.

After normalizing for intended participant count and fill amount, all paths use
the same engine request type, callback prefix, move rows, Core move validator,
protected asset profile, and event evidence classes. If the effect format gains
`direct`, `order`, `partial`, `auction`, or other authorization/product fields,
the candidate boundary is rejected.

## Candidate protected effect algebra

The engine returns an ordered list:

```text
MoveCandidateV0 {
  source_capability_index: u8
  destination_capability_index: u8
  amount: u64 little-endian
}
```

Each row is exactly 10 bytes. Core applies the following normal form before any
protected movement:

1. `amount` is nonzero.
2. Both indices are in range and distinct.
3. Source and destination select the same exact asset identity, asset program,
   and settlement profile.
4. Source has debit rights; destination has credit rights.
5. The engine cannot reference a Core-reserved fee destination.
6. Rows are strictly increasing by `(source_index, destination_index)`.
7. Duplicate pairs are rejected rather than silently aggregated.
8. One capability may appear on only one side of the engine graph: source or
   destination, never both.
9. Checked `u128` aggregation derives gross debit and gross credit for every
   capability and principal-keyed fee group.
10. For every `(asset identity, asset program, settlement profile)`, aggregate
    debit equals aggregate credit.
11. Every user's engine-debit ceiling, total source-debit ceiling after the
    Core-derived fee, cumulative credit inequality, terminal minimum, and fee
    ceiling holds independently.
12. Every domain debit is covered by its own accounted balance and admission;
    every domain's accounted change is derived only from that domain's local
    capability deltas.

After those checks, Core derives:

```text
canonical_effect_digest = H(
  "canonical-effect-v0",
  request_digest,
  protected_execution_root,
  u32_le(move_count),
  every complete 10-byte MoveCandidateV0 in canonical order
)
```

The source-or-destination normal form removes cycles and makes the observed
classic-SPL gross debit equal to the canonical plan debit. An engine can express
the same final fungible allocation by netting an intermediate account before it
returns the graph. If the gross intermediate path itself affects transfer hooks,
fees, rights, or product meaning, this first protected profile does not support
that behavior.

For every domain asset slot, Core derives:

```text
accounted_after = accounted_before + local_credits - local_debits
```

with checked arithmetic. Global conservation does not authorize a local debit.
Raw balances must cover accounted balances before and after settlement. Raw
donations do not change accounting.

The first Core-custody authority is exact and domain-local:

```text
domain_accounting = PDA(
  experimental Core program,
  "domain-accounting-v0",
  exact domain descriptor account key
)
```

The stable descriptor-key seed intentionally survives descriptor revisions;
the accounting account still stores and must match the current descriptor key,
revision, PDA, and bump. That same Core-owned writable accounting control is the
Classic-SPL transfer-authority `AccountInfo`; Core signs only the accepted
settlement transfers with its exact PDA seeds. No separate custody authority is
passed to the engine. Under this first profile, every domain-accounted endpoint
is initialized non-native Classic SPL, has the accounting PDA as token owner,
and has neither delegate nor close authority. Other custody models require a
separate authority profile and hostile tests.

Core executes each accepted classic-SPL move through one exact
`TransferChecked` call under its source capability's validated authority. Core
reloads all affected accounts and verifies exact aggregate source debits and
destination credits. No engine receipt can substitute for those observations.

## Protocol-fee algebra

The engine does not return protocol-fee moves. Core derives the mandatory
assessment once from the authenticated Core fee policy and the canonical pre-fee
engine graph, then appends reserved fee moves before checking final user bounds.

The first experiment supports exactly one fee fact Core can observe objectively:
a rate over exact protected gross-debit assessment groups. A flat or fixed
per-envelope fee is deliberately disabled. Without a separate sponsor
authorization bound to the complete envelope, a caller could choose one
participant to subsidize another or repeatedly include a no-op authorization as
the payer. Flat fees require a later authorization decision and are unknown fee
classes in this experiment.

The authenticated policy row and digest are exact:

```text
FeePolicyRowCandidateV0 {                        // exactly 32 bytes
  wire_version: u8
  rounding_mode: u8
  flags: u8
  reserved: [u8; 5]
  revision: u64
  rate_numerator: u64
  nonzero_denominator: u64
}

fee_policy_digest = H(
  "fee-policy-v0",
  Core program,
  u32_le(Core experimental major),
  exact encoded 32-byte policy row
)
```

Flags and reserved bytes are zero. The only accepted rounding modes are the
explicit floor and ceiling values exercised below. This accepted rate profile
requires `0 < rate_numerator <= nonzero_denominator`; a later policy may choose
another bounded range only through a new explicit profile.

A rate assessment first derives the immutable fee principal:

```text
fee_principal_digest = H("fee-principal-v0", actor, intent_digest)
```

Its rounding group is keyed by exactly the principal and objective policy facts:

```text
FeeRoundingGroupRowCandidateV0 {                 // exactly 176 bytes
  fee_principal_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
  fee_class: u8
  reserved: [u8; 7]
  fee_policy_revision: u64
}

assessment_group_digest = H(
  "fee-rounding-group-v0",
  exact encoded 176-byte rounding-group row
)

collection_relation_digest = H(
  "fee-collection-v0",
  assessment_group_digest,
  designated_funding_endpoint_key,
  u64_le(maximum_protocol_fee),
  u64_le(maximum_total_debit),
  fee_shard_index as one-byte part,
  u64_le(fee_delta)
)
```

The rounding-group reserved bytes are zero.

Endpoint key, local or global capability index, authorization slot, fee-shard
index, and account offset are not part of the rounding-group identity. Basis is
aggregated across every source capability in the group, so splitting one
principal's basis across source accounts cannot reset floor or ceiling rounding.
Every intent-funded debit in this first profile has the gross-debit-rate fee
class; a caller cannot mark a user source `NONE` to remove it from basis. For
each declared principal/asset/program/profile/policy group, exactly one signed
local source term and its mapped settlement capability carries the fee-funding
flag, a valid shard, and positive `maximum_protocol_fee`. Other sources in the
same group retain the rate class but have no shard, zero fee ceiling, and
`maximum_total_debit == maximum_engine_debit`. The funding source may set a
tighter combined total than the sum of its independent maxima: Core requires
`maximum_total_debit >= maximum_engine_debit`,
`maximum_protocol_fee <= maximum_total_debit`, actual fee within the fee ceiling,
and actual engine debit plus fee within the total ceiling. The collection
relation is validated separately from the basis key. It intentionally omits
global capability and authorization indices; the assessment also binds
`protected_execution_root`, which commits the signed local mapping and exact
shard and vault closure.

The basis excludes every protocol-fee move. No engine verb, product label,
caller flag, receipt claim, spread, reserve growth, auction surplus, or opaque
state change can make a leg assessable or exempt.

Within one envelope Core aggregates the complete principal-keyed group before
rounding:

```text
basis = sum(canonical pre-fee gross debits in the assessment group)
fee = R_policy(basis * rate / denominator)
```

For the private experiment, `basis` and cumulative basis are checked `u128`,
`rate` and the nonzero `denominator` are unsigned 64-bit policy facts, and the
multiplication/division model is exact unsigned 256-bit arithmetic before a
checked downcast. `R_policy` is exactly either floor or ceiling as committed by
the authenticated policy revision:

```text
floor(n / d) = quotient
ceil(n / d)  = quotient + (remainder != 0)
```

Unknown rounding modes, a zero denominator, overflow, a fee that does not fit
the protected amount type, or a cumulative basis decrease fail closed. The
fixture matrix exercises both accepted rounding modes; a product decision may
later narrow this to one.

Per-leg rounding is forbidden. Per-leg floor permits dust splitting; per-leg
ceiling charges different fees for equivalent split graphs. Net-basis charging
is also forbidden for this profile.

For a stored or partially filled intent, the authorization account stores both
the cumulative basis and cumulative assessed fee for each group. On entry Core
first requires:

```text
cumulative_assessed_before
  == R_policy(cumulative_basis_before * rate / denominator)
```

The incremental assessment is then:

```text
fee_delta = R_policy(
              (cumulative_basis_before + fill_basis) * rate / denominator
            )
          - R_policy(cumulative_basis_before * rate / denominator)
```

Both terms use checked wide arithmetic and the same exact rounding function.
This makes equivalent fill partitions produce the same cumulative rate fee.
Core stores both new cumulative values atomically with the fill. Cumulative fee
debit must remain within the principal's signed fee and total source-debit
ceilings.

Every fee assessment has this exact private identity:

```text
assessment_digest = H(
  "fee-assessment-v0",
  Core program,
  u32_le(Core experimental major),
  market_binding_digest,
  fee_policy_digest,
  u64_le(fee_policy_revision),
  intent_set_digest,
  protected_execution_root,
  canonical_effect_digest,
  assessment_group_digest,
  collection_relation_digest,
  u32_le(fill_sequence),
  u128_le(cumulative_basis_before),
  u128_le(fill_basis),
  u128_le(cumulative_basis_after),
  u64_le(fee_delta)
)

FeeAssessmentSetRowCandidateV0 {                 // exactly 64 bytes
  assessment_group_digest: [u8; 32]
  assessment_digest: [u8; 32]
}

fee_assessment_set_digest = H(
  "fee-assessment-set-v0",
  u32_le(assessment_count),
  every complete 64-byte row sorted by assessment_group_digest
)
```

Group digests are unique, and ties or duplicate rows fail. `fill_sequence` is
the principal's pre-execution sequence; it is zero for direct and exact-delegate
one-shot execution.

Exactly-once charging comes from the atomic monotonic stored-authorization state
or complete exact-delegate consumption, not an unbounded fee-ID registry in a
liability ledger. Each fee move uses the group's exact authorized funding
capability, policy-derived recipient shard, and user ceiling.

The first rate fixture is payable in its basis asset. Cross-asset notional
conversion, oracle pricing, flat fees, and a universal percentage of an
engine-defined trade are non-goals.

Every accepted private fee shard has all three protected components:

```text
FeeShardDescriptorCandidateV0   // authenticated read-only control account
FeeLiabilityLedgerCandidateV0   // Core-owned writable control account
exact fee-vault capability      // writable settlement endpoint
```

The descriptor binds fee policy revision, shard index, asset and settlement
profile, vault, liability ledger, and recipient policy. The FeeShard wire row
binds the descriptor and ledger offsets to the exact reserved fee-vault
capability. None is supplied to the engine. Missing, aliased, reordered,
read-only liability, wrong-vault, wrong-asset, or wrong-policy components fail
before callback.

Its self-digest is typed rather than an arbitrary stored label:

```text
FeeShardDescriptorRowCandidateV0 {              // exactly 272 bytes
  wire_version: u8
  shard_index: u8
  reserved: [u8; 6]
  market_binding_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
  fee_policy_revision: u64
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
  vault: [u8; 32]
  liability_ledger: [u8; 32]
  recipient_policy_digest: [u8; 32]
}

exact_fee_recipient_policy_digest = H(
  "exact-fee-recipient-v0",
  Core program,
  u32_le(Core experimental major),
  market_binding_digest,
  vault,
  asset_identity,
  asset_program,
  settlement_profile_digest
)

fee_shard_descriptor_digest = H(
  "fee-shard-descriptor-v0",
  Core program,
  u32_le(Core experimental major),
  complete 272-byte descriptor row
)

fee_shard_descriptor = PDA(
  experimental Core program,
  "fee-shard-v0",
  market_binding_digest,
  shard_index as one byte
)
```

The stored bump and self-digest are excluded from the row and recomputed. The
first recipient policy binds the exact destination and asset facts, not the
vault's mutable token-owner authority. The vault must still be an initialized,
non-native classic-SPL account for the exact asset, and its current lifecycle is
committed in the protected-capability row. This policy cannot redirect a payer's
fee to another account.

Core derives the exact landing-time row and list digest:

```text
FeeShardDigestRowCandidateV0 {                   // exactly 256 bytes
  shard_index: u8
  asset_index: u8
  vault_settlement_capability_index: u8
  flags: u8
  reserved: [u8; 4]
  descriptor_key: [u8; 32]
  descriptor_digest: [u8; 32]
  liability_key: [u8; 32]
  vault_key: [u8; 32]
  asset_binding_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
  recipient_policy_digest: [u8; 32]
  fee_policy_revision: u64
  liability_before: u128
}

fee_shard_set_digest = H(
  "fee-shard-set-v0",
  u32_le(fee_shard_count),
  every complete 256-byte row in contiguous shard-index order
)
```

The prefix is eight bytes, followed by seven 32-byte fields, one little-endian
`u64`, and one little-endian `u128`. Flags and reserved bytes are zero.
Descriptor and liability PDAs are partitioned by `market_binding_digest`, local
shard index, asset binding, and fee policy. Ordinary execution therefore writes
only market-local liability shards; no unrelated market shares a global
writable fee lock.

After fee transfers and exact balance reload, Core applies only:

```text
liability_after = liability_before + observed_net_fee_vault_credit
```

with checked wide arithmetic. Only that exact Core-created, observed net credit
creates liability. A pre-existing balance or donation does not. Fee claims,
recipient withdrawals, and liability reduction are separate from this
experiment. A later transfer-fee profile must distinguish gross source debit,
issuer-withheld amount, net destination credit, and accounted revenue.

If one protected capability profile can emulate another profile's effects, its
mandatory fee floor cannot be selected merely because the engine gives the
action a cheaper name. Fee classes are keyed only to objective authority and
effect facts.

The strongest revenue claim in this experiment is a mandatory rate on exact
Core-observed protected gross debits. A zero-debit engine transition has zero
assessable basis. An engine may batch semantic actions or expose a route outside
Core; this experiment does not claim a fee on unknowable off-Core behavior.

## Callback and phase binding

The experiment uses only the selected single writable transition before
settlement. There is no post-settlement engine callback.

The opaque engine payload is bound without interpretation by Core:

```text
payload_digest = H("payload-v0", exact payload bytes)
```

The callback PDA is derived from a domain-separated digest equivalent to:

```text
callback_seed = H(
  "callback-seed-v0",
  Core program,
  Core experimental major,
  selected engine program,
  engine_interface_id,
  engine_instance_id,
  engine_loader_state_snapshot_digest,
  market_binding_digest,
  intent_set_digest,
  domain_set_digest,
  protected_execution_root,
  opaque_capability_root,
  payload_digest,
  TRANSITION phase
)
```

The digest is reduced to one fixed 32-byte seed so a variable domain or account
set does not become a variable PDA seed list. The callback address itself is not
inside the digest, avoiding a hash/address cycle.

The callback account is read-only and non-executable in the Core instruction and
is signed only inside the exact Core-to-selected-engine CPI. The engine may
forward it to opaque programs. No Core instruction, protected profile, admission
rule, fee path, claim path, or administrative path accepts it as authority.

Core authenticates engine return data immediately after the callback. The
selected engine must set its receipt after its final nested CPI. A missing value,
wrong setter, descendant setter, malformed bytes, unsupported version, wrong
phase, wrong request binding, or trailing byte fails closed.

Any later Core, token, fee, accounting, postcondition, or compute failure rolls
back the earlier engine and opaque mutations. Token-2022 transfer hooks would
introduce a later untrusted callback graph and are outside this profile.

## Private wire candidates

Every encoding in this section is experiment-local. All integers are
little-endian. Every reserved byte must be zero. Decoders require exact lengths
and reject unknown flags, versions, padding, and trailing bytes.

### Top-level execution envelope

The private top-level header is:

```text
ExecuteEnvelopeHeaderCandidateV0 {
  wire_version: u8
  loader_policy_account_count: u8
  domain_control_account_count: u8
  authorization_account_count: u8
  protected_profile_account_count: u8
  fee_control_account_count: u8
  settlement_capability_count: u8
  opaque_capability_count: u8
  domain_count: u8
  intent_count: u8
  inline_intent_row_count: u8
  asset_count: u8
  fee_shard_count: u8
  authorization_snapshot_row_count: u8
  maximum_engine_moves: u8
  flags: u8
  payload_len: u16
  reserved_0: [u8; 6]
  expires_at_slot_exclusive: u64
  expected_engine_sequence: u64
  intent_set_digest: [u8; 32]
  domain_set_digest: [u8; 32]
  protected_execution_root: [u8; 32]
  expected_opaque_capability_root: [u8; 32]
  fee_policy_digest: [u8; 32]
  expected_engine_loader_state_snapshot_digest: [u8; 32]
  payload_digest: [u8; 32]
}
```

The header is exactly 264 bytes. `flags` and every reserved byte must be zero.
Its exclusive expiry may shorten execution freshness but may not extend any
participating intent's exclusive expiry.

The byte previously considered for a caller-declared engine-context count is
`authorization_snapshot_row_count`; Core derives the later engine
`context_row_count` from validated capabilities, so the caller cannot create a
second classification of the protected table.

It is followed, in this exact order, by:

```text
DomainControlRowCandidateV0[domain_count]
AuthorizationSnapshotRowCandidateV0[authorization_snapshot_row_count]
InlineIntentIdentityRowCandidateV0[inline_intent_row_count]
FeeShardRowCandidateV0[fee_shard_count]
SettlementCapabilityRowCandidateV0[settlement_capability_count]
payload[payload_len]
```

The private declaration rows are:

```text
DomainControlRowCandidateV0 {                    // exactly 8 bytes
  descriptor_control_offset: u8
  admission_control_offset_or_none: u8
  accounting_control_offset: u8
  flags: u8
  reserved: [u8; 4]
}

AuthorizationSnapshotRowCandidateV0 {            // exactly 8 bytes
  authorization_slot: u8
  witness_kind: u8
  authorization_control_offset_or_none: u8
  inline_identity_index_or_none: u8
  expected_fill_sequence: u32
}

InlineIntentIdentityRowCandidateV0 {             // exactly 80 bytes
  actor: [u8; 32]
  engine_terms_commitment: [u8; 32]
  authorization_nonce: u64
  expires_at_slot_exclusive: u64
}

FeeShardRowCandidateV0 {                         // exactly 8 bytes
  descriptor_control_offset: u8
  liability_control_offset: u8
  vault_settlement_capability_index: u8
  asset_index: u8
  flags: u8
  reserved: [u8; 3]
}

SettlementCapabilityRowCandidateV0 {             // exactly 48 bytes
  asset_index: u8
  domain_index_or_none: u8
  authorization_slot_or_none: u8
  intent_local_term_index_or_none: u8
  authority_class: u8
  fee_shard_index_or_none: u8
  fee_class: u8
  flags: u8
  rights_bits: u16
  domain_accounting_slot_or_none: u8
  spend_authority_control_offset_or_none: u8
  reserved_0: u8
  reserved: [u8; 3]
  maximum_engine_debit: u64
  maximum_total_debit: u64
  minimum_credit: u64
  maximum_protocol_fee: u64
}
```

`255` is the only absent-index sentinel. Domain rows and fee rows are in their
canonical index order. `authorization_snapshot_row_count` must equal
`intent_count`; snapshot rows are strictly increasing from authorization slot
zero and therefore cover the complete half-open range `[0, intent_count)`.
Inline identity rows are ordered by the authorization slot that references them;
permuting two otherwise valid rows changes the wire and fails canonicalization.
Private witness kinds `0`, `1`, and `2` mean direct actor signer, exact one-shot
delegate, and stored authorization. Kinds `0` and `1` each reference one inline
identity row; kind `2` uses `255` and loads its immutable identity and Core terms
from one exact Core-owned authorization-control account. Every inline identity
row is referenced exactly once, and its engine-terms commitment is nonzero.
Settlement row position is both its capability index and the relative position
of its endpoint in the settlement-account segment.

Core resolves every identity and terms root, derives each `intent_digest`, sorts
by digest, and requires the resulting position to equal the declared
authorization slot. Direct snapshots point to the exact actor signer in the
authorization-control segment; the same actor control may authorize multiple
distinct intent digests. Stored snapshots point to their exact writable
authorization account. Exact-delegate snapshots set the authorization-control
offset to `255`; their source rows instead name the exact source-specific spend
authority controls. Direct and exact-delegate snapshots require
`expected_fill_sequence == 0`; stored snapshots compare the supplied value to
current state. A stored sequence increment is checked and reaching the private
`u32` ceiling makes the authorization terminal rather than wrapping.
Private fee classes `0` and `1` mean no assessment and principal-keyed protected
gross-debit rate. Settlement flag bit zero is the fee-funding relation and bit
one is the stored-only explicit unconstrained-debit relation; unknown bits fail.
`intent_local_term_index_or_none` proves the exact persistent-to-global
mapping. The domain accounting slot and source spend-authority offset are
present only when their declared role requires them and are otherwise `255`.
For a domain-accounted capability, `domain_index_or_none` identifies the domain
whose local accounting authority is exercised. For an intent-funded debit or
exact external credit, that same field is only an optional required-domain
predicate: `255` reconstructs a zero required-domain digest, while a present
index reconstructs the authenticated descriptor digest at that index. It grants
no domain rights and the domain-accounting slot remains `255`. A stored term's
nonzero `required_domain_descriptor_digest_or_zero` must map through this field
to that exact descriptor; a zero term requires `255`. A Core-reserved fee row
always uses `255`. Thus direct, exact-delegate, and stored intents can all bind a
domain without placing an execution-global index in persistent identity or
inferring policy from opaque engine bytes.

Credit-constraint bitmaps, resolved through the local-to-global mapping, are the
only debit-to-credit relation; `reserved_0` and the other reserved bytes are
zero. Unused dependent amounts are zero. Unknown witness kinds,
authority classes, rights, fee classes, flags, nonzero reserved bytes,
overlapping control offsets outside explicit same-direct-actor reuse,
unreferenced rows or control accounts, and out-of-range indices fail before
callback. A flat or fixed-envelope fee class is unknown in this experiment.

Core never guesses a capability role, authorization slot, domain, fee class, or
bound from an account owner or position. It decodes the row, then proves every
declared fact from the exact account, mint, domain, fee policy, and authorization
state. `protected_execution_root` commits the normalized
`authorization_view_set_digest`, exact 256-byte fee-shard digest rows, exact
368-byte protected-capability digest rows, all list lengths, and validated
landing-time endpoint and fee-control facts.

One Core-owned `StoredAuthorizationCandidateV0` contains the immutable identity
and mutable tombstone state defined above. Core parses these exact tagged fields;
it does not infer them from token accounts. The instruction snapshot supplies
only the expected mutable sequence and account mapping. Every authorization
control account is consumed by exactly one resolved snapshot, except that one
direct actor account may be the authenticated actor for more than one otherwise
distinct intent.

The full instruction-data length is therefore exactly:

```text
8 + 264 + 8*domain_count + 8*authorization_snapshot_row_count
        + 80*inline_intent_row_count
        + 8*fee_shard_count + 48*settlement_capability_count
        + payload_len
```

At the independent private encoding maxima of four domains, eight authorization
snapshots, four inline identities, four fee shards, 12 settlement capabilities,
and a 128-byte payload, this is 1,424 bytes. The Cartesian maximum therefore
cannot fit the pinned 1,232-byte legacy/v0 transaction even before message
overhead and is an expected falsification point, not a supported maximum. Every
actual matrix point is serialized and measured. The reduced controlled case
defined below has 608 bytes of Core instruction data before transaction-message
overhead.

The outer Core account order is exact for this candidate:

```text
fixed prefix
  0. experimental Core configuration                 read-only, non-signer
  1. market descriptor                               read-only, non-signer
  2. authenticated protocol-fee policy               read-only, non-signer
  3. selected engine program                          read-only, executable
  4. callback authority PDA                           read-only, non-signer
  5. Instructions sysvar                              read-only, non-signer

dynamic segments
  6..L. loader-policy closure                         read-only, non-signer
  next. domain descriptor/admission/accounting        exact row privileges
  next. actor, stored, or intent-spend controls        exact derived privileges
  next. protected-profile programs, mints, and facts  read-only, non-signer
  next. fee-shard descriptors and liability ledgers   exact row privileges
  next. settlement-capability endpoints               exact derived privileges
  last. ordered opaque capability tail                actual accepted privileges
```

The five control-segment counts before `settlement_capability_count` delimit the
control segments. `settlement_capability_count` and
`opaque_capability_count` delimit the last two. Exact consumption must reach the
end of the landed account slice; there are no unclassified remaining accounts.
A zero-length dynamic segment is valid only where its policy permits it.

For the classic-SPL fixture, the protected-profile segment is exactly the
read-only classic SPL Token program followed by one read-only mint per asset in
asset-index order. Each closed-rule participating domain consumes exactly three
domain controls in its row: descriptor read-only, admission read-only, and local
accounting writable. An open-rule domain consumes descriptor and accounting;
its admission offset is `255`. Each fee shard consumes exactly two fee controls:
an authenticated read-only shard descriptor and its Core-owned writable
liability ledger. Its exact fee vault is the settlement endpoint named by the
FeeShard row, not an omitted or inferred account.

The fixed engine CPI is assembled from account 4 plus only the final opaque
segment as callee metas. Accounts 0 through 2, account 4 before signing, account
5, and every intervening control or settlement account are absent from the
callee meta list. The engine program `AccountInfo` at position 3 is still
supplied to the caller's `invoke_signed` account-info slice as the executable
target, as Solana requires; it is not exposed to the engine as a callee
capability. The callee account order remains exactly the read-only signed
callback PDA followed by the unmodified opaque tail, with no engine-state
prefix.

Before interpreting segments, Core unions signer and writable privilege for
every equal public key across the complete outer instruction. It then applies
the fixed and segment-specific privilege rules and the cross-table alias rules.
Instruction account order is part of the private envelope digest. A builder may
use address lookup tables, but resolution cannot change this effective order or
security identity.

For every direct witness and actor-authorized control instruction, Core also
parses account 5 with the checked Instructions-sysvar API and applies the exact
dual authority rule above. The wallet branch proves transaction-root Core bytes,
ordered account keys, resolved effective privileges, and signer meta. It does
not claim that the Instructions sysvar preserves original duplicate-position
flags. The program branch proves one direct CPI from the top-level parent,
off-curve signer authority,
parent-meta presence, and protected-authority exclusions without claiming that
the sysvar exposes inner CPI bytes. Nested routing or inherited signer privilege
fails.

`expected_engine_sequence` is executor-selected freshness and is not silently
treated as a user-authorized economic term. Every intent still has its own
exclusive expiry and bounds. Every resolved intent digest also commits the
exact `expected_engine_loader_state_snapshot_digest`, so mutable-controller admission
does not make an old user authorization float across deployments.

### Engine request

The engine CPI begins with an eight-byte private discriminator followed by:

```text
EngineRequestHeaderCandidateV0 {
  magic: [u8; 8]
  wire_version: u8
  phase: u8
  settlement_capability_count: u8
  opaque_capability_count: u8
  intent_count: u8
  domain_count: u8
  asset_count: u8
  context_row_count: u8
  payload_len: u16
  maximum_engine_moves: u8
  reserved: [u8; 5]
  market_binding_digest: [u8; 32]
  engine_instance_id: [u8; 32]
  engine_interface_id: [u8; 32]
  intent_set_digest: [u8; 32]
  domain_set_digest: [u8; 32]
  protected_execution_root: [u8; 32]
  opaque_capability_root: [u8; 32]
  engine_loader_state_snapshot_digest: [u8; 32]
  fee_policy_digest: [u8; 32]
}
```

The header is exactly 312 bytes, or 320 bytes with the private discriminator.
The reserved bytes are zero. It is followed first by exactly `asset_count`
authenticated asset rows:

```text
EngineAssetRowCandidateV0 {
  asset_index: u8
  asset_flags: u8
  decimals: u8
  reserved: u8
  asset_identity: [u8; 32]
  asset_program: [u8; 32]
  settlement_profile_digest: [u8; 32]
}
```

Each asset row is exactly 100 bytes. Indices are contiguous from zero and rows
are in index order. The classic-SPL fixture requires `asset_flags == 0`; unknown
bits and nonzero reserved bytes fail. Asset identity is the exact mint for this
profile. The engine receives identity facts as data without receiving any
protected mint or token `AccountInfo`.

The asset rows are followed by exactly `domain_count` authenticated domain rows:

```text
EngineDomainRowCandidateV0 {
  domain_index: u8
  reserved_0: [u8; 7]
  domain_descriptor: [u8; 32]
  domain_revision: u64
  admission_digest: [u8; 32]
  accounting_profile_digest: [u8; 32]
}
```

Each domain row is exactly 112 bytes. Indices are contiguous from zero and rows
are in index order. These are authenticated economic inputs, not authorities;
the corresponding domain control accounts stay in Core.

The domain rows are followed by exactly `intent_count` witness-neutral intent
rows:

```text
EngineIntentRowCandidateV0 {                     // exactly 120 bytes
  authorization_slot: u8
  reserved: [u8; 7]
  identity: InlineIntentIdentityRowCandidateV0   // exact 80 bytes
  intent_digest: [u8; 32]
}
```

Rows are in contiguous authorization-slot order, reserved bytes are zero, and
the embedded engine-terms commitment is nonzero. Core recomputes every digest
from the exact identity and immutable Core terms. The row exposes no witness
kind, control-account offset, mutable fill sequence, or stored-state tag.

The intent rows are followed by exactly one authenticated policy row:

```text
EngineFeePolicyRowCandidateV0 = FeePolicyRowCandidateV0  // exactly 32 bytes
```

Its complete encoding must reproduce the `fee_policy_digest` in the request
header. It is data for engine economics, not authority to change the fee.

The policy row is followed by exactly `context_row_count` settlement-context
rows:

```text
EngineContextRowCandidateV0 {
  settlement_capability_index: u8
  asset_index: u8
  domain_index_or_none: u8
  authorization_slot_or_none: u8
  rights_bits: u16
  fee_class: u8
  context_flags: u8
  endpoint_key: [u8; 32]
  observed_before: u64
  accounted_before_or_zero: u64
  remaining_maximum_engine_debit: u64
  remaining_maximum_total_debit: u64
  remaining_minimum_credit: u64
  remaining_maximum_protocol_fee: u64
}
```

Each context row is exactly 88 bytes. `255` is the only no-domain or
no-authorization sentinel. Private `rights_bits` are debit, credit,
domain-accounted, and exact-external-recipient; all other bits and all
`context_flags` fail. A Core-reserved fee destination has no engine context row.
Every engine-referenceable non-fee capability has exactly one context row; no
such row may be omitted, while fee-vault capabilities are excluded. The exact
row meaning is fixed by the classic-SPL capability profile; it does not introduce
product verbs. Rows are strictly increasing by settlement capability index, and
every referenced asset, domain, and authorization index is in range. Endpoint
keys are authenticated data only and their protected `AccountInfo`s stay in
Core. `remaining_minimum_credit` is the unfulfilled absolute amount; Core
evaluates complete cumulative credit inequalities separately from the
authenticated intent terms. The 88-byte engine row carries a checked `u64`
projection of domain accounting; a nonzero protected `u128` value that does not
fit fails before callback rather than truncating.

The request ends with exactly `payload_len` opaque bytes. The central typed
encoder produces the complete canonical CPI instruction data, including the
discriminator, header, every typed row, and payload; the exact decoder rejects
trailing or non-canonical bytes. Its private digest is:

```text
request_digest = H(
  "engine-request-v0",
  complete canonical CPI instruction data as one framed part
)
```

No caller or engine independently reconstructs a differently partitioned hash
preimage. The request digest is bound by the engine receipt.

At the private maxima of eight assets, four domains, eight intents, 12 context
rows, and a 128-byte payload, the complete engine instruction data is exactly:

```text
320 + 8*100 + 4*112 + 8*120 + 32 + 12*88 + 128 = 3,744 bytes
```

This remains below both the private 8,192-byte headroom ceiling and current
10,240-byte CPI data limit. It is an encoding maximum, not evidence that every
independent account and move maximum fits the transaction packet, lock, or
compute gates.

### Engine effect receipt

The candidate return data is:

```text
EffectReceiptCandidateV0 {
  magic: [u8; 8]
  wire_version: u8
  phase: u8
  move_count: u8
  flags: u8
  request_digest: [u8; 32]
  intent_set_digest: [u8; 32]
  protected_execution_root: [u8; 32]
  engine_sequence: u64
  engine_supplied_evidence_digest: [u8; 32]
  moves: [MoveCandidateV0; move_count]
}
```

The fixed receipt is exactly 148 bytes. Every move is 10 bytes, so the private
12-move maximum is 268 bytes. `flags` must be zero, and the engine-supplied
evidence digest must be nonzero. It is the engine's raw opaque claim, not the
already wrapped attested digest. Core computes the canonical effect digest and
the final engine-attested evidence digest exactly once after decoding and
validation.

The receipt deliberately contains no action, product, authorization mode,
engine-state account, fee move, position mutation, escrow mutation, asset
adapter call, or arbitrary program instruction.

Core and engine evidence remain explicitly different claims:

```text
ObservedDeltaRowCandidateV0 {                    // exactly 40 bytes
  settlement_capability_index: u8
  reserved: [u8; 7]
  observed_before: u64
  observed_after: u64
  gross_debit: u64
  gross_credit: u64
}

observed_delta_root = H(
  "observed-protected-delta-set-v0",
  u32_le(observed_delta_count),
  every complete 40-byte row in strict capability-index order
)
```

The row set is exactly every protected endpoint changed by an accepted engine
or Core-fee move; Core derives it from those moves, so the caller cannot omit a
changed endpoint. Unaffected rows are omitted deterministically. Reserved bytes
are zero, arithmetic is checked, and this profile requires
`after = before - gross_debit + gross_credit`. The Move normal form prevents one
endpoint from carrying both a nonzero debit and credit.

```text
core_verified_evidence_digest = H(
  "core-verified-evidence-v0",
  Core program,
  u32_le(Core experimental major),
  market_binding_digest,
  engine_loader_state_snapshot_digest,
  intent_set_digest,
  domain_set_digest,
  protected_execution_root,
  opaque_capability_root,
  request_digest,
  canonical_effect_digest,
  fee_assessment_set_digest,
  observed_delta_root
)

engine_attested_evidence_digest = H(
  "engine-attested-evidence-v0",
  engine_program,
  engine_interface_id,
  engine_instance_id,
  request_digest,
  engine_supplied_evidence_digest
)
```

The Core digest attests only the exact authority, movement, fee, and observed
postcondition facts Core verified. The engine digest attests arbitrary engine
meaning. Neither can be substituted for the other.

### Private measurement limits

The experiment uses these ceilings only to create deterministic resource cases:

| Item | Private maximum |
| --- | ---: |
| Participating domains | 4 |
| Authorization intents | 8 |
| Authorization snapshot rows | 8 |
| Inline intent identity rows | 4 |
| Authenticated assets | 8 |
| Loader-policy accounts | 1 |
| Domain-control accounts | 12 |
| Authorization-control accounts | 20 independent maximum; expected failing Cartesian sweep |
| Protected-profile accounts | 9 |
| Fee shards | 4 |
| Fee-control accounts | 8 |
| Settlement capabilities | 12 |
| Engine moves | 12 |
| Opaque-tail account positions | 8 |
| Engine intent rows | 8 |
| Engine fee-policy rows | exactly 1 |
| Engine context rows | 12 |
| Opaque payload | 128 bytes |
| Top-level instruction data | 1,424 bytes; expected Cartesian failure |
| Engine request | 3,744 bytes at all encoding maxima |
| Engine receipt | 268 bytes at 12 moves |

Fields remain wider where convenient so hitting one experiment ceiling does not
silently define a public integer width. A result may recommend smaller, larger,
or structurally different product limits only through a later decision. These
are independent sweep maxima, not a claim that their Cartesian product can fit
one transaction.

## Reference semantics

The same Core binary, move decoder, validator, fee kernel, and event evidence
must execute all reference engines. Core code may not branch on a reference
engine identity or semantic label.

### Stateless allocation engine

A stateless engine uses no engine-owned state account in its opaque tail. It
derives one deterministic allocation from authenticated context and payload,
returns moves, and increments no engine sequence except an agreed stateless
sentinel. This proves that zero fixed state accounts are valid.

### Constant-product engine

One engine-owned state account in the opaque tail stores its curve parameters
and sequence. It computes a conventional exact-input two-asset result and
returns the same authority-level moves Core would use for any two-asset
allocation. Core does not reproduce its formula or label the result a swap.

### Stored partial-fill auction

An engine uses multiple engine-owned opaque-tail accounts for auction state and
orders. One or more stored Core authorizations contribute bounded debits and
credits. A fill updates engine state, returns several moves, and exercises the
cumulative credit inequalities and fee equation. A second fill from cloned
state proves partition-independent cumulative fees, prefix-safe credit, and
explicit replay state.

### Multi-intent batch allocation

At least four independently authorized actors and two classic-SPL assets settle
one batch. The engine may direct several source capabilities to several exact
credit recipients. Core sees only capabilities, moves, bounds, admissions, and
fees. One actor's bounds, balance, fee, or authorization state cannot subsidize
another actor.

### Multi-recipient inventory distribution

A 0-decimal classic-SPL inventory asset and a payment asset settle to an exact
buyer recipient plus multiple payment recipients. The Core verifies transfers
but does not claim that the inventory is an NFT, that a payment is a royalty, or
that an engine-defined split is economically correct.

Together the fixtures exercise zero, one, and many engine-owned state accounts
without changing the fixed callback prefix.

## Required hostile evidence

Every cross-program security claim requires exact-SBF evidence. Pure codec,
hash, canonicalization, arithmetic, and model properties may use deterministic
unit or property tests in addition to SBF cases.

### Engine callback and identity

Tests cover:

- direct engine invocation without Core;
- wrong callback address, signer privilege, phase, market-binding digest,
  interface, instance, intent set, domain set, protected-execution root, or
  payload;
- callback forwarding to an opaque helper and attempted reuse after Core
  returns;
- wrong engine program or substituted executable account;
- zero, one, and many engine-owned state accounts in the opaque tail;
- an engine that assumes one fixed state prefix and therefore fails the
  candidate contract;
- return data set by a descendant, missing return data, stale data, trailing
  bytes, unknown version, nonzero flags, wrong request digest, and oversized
  move count; and
- engine mutation followed by every later failure class with complete rollback.

### Capability closure and aliasing

Tests cover:

- duplicate, reordered, omitted, and added opaque positions;
- under-counted, over-counted, overlapping, or trailing outer account segments;
- landing-time owner or executable-state drift after authorization;
- duplicate public keys with signer or writable privilege hidden in another
  position;
- static-account and address-lookup-table aliasing;
- settlement/opaque cross-table aliases;
- callback, fee, admission, loader-policy, authorization, domain, mint, token
  program, and Core-program aliases;
- opaque signers, Core-owned accounts, writable executables, writable classic
  token accounts, and writable Token-2022 accounts;
- protected profile ancillary-account aliases; and
- engine PDA signers exercising all authority already available in the opaque
  plane while remaining unable to reach the protected plane.

Canonical-wire tests also mutate every count, index, length prefix, reserved
byte, boolean, rights bit, persistent asset binding, engine asset row, domain
row, authorization snapshot, inline identity, capability-state row, fee-state
row, engine intent row, fee-policy row, context row, SettlementCapability field,
368-byte protected-capability digest field, 256-byte fee-shard digest field,
local-to-global capability mapping, list position, and digest label. Golden
vectors cover empty, singleton, and maximum lists so an ambiguous concatenation
or client-side sort cannot accidentally pass.

### Move decoding and accounting

Tests cover:

- zero amount, identical endpoints, out-of-range indices, wrong rights, reserved
  fee indices, and asset/profile mismatch;
- duplicate and non-canonical row order;
- a capability used as both source and destination;
- checked aggregation overflow at `u64` and `u128` boundaries;
- source debit over engine or total-source authorization, a partial debit of 99
  and credit of 1 against a signed 90/100 cumulative constraint, unmet terminal
  credit, domain debit over accounted balance, and insufficient raw vault
  balance;
- a globally balanced graph containing an unauthorized victim-domain debit;
- exact per-domain accounting when several admitted domains participate;
- donation before execution, with raw balance increasing but no accounted right;
- unexpected token debit or credit and post-balance mismatch; and
- failure during every protected transfer position with complete rollback.

### Domain admission

Tests cover:

- a new attacker market naming an existing victim domain;
- wrong market, engine program, interface, instance policy, code policy,
  capability profile, descriptor, or revision;
- one domain deliberately admitting several markets;
- one open deterministic domain rule;
- revoked or superseded mutable local admission where the selected private rule
  permits changes;
- a non-participating domain supplied as a credit destination, proving that a raw
  donation creates no accounting; and
- no global writable registry or platform signer in any successful path.

### Loader-aware policy

Tests cover:

- immutable release capture and ordinary execution through the exact Core-owned
  evidence PDA without ProgramData in the hot path;
- exact-idempotent capture and rejection of a conflicting first-writer,
  arbitrary loader, wrong release PDA, wrong owner, wrong digest, or forged
  immutable kind without release evidence;
- the pinned-mutable hostile fixture, explicitly rejected as a public policy,
  and the explicit mutable-controller-risk fixture;
- wrong loader, canonical ProgramData derivation, embedded ProgramData address,
  last-modified slot, current controller, or authority option;
- valid Loader-v3 Program accounts larger than 36 bytes, ProgramData longer than
  45 bytes, and immutable `None` state retaining stale former-authority bytes;
- real loader deploy, upgrade, authority removal, and replacement-engine
  behavior rather than metadata-only account mutation or stale VM cache;
- upgradeable-loader v3 execution at the ProgramData last-modified slot;
- writable effective Program or ProgramData privilege hidden by a duplicate;
- writable Program or ProgramData privilege retained by any top-level
  instruction but downgraded in a routed Core CPI;
- addition or removal of an upgrade authority contrary to the bound policy;
- permissionless mutable `ExtendProgram` changing the slot, increasing loaded
  ProgramData, invalidating pinned state, and demonstrating the targeted
  liveness/resource denial up to the loader cap;
- a later deployment accepted by mutable-controller admission only with a new
  loader-state snapshot and user intent;
- an old intent or envelope snapshot replayed after that later modification;
- policy disagreement between market and one participating domain; and
- unknown loader or malformed loader state.

No test may describe a mutable-controller fixture as pinned or immutable, call
a mutable loader-state tuple an ELF hash, or present the pinned-mutable fixture
as an accepted product policy.

### Authorization neutrality and replay

Tests cover:

- equivalent direct, exact delegated, stored, partial, and multi-intent paths;
- an on-curve direct witness at transaction level, an off-curve actor through
  exactly one direct top-level-program CPI, and rejection of the same signer
  through a nested router or callback;
- rejection of an on-curve signer through CPI, an off-curve transaction-root
  pseudo-signer, a program actor absent from the top-level parent's metas, and
  every callback, spend, execution, accounting, fee, or control PDA substituted
  as the actor;
- one malicious router changing recipient or amount and attempting two Core
  calls under one inherited signer;
- wrong Instructions sysvar, current instruction index, Core program, landed
  bytes, ordered meta, requested privilege, or actor signer;
- dual-rule actor-authorized create, write, activate, cancel, replacement, and
  canonical delegate helper, including nested-CPI rejection;
- persistent 136-byte capability terms and 64-byte credit constraints rejecting
  any global index, account offset, witness discriminator, duplicate local index,
  non-canonical debit group, or unmapped local term;
- wrong actor, source, recipient, protected-execution root, intent order,
  authorization slot, nonce, fill sequence, expiry, cancellation state, and
  remaining amount;
- immutable intent-digest stability across successful stored fills and mutable
  authorization-view-set-digest change on every fill;
- stale sequence after a competing fill, and overlapping multi-intent fills that
  share one writable authorization;
- cancellation-first and fill-first ordering, replacement only through a new
  nonce, and rejected cancel-close-recreate sequence-zero resurrection;
- post-success replay of a direct transaction, stored intent, and consumed
  exact delegate under their distinct replay boundaries;
- exact one-shot delegate success only when each source uses the spend PDA
  derived from `(intent_digest, source)`, its engine plus Core-fee debit consumes
  the complete allowance, and it leaves zero delegation;
- a generic Core delegate, a spend PDA derived from another source or digest,
  and later reapproval after expiry all rejected;
- changed market, loader-state snapshot, engine-terms commitment, Core terms,
  fee policy, source, or recipient under an otherwise valid delegate;
- reapproval of an old exact authority treated as explicit reauthorization of
  the same unexpired nonce and terms;
- a maximum-only, leftover, variable, partial, or zero-debit delegate rejected
  unless a StoredAuthorization supplies explicit replay state;
- partial fills whose sum exactly reaches the authorized maximum;
- one additional partial fill after exhaustion;
- zero-debit execution bounded by and consuming the signed successful-fill
  count;
- an unrelated pure no-op authorization inserted into a multi-intent envelope;
- duplicate intent digest, stored account, or non-canonical slot assignment in
  one multi-intent set;
- the same protected public key in multiple authorization slots;
- one user's surplus, minimum, fee ceiling, or remaining amount being applied to
  another user; and
- pre-callback execution-state reentry, mutation after entering `Executing`, and
  every post-receipt failure rolling back to the exact pre-execution
  authorization state.

### Protocol fees

Tests cover:

- missing, zeroed, redirected, caller-selected, engine-selected, duplicated, or
  wrong-policy fee effects;
- missing, aliased, reordered, read-only, or wrong-policy shard descriptor,
  liability ledger, or fee-vault capability;
- a fee liability shard from another market and any global writable fee ledger;
- engine attempts to reference the reserved fee destination;
- equivalent unsplit and split engine graphs producing one identical aggregate
  rate assessment;
- dust splitting around every rounding boundary, including 99 units from each of
  two funding accounts at a 1/100 floor rate under one fee principal;
- endpoint, source-capability, authorization-slot, and shard changes under the
  same fee principal leaving one unchanged rounding group, while a different
  actor or intent digest produces a distinct group;
- zero or two fee-funding sources for one nonzero-fee group rejected;
- a source/destination cycle rejected before fee calculation;
- cumulative fee equality for at least 4,096 partitions and boundary values;
- stored `cumulative_assessed == A_policy(cumulative_basis)` before and after
  every fill;
- mutation of every fee-assessment preimage part, duplicate group digest, and
  non-canonical 64-byte assessment-set row order;
- flat, fixed-envelope, caller-selected sponsor, and fee-only classes rejected;
- zero assessable fee for a zero-protected-debit engine transition;
- independent fee attribution for multiple actors and assets;
- fee above a user ceiling;
- raw fee-vault donation creating no liability; and
- fee transfer or post-credit failure rolling back engine, authorization,
  protected, fee, and Core state.

### Product neutrality

Tests or compile-time fixtures prove that:

- the Core crate contains no reference-engine program ID;
- the Core instruction and effect codec contain no action or product enum;
- every reference engine uses the same callback account prefix;
- zero, one, and many engine state accounts are all expressed only through the
  opaque tail;
- the same move bytes can arise from different engine-defined semantics; and
- no engine-provided label changes validation, fee class, evidence class, or
  authority.

## Atomicity and liveness boundary

The selected engine remains the economic authorization oracle for every admitted
domain. A compromised engine may return a conservative but economically
destructive graph that drains participating domains within the capabilities and
user bounds those domains accepted. Core containment does not imply fair pricing
or provider solvency.

Engine, helper, stored-intent, token, fee, domain-accounting, and Core writes
occur in one Solana transaction. A failed CPI is not catchable as an alternate
settlement branch. Any failure rolls back account-state changes. Transaction
fees and failed-transaction metadata are not rolled back.

An unavailable, upgraded, or permanently failing engine can halt a market.
Before persistent Core custody, every domain still requires a separately
accepted exit class: no persistent custody, an exact Core-verifiable
engine-independent claim, or disclosed engine-liveness dependence. This
experiment creates no provider entitlement and proves no withdrawal path.

Shared domains intentionally share locks and liveness. Unrelated domains use no
global writable registry, accumulator, sequence, or fee account, so one engine's
failure does not create a protocol-wide state dependency.

## Resource sweep

The private gate pins and must revalidate the active 2026-08-28 legacy/v0
runtime baseline before recording results:

- 1,232 serialized transaction bytes;
- 64 unique locked accounts;
- 255 `AccountInfo` positions in one CPI under the active SIMD-0339 gate, which
  does not increase the 64 unique-account lock limit;
- 1,400,000 compute units when explicitly requested;
- 946 base compute units per invocation under the same active CPI cost gate,
  before the invoked work and account-data charges;
- instruction stack height 5, meaning top level plus four nested invocations;
- 64 total executed instructions;
- 1,024 bytes in the single transaction return-data buffer;
- 10,240 bytes of instruction and CPI data; and
- 64 MiB, or 67,108,864 bytes, of loaded account data.

Address lookup tables compress v0 addresses but do not remove locks, cannot
supply signer keys, and require the active table state to be warmed up. Their
256-address table capacity is not transaction account capacity. Transaction v1,
4,096-byte messages, a 128-account lock gate, and a deeper CPI stack are not
assumed.

The experiment records successful and failing sweeps across:

| Dimension | Values |
| --- | --- |
| Engine moves | 2, 4, 8, 12 |
| Settlement capabilities | 4, 8, 12 |
| Participating domains | 1, 2, 4 |
| Intents | 1, 2, 4, 8 |
| Authorization snapshot rows | 1, 2, 4, 8 |
| Inline intent identity rows | 0, 1, 2, 4 |
| Assets | 1, 2, 4, 8 |
| Fee shards | 1, 2, 4 |
| Opaque-tail positions | 0, 4, 8 |
| Payload bytes | 0, 32, 128 |
| Engine state accounts | 0, 1, several within the opaque tail |
| Execution | direct and permissionless-routed |
| Nested engine behavior | no helper and one helper CPI |
| Authorization | direct, exact delegate, stored, partial, multi-intent |

At least one maximum controlled path uses a real v0 address lookup table and
executes:

```text
permissionless router -> Core -> engine -> opaque helper
```

After the engine returns, the same Core invocation executes every real
classic-SPL protected move and fee move. The helper and token calls need not be
nested simultaneously, but their frames and shared compute all count.

The complete Core account-position equation is:

```text
6 fixed + loader + domain controls + authorization controls
        + protected-profile controls + fee controls
        + settlement endpoints + opaque positions
```

A closed domain contributes three controls; an open domain contributes two. A
classic-SPL profile contributes one program plus one mint per asset. Each fee
shard contributes two fee controls in addition to its vault settlement endpoint.
No descriptor, admission, accounting, fee ledger, shard, or vault may be omitted
from a resource row.

The reduced predeclared combined case uses the routed path, two closed domains,
two stored intents, two assets, two market-local fee shards, one loader-policy
account, six settlement capabilities, six moves, four opaque positions, and no
payload. It has at most four authorization controls: two stored accounts and two
source-specific spend-authority PDAs. Its exact Core positional count is:

```text
6 fixed + 1 loader + 6 domain + 4 authorization + 3 protected profile
        + 4 fee + 6 settlement + 4 opaque = 34 positions
```

Its Core instruction data is exactly:

```text
272 fixed + 2*8 domain + 2*8 snapshot + 0*80 inline
          + 2*8 fee shard + 6*48 settlement + 0 payload = 608 bytes
```

These are position and instruction-data facts, not packet proof. The test must
serialize the actual v0 message and derive unique locks after privilege union,
including payer and every invoked program ID; this document makes no unmeasured
unique-lock or packet-size prediction. Direct-control cases also record the
compute and loaded-data cost of checked Instructions-sysvar parsing.

Separate one-axis boundary cases raise settlement capabilities and moves to 12,
opaque positions and intents to eight, assets to eight, domains and fee shards
to four, and every control segment to its private maximum while holding other
dimensions at their documented baseline. Every reported row includes the exact
resolved unique-key set; position counts alone are not lock evidence.

For the controlled maximum accepted case, the experiment requires at least 20%
headroom against every countable active limit and one unused stack level:

| Resource | Experimental acceptance ceiling |
| --- | ---: |
| Serialized legacy/v0 transaction | at most 985 bytes |
| Unique locked accounts | at most 51 |
| CPI `AccountInfo` positions | at most 204 |
| Compute | at most 1,120,000 CU |
| Maximum stack height reached | at most 4 |
| Executed instructions | at most 51 |
| Return data | at most 819 bytes |
| Instruction or CPI data | at most 8,192 bytes |
| Loaded account data | at most 53,687,091 bytes |

The exact 20% policy is a private falsification threshold, not a public product
limit. The test records writable-lock counts and the observed invoke cost even
where they are not the first failing limit.

If the full 12-capability or 12-move private ceiling fails but all required
reference semantics pass a smaller predeclared matrix with the required
headroom, the result may recommend a smaller next experiment. It may not call
the failed maximum supported or hide which dimension failed.

## Acceptance criteria

The private direction passes only if executable evidence establishes all of the
following:

1. Every reference semantic executes through one unchanged Core effect path with
   no product enum, engine-specific branch, or engine-specific fixed account.
2. The engine callback prefix is exactly one read-only callback signer followed
   by the ordered opaque tail; zero, one, and many engine-owned state accounts
   all work there.
3. No protected account or authority enters the engine CPI or any descendant
   through authority granted by Core.
4. The protected-execution and opaque-capability roots bind normalized current
   authorization state and the actual landing-time closure, preserve opaque
   position and multiplicity, normalize privilege by key, and reject every
   cross-plane alias; canonical AssetBinding, AuthorizationSnapshot,
   InlineIntentIdentity, IntentCapabilityTerm, CreditConstraint,
   CapabilityState, FeeState, DomainControl, FeeShard, SettlementCapability,
   EngineAsset, EngineIntent, FeePolicy, Context, ProtectedCapabilityDigest, and
   FeeShardDigest rows leave no inferred role or bound.
5. Every domain debit has an exact domain-local admission proof; self-declared
   or non-participating domains fail before the engine callback.
6. The Move normal form, asset conservation, per-domain accounting, user bounds,
   exact observed deltas, and donation boundary all pass differential and hostile
   tests.
7. Immutable identity, immutable local Core terms, and mutable authorization
   snapshots remain separate; persistent terms contain no later global index,
   account offset, or witness kind. Top-level direct, exact-delegate, stored,
   partial, and multi-intent paths use one unchanged engine request and Move
   result, with explicit replay state wherever complete source-specific delegate
   consumption is not sufficient.
8. Stored fills enforce prefix-safe cumulative credit inequalities, terminal
   minima, finite successful-fill counts, tombstone cancellation, and exact
   rollback from the `Executing` state.
9. Core alone derives the mandatory intent-principal gross-debit rate assessment,
   cumulative partial-fill fees are partition-independent, endpoint or source
   splitting cannot reset rounding, exactly one signed fee-funding source exists
   per group, flat fees fail closed, and each observed fee credit updates its
   exact market-local protected shard liability ledger once.
10. Loader-aware immutable release evidence and explicitly mutable-controller
   risk are distinguished through separate admission-policy and loader-state
   snapshot digests, including top-level privilege scans and strict later-slot
   gates; the pinned-mutable fixture demonstrates and rejects its permissionless
   Extend liveness/resource denial rather than passing as a public policy.
11. Wrong receipt setters, malformed plans, late transfer or fee failures, and
    resource exhaustion leave no partial account-state transition.
12. Wallet-direct, direct program-actor, direct exact-delegate, routed
    exact-delegate, and stored paths from cloned authorized state produce the
    same semantic outcome and evidence classes without treating an inherited
    signer from a nested call as intent.
13. The reduced controlled case retains the declared packet, lock, compute,
    stack, trace, and return-data headroom under the pinned active runtime; the
    1,424-byte Cartesian top-level envelope fails as explicitly predicted.
14. No private byte, seed, discriminator, bound, fixture, program ID, or account
    layout is exported as a compatibility promise.

Passing means the capability-indexed Move direction survived this experiment.
It does not mean the exact candidate wire or limits are accepted.

## Falsification criteria

The direction is rejected, narrowed, or sent to a new architecture decision if:

- one reference engine needs a Core-visible product/action discriminator;
- the fixed callback prefix needs any engine-state account;
- the engine needs a protected signer, protected writable account, or arbitrary
  Core-authorized driver to produce the required result;
- an account can occupy inconsistent protected roles after privilege union;
- Core must infer an intent role, authorization slot, bound, fee class, domain,
  shard, or liability target from undeclared account position or ownership;
- global conservation can hide an unauthorized domain debit or accounting loss;
- direct, exact-delegate, stored, partial, or multi-intent authorization changes
  the effect semantics or engine result format;
- direct signer authorization remains valid through CPI, or a mutable fill
  sequence remains inside the immutable intent identity;
- a stored intent must bind unknown future counterparties or may omit its exact
  local funding, recipient, fee, or constraint terms;
- persistent intent terms require an execution-global index, account offset, or
  witness discriminator instead of a local canonical mapping;
- a nonce is treated as mutable replay state, or reusable routed authority is
  accepted without a stored state transition or complete source-specific
  exact-delegate consumption;
- cancellation or replacement can reset an old authorization to an executable
  sequence, or a zero-debit stored transition has no finite fill bound;
- a partial fill can violate its cumulative credit inequality or enter a terminal
  state below its absolute minimum;
- equivalent fee bases produce different cumulative fees because of split,
  ordering, netting, or rounding;
- changing endpoints, capabilities, slots, or shards under one fee principal
  resets cumulative fee basis or assessed state;
- the first experiment needs a flat fee or lets any participant be selected as
  an envelope-fee sponsor;
- an engine or caller can choose a cheaper semantic fee label for equivalent
  protected effects;
- a market can adopt an existing domain without that domain's local proof;
- an immutable execution can omit or forge its Core-owned release evidence, or
  loader/controller/last-modified-state drift remains indistinguishable under a
  mutable policy;
- a pinned-mutable loader-state fixture is presented as liveness-safe despite
  the permissionless `ExtendProgram` slot and loaded-data denial;
- mutable-controller admission either pins every future deployment slot or lets
  an old intent float to a different loader-state snapshot;
- a post-settlement engine callback is required for the baseline semantics;
- correctness relies on a future Solana feature or unmeasured runtime behavior;
- the required reference matrix cannot retain the declared resource headroom; or
- the implementation needs a generic arbitrary-CPI interpreter in Core.

A failed hypothesis is a useful result. The result record must identify the
smallest failing fixture and authority or resource reason rather than weakening
the boundary until it passes.

## Required evidence outputs

The later experiment result must record:

- exact source commit and tree, parent commit, branch, and pull request;
- exact host Rust, Solana/Agave, SBF/SBPF compiler, platform-tools, Anchor,
  LiteSVM, Mollusk or Surfpool versions;
- exact cluster genesis, software, feature-set, and active-feature observations
  used to revalidate the runtime baseline;
- every disposable program ID and exact canonical Ubuntu SBF artifact hash;
- at least one independent reproducible artifact comparison or an explicit
  statement that it remains absent;
- complete private wire layouts, exact lengths, domain-separated hash inputs,
  canonical AssetBinding/IntentCapabilityTerm/CreditConstraint/CapabilityState/
  FeeState/
  AuthorizationSnapshot/InlineIntentIdentity/DomainControl/FeeShard/
  SettlementCapability/ProtectedCapabilityDigest/FeeShardDigest/EngineAsset/
  EngineIntent/FeePolicy/Context byte vectors, stored-view equivalence vectors,
  and mutation coverage;
- exact protected-execution, fee-assessment, fee-assessment-set, CoreVerified,
  and EngineAttested preimage vectors;
- the complete unit, property, differential, exact-SBF, router, engine, helper,
  authorization, domain, loader, fee, and rollback test inventory;
- direct and routed packet bytes, static and ALT-loaded keys, unique and writable
  locks, CPI `AccountInfo` positions, compute and invoke cost, maximum stack
  height, total frames, instruction trace length, CPI data, return data, and
  loaded-account data for every resource fixture;
- zero-, one-, and many-engine-state tail evidence;
- reference-semantic parity showing one unchanged Core effect path;
- per-authorization-mode effect and evidence parity;
- cumulative credit-inequality vectors across partial-fill and terminal
  boundaries;
- cumulative fee vectors across partition and rounding boundaries;
- the 1,424-byte expected top-level Cartesian failure, the exact 608-byte
  reduced-case instruction, and serialized packet proof for every claimed
  accepted resource point;
- domain-admission, non-participating-domain, alias, callback-forwarding,
  admission-policy/loader-snapshot drift, same-slot loader rejection, replay, fee-
  liability, and late-failure attack traces;
- a CoreVerified-versus-EngineAttested event classification example;
- rejected alternatives, failed maxima, remaining uncertainty, and the exact
  smallest next gate; and
- a maturity checkpoint that cannot be presented as a production audit.

The result must distinguish local host tests, exact-SBF execution, CI artifact
reproduction, fork behavior, devnet behavior, deployment, and onchain source
verification. This experiment requires only the first three. No program from it
may be deployed or entrusted with real funds.

## Explicit non-goals

This experiment does not establish:

- a public engine, intent, capability, move, receipt, event, IDL, SDK, or account
  ABI;
- final product account, payload, move, intent, domain, packet, or compute limits;
- a generic arbitrary-action interpreter or safe arbitrary CPI;
- fair prices, honest engines, profitable markets, provider solvency, or MEV
  resistance;
- full order placement, cancellation, matching, auction, or asynchronous intent
  lifecycle semantics beyond the minimum authorization-neutral fixtures;
- Token-2022, transfer hooks, SOL/WSOL lifecycle, mint, burn, NFTs, compressed
  assets, external settlement drivers, or custom asset authority;
- Core-native positions, provider claims, fee claims, withdrawals, recovery,
  migration, production tombstone rent reclamation, or an engine-independent
  exit;
- final loader support, release manifests, governance, upgrade process,
  immutability, or deployment policy;
- a fee rate, recipient, asset schedule, governance right, or business forecast;
- router, wallet, indexer, archive, monitoring, or incident-response readiness;
- devnet or mainnet execution; or
- immunity from implementation defects.

Each omitted protected authority receives its own decision and hostile tests. It
is not added as a flag, product enum, arbitrary adapter instruction, or opaque
receipt field merely to make this private candidate appear universal.
