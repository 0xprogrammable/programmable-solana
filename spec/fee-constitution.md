# Fee constitution

Status: Draft

This document defines candidate protocol invariants for fees. It is a design and
acceptance contract, not a claim that the current experiment implements these
rules, and it does not select any production fee amount, rate, cap, recipient,
asset, or shard count.

## Purpose

Programmable permits engines with arbitrary and partly opaque semantics. The fee
system must therefore charge only for facts that Core can authenticate without
pretending to know what an engine-defined state transition means.

This constitution separates:

- what Core can enforce inside a committed Core settlement envelope;
- what Core can observe but must describe without product semantics;
- what remains an engine attestation; and
- what no onchain mechanism can make non-bypassable.

The constitution is part of a Core major. A market policy can select only from
the closed modes and bounds accepted by that major. A policy cannot expand the
constitution.

## Normative language

`MUST`, `MUST NOT`, `SHOULD`, and `SHOULD NOT` describe candidate requirements
that must be satisfied before the affected feature can be accepted. They do not
describe the disposable probe unless a separate result document proves that
they hold.

## Honest enforceability boundary

### Enforceable claims

Within a successful Core settlement route, Core can enforce:

1. one authenticated protocol-assessment set for each committed settlement
   envelope, with exactly one canonical entry for every applicable policy and
   assessment group;
2. a fixed assessment funded through an authenticated signer, delegate,
   prefunded escrow, or sponsor;
3. a rate assessment on an exact, canonical group of objective Core-verified
   effects;
4. a share of an explicit fee bucket that Core itself controls; and
5. user ceilings over every Core assessment, external asset tax, and total
   source debit.

Core MUST derive every mandatory protocol assessment from Core-owned policy.
The caller and engine MUST NOT supply a zero policy, alternate protocol
recipient, cheaper semantic class, optional assessment flag, or second fee
schedule.

An assessment is non-bypassable only inside the Core envelope. Permissionless
engines and external programs may expose entrypoints outside Core. The protocol
MUST NOT claim to tax those entrypoints.

All Core writes, engine writes, transfers, fee accounting, and emitted state
evidence are atomic. A failed transaction cannot leave protocol fee revenue or
liability behind. Solana network fees and failed-transaction metadata are
outside this rollback statement.

### Non-enforceable claims

For arbitrary engine semantics, Core MUST NOT claim a universal percentage of:

- a trade or trade count;
- notional value or volume;
- price, spread, reserve growth, or price improvement;
- auction surplus, funding, rebate, royalty, or position fee growth;
- an opaque custom-asset value; or
- engine revenue not paid into an explicit Core-controlled bucket.

An engine may batch several semantic actions into one envelope, encode value in
state or price, or perform the same activity through an external entrypoint.
Consequently, a flat envelope assessment is enforceable per envelope, not per
unknown internal action. A rate on a Core-verified debit is enforceable as a
rate on that debit, not automatically as a trade fee.

Core MUST NOT sum different asset units into a single volume value. Any offchain
conversion to a quote currency needs an explicit price source, observation
time, and manipulation policy and remains indexer evidence rather than onchain
fee truth.

## Assessment classes

Every charged amount belongs to exactly one class.

### Protocol assessment

`ProtocolAssessment` is the mandatory Core-derived bucket. Its mode, asset,
basis, rounding, bounds, recipient set, activation, and revision come only from
authenticated Core policy.

An engine or caller cannot remove, duplicate, redirect, discount, or replace a
protocol assessment. An objectively exempt Core path, such as the independent
emergency exit defined below, is exempt by the Core major rather than by engine
data.

### Builder assessment

`BuilderAssessment` is a separate market-policy bucket. It MUST have its own
amount or basis, recipient, revision, and user ceiling. It MUST NOT be reported
as protocol revenue or silently deducted from a protocol assessment.

The builder recipient is authenticated by the accepted market policy. The
caller cannot substitute it at execution time.

### Integrator assessment and referral share

`IntegratorAssessment` is optional or additive. The user authorization MUST
bind the integrator identity, recipient-set digest, fee limit, and expiry.
Router or engine data alone cannot select or replace the integrator.

A referral share MUST come from the integrator assessment or another explicit
campaign bucket. It MUST NOT silently reduce the mandatory protocol assessment.

Permissionless integrator or referral eligibility is not Sybil-resistant. A
user can self-integrate or self-refer. If an open share is funded from protocol
revenue, the strongest non-bypassable treasury claim is only the residual after
the maximum open share. A restricted campaign may change eligibility, but it
MUST be described as a permissioned rebate program rather than proof of unique
users or organic routing.

### External asset tax

`ExternalAssetTax` covers issuer- or asset-program-imposed amounts, including a
supported Token-2022 transfer fee. It is not selected by Core, is not protocol
revenue, and MUST be quoted, bounded, observed, and emitted separately.

Solana base and priority fees are also external to protocol assessments and
MUST be displayed separately by clients.

## Policy and user authorization

An accepted fee policy MUST bind at least:

```text
FeePolicy {
  core_major
  policy_id
  revision
  assessment_class
  assessment_mode
  basis_selector
  basis_asset_profile
  basis_asset
  fee_asset_profile
  fee_asset
  flat_component
  rate_numerator
  denominator
  rounding_rule
  minimum
  minimum_scope
  policy_maximum
  maximum_scope
  maximum_assessment_count
  recipient_set
  activation_condition
  successor_or_expiry
  collection_partition_rule
  shard_count
}
```

This shape is illustrative rather than a public ABI. Exact codecs and field
widths require their own accepted specification.

A user authorization MUST bind at least:

```text
IntentFeeLimits {
  core_major
  policy_id
  exact_policy_revision
  assessment_class
  basis_asset_profile
  basis_asset
  fee_asset_profile
  fee_asset
  maximum_protocol_assessment
  maximum_builder_assessment
  maximum_integrator_assessment
  maximum_external_asset_tax
  maximum_total_fee_debit
  maximum_total_source_debit
  integrator_and_recipient_digest
  expiry
}
```

An intent with several policies or assets binds one canonical record per
assessment class and group and commits to their complete ordered digest.

The policy maximum and the user maximum have different meanings:

- a policy maximum is part of the deterministic fee formula; and
- a user maximum is a rejection bound.

Core MUST NOT silently clip a computed amount to the user's maximum. If any
user limit is exceeded, the complete transaction fails. A policy cap is a
safety rule, not a promise of an uncapped effective take; batching may cause a
per-envelope cap to bind only once.

Minimum, maximum, and flat-component scope MUST be explicit. A value scoped to
an envelope is deliberately assessed for each committed envelope. A value
scoped to an intent is part of the cumulative intent formula. Core MUST NOT mix
these scopes implicitly.

## Objective fee bases

### Closed basis selectors

A Core major MUST expose a closed set of objective basis selectors. Candidate
selectors are:

- `FlatPerCommittedEnvelope`;
- `RateOnGrossProtectedDebit`;
- `FlatPlusRateOnGrossProtectedDebit`;
- a separately accepted profile-specific `RateOnNetProtectedCredit`; and
- `ShareOfExplicitCoreFeeBucket`.

The generic basis is a gross protected debit. A net-credit basis is allowed only
when an accepted asset and settlement profile defines exact gross, withheld,
refund, and net semantics. A share of an explicit fee bucket applies only to
that bucket and does not prove that all engine revenue entered it.

The assessment leg itself MUST NOT enter its own basis. Builder, integrator,
external tax, and network fees MUST NOT be silently included in protocol basis.
Any deliberate interaction among buckets needs an explicit policy rule and
separate user limits.

A rate formula is dimensionally valid only when basis and fee use the same
base-unit asset identity or a separately accepted conversion profile binds the
price source, observation rule, staleness bound, confidence treatment, and
rounding direction. The initial generic candidate has no cross-asset rate or
implicit oracle. A flat assessment may use a separate supported fee asset
because it does not claim to represent a percentage of the basis asset.

### Capability monotonicity

Fee classes MUST depend on objective Core facts, not engine-supplied product
labels such as `swap`, `launch`, `auction`, `deposit`, or `sale`.

Let `Effects(P)` be the set of protected effects that capability profile `P`
can express. If:

```text
Effects(A) is a subset of Effects(B)
```

then profile `B` MUST NOT provide a lower mandatory assessment floor for an
effect sequence that it can use to emulate profile `A`. Otherwise the cheaper
profile is a protocol-defined bypass.

Every protected debit class is therefore either:

- assessable under an objective policy;
- objectively exempt under the Core major; or
- rejected.

The engine cannot mark an individual effect as non-assessable.

## Canonical aggregation and rounding

### Assessment groups

Before fee calculation, Core MUST canonicalize all assessable effects for the
complete envelope and aggregate them by:

```text
AssessmentGroupKey = (
  policy_id,
  assessment_principal,
  basis_selector,
  basis_asset_profile,
  basis_asset,
  fee_asset_profile,
  fee_asset,
  assessment_class,
  policy_revision
)
```

`assessment_principal` is the authenticated user, sponsor, or intent principal
defined by the authorization profile, not an arbitrary source-account address.
Splitting one principal's basis across several token accounts therefore cannot
create new rounding groups.

For a rate assessment:

```text
basis(group) = sum(all pre-assessment gross basis amounts in the group)
fee(group)   = R_policy(basis(group) * numerator / denominator)
```

All arithmetic MUST use checked intermediates wide enough for the maximum
accepted aggregate. Overflow, conversion failure, zero denominator, unknown
rounding rule, malformed group, or unsupported basis fails the complete
transaction.

Core MUST aggregate before rounding. Reordering or splitting identical effects
inside an envelope MUST NOT change the assessment. The canonicalization rule
MUST define treatment of duplicate source-destination pairs, cycles, refunds,
and source/sink aliases. A final net account delta alone is not a generic gross
basis because cycles and pass-through transfers can disappear under netting.

Each canonical assessment has an identifier derived from at least the Core
major, policy ID, intent or envelope digest, fill sequence where applicable,
policy revision, assessment class, and group key. Core MUST synthesize the
assessment exactly once. The engine cannot address the fee vault or construct
an authoritative protocol fee leg.

### Rounding

The allowed rounding functions form a closed, versioned set. The selected
rounding function is policy-bound and cannot be supplied by the caller or
engine. Every accepted function and the complete cumulative assessment
function derived from it MUST be deterministic, non-negative, and monotonically
non-decreasing over an increasing non-negative basis.

The specification for each function MUST define:

- exact integer formula and intermediate width;
- behavior at zero and the smallest asset unit;
- interaction with minimum and maximum rules;
- behavior when a cap binds; and
- test vectors at every discontinuity and integer boundary.

Rounding each effect separately is forbidden. Per-effect floor rounding enables
dust splitting, while per-effect ceiling rounding can multiply charges when an
equivalent plan is split. Independent envelopes cannot in general be proven to
be one economic action; a deliberate flat or minimum component is the only
generic envelope-level protection against cross-envelope dust splitting.

### Partial fills

A stored or partially filled intent MUST carry cumulative assessment state for
each assessment group. Let `A_policy(x)` be the policy's monotone cumulative
rate assessment after its rounding and every intent-scoped minimum or maximum:

```text
rate_fee_delta =
  A_policy(cumulative_basis_before + fill_basis)
  - A_policy(cumulative_basis_before)
```

Core atomically advances cumulative basis, cumulative assessed fee, and fill
sequence with the settlement. The subtraction MUST be checked and the monotonic
policy rule must make underflow impossible. Reordering or splitting fills MUST
produce the same cumulative rate assessment as the same accepted total basis
under the same policy revision.

An explicitly envelope-scoped flat component, minimum, or maximum is applied
once to each successful fill envelope and is intentionally outside this
partition-invariance claim. Quotes and events MUST distinguish the cumulative
rate component from every envelope-scoped component.

A policy revision MUST NOT change midway through an intent unless the user's
original authorization explicitly defines that transition. Cancellation,
expiry, replacement, replay, and concurrent-fill rules must preserve cumulative
state and exactly-once assessment.

## Fee assets and asset profiles

Every fee asset belongs to an exact, versioned asset profile. The profile binds
at least the asset program, mint or native asset identity, supported extension
set, authority assumptions, gross/net accounting rule, callback closure,
liveness boundary, and exit treatment.

Unknown programs, extensions, lifecycle states, or authority configurations
fail closed. A claim such as `Token-2022 supported` is insufficient; each
accepted extension combination is a separate profile.

### Classic SPL Token

Classic SPL Token is the initial strong candidate because Core can authenticate
the mint, token program, owner, authority, and exact pre/post token-account
deltas without transfer hooks or withheld fees.

The profile still MUST define treatment of mint freeze authority, frozen token
accounts, account delegates, close authority, native wrapping, and transfer
liveness. Unsupported configurations are rejected before protected movement.

### Native SOL

A native SOL assessment requires an authenticated source signer, an exact
prefunded PDA escrow, or an accepted sponsor. Core cannot debit an arbitrary
system account without authority. Network fees remain distinct from a native
SOL protocol assessment.

### Token-2022 transfer-fee profile

A Token-2022 transfer-fee profile MUST distinguish:

```text
G = gross source debit
T = issuer-controlled withheld amount
N = spendable destination credit

N = G - T
```

Only the Core-verified spendable fee-vault credit `N` can create protocol
liability. The withheld amount is controlled by the Token-2022 fee authorities
and MUST NOT be counted as protocol revenue.

The profile must choose one of two explicit charge models:

- gross-defined: user debit `G` is fixed and liability is observed `N`; or
- net-defined: desired vault credit `N` is fixed, Core calculates the required
  `G`, verifies `T`, and the user caps gross debit and external tax.

Landing-time transfer-fee configuration and epoch MUST be revalidated. A stale
quote, configuration change, inverse-fee overflow, non-positive net credit, or
user-limit breach fails the transaction.

When several protocol-controlled recipient buckets use the same transfer-fee
asset, Core SHOULD aggregate their collection into one fee-vault transfer and
create internal liabilities from the verified net credit. Direct settlement
transfers to multiple recipients can incur repeated issuer tax and add writable
accounts.

An aggregated transfer MUST define either exact per-bucket net targets or one
deterministic allocation of the observed net credit. The sum of liabilities
created across all buckets MUST equal `N`. No beneficiary may claim a
pre-transfer gross amount as spendable funding.

A later claim can incur a new external transfer fee. Liability MUST state
whether it promises a gross vault debit or a net recipient credit. The initial
candidate SHOULD denominate liability in vault-debit units and report the
recipient's observed net credit separately; it MUST NOT promise a net amount
that mutable issuer policy can invalidate.

### Transfer hooks and other extensions

Transfer hooks execute external code during protected movement and may require
additional writable accounts. They are a separate callback and account-closure
profile, not an automatic extension of Classic SPL or the transfer-fee profile.

An accepted hook profile must bind and revalidate the hook program code policy,
extra-account-meta configuration, effective privileges after duplicate-key
union, side-payment authorities, callback phase, compute and account limits,
post-deltas, and exit liveness. Protected-account aliases through hook extras
must fail before movement.

Permanent Delegate, pausing, freeze/default-state controls, CPI Guard, memo
requirements, confidential or otherwise unobservable balances, and mutable
extension authorities each require explicit profile treatment. Until accepted,
they are rejected.

### Custom assets

Opaque or custom assets cannot serve as a CoreVerified percentage basis merely
because an engine labels a movement or price. Core can still collect a fixed
assessment in a separate supported fee asset.

A rate on a custom-asset effect requires a separately accepted driver/profile
that proves exact base-unit debit and credit, authority, code identity,
callbacks, liveness, and postconditions. Even then, Core attests the base-unit
effect rather than economic value.

For NFTs or compressed assets, metadata or an engine-declared sale price is not
a protocol fee basis. An exact CoreVerified payment leg may be assessed under
its own supported asset profile.

## Collection, recipients, liabilities, and claims

### Collection truth

Only an exact Core-verified spendable fee-vault credit creates accounted fee
liability. Raw donations, withheld token fees, rebases, unsolicited transfers,
and opaque engine assertions create no liability or recipient claim.

Events SHOULD distinguish at least:

- policy-computed assessment;
- gross source debit;
- external asset tax or withheld amount;
- spendable fee-vault credit;
- liability created; and
- policy and recipient-set revisions.

`assessed`, `funded`, `claimable`, `claimed`, `recipient net received`, and
offchain-valued revenue are different states and MUST NOT be reported as one
number.

### Recipient authority

The protocol beneficiary comes from Core policy. A builder beneficiary comes
from accepted market policy. Integrator and referrer identities come from exact
user authorization and an accepted recipient set. None may be replaced by an
execution-time destination supplied only by the caller or engine.

Recipient-set changes affect only future assessments under the new revision.
Existing liabilities retain the beneficiary and split snapshot under which they
were funded.

A recipient-signed claim may choose a validated account owned by that recipient.
A permissionless keeper may claim only to the canonical policy-bound
destination. Claims MUST reject a wrong mint, token program, asset profile,
owner, destination, policy revision, or beneficiary.

### Split accounting

Settlement SHOULD make one Core-owned vault credit and account recipient
liabilities internally rather than issuing a transfer to every beneficiary in
the hot path.

Split rounding must be deterministic. A policy may assign rounded subordinate
shares and give the designated residual beneficiary the exact remainder. If
long-run proportional entitlement is required, the shard MUST use cumulative
funding:

```text
entitlement_i =
  floor(cumulative_net_funding * share_i / share_denominator)
  - previously_allocated_i
```

Advancing a recipient checkpoint without allocating and preserving the exact
amount is forbidden. The sum of recipient liabilities after every funding event
MUST equal the total accounted spendable funding, less completed claims.

### Claim atomicity

A claim MUST atomically:

1. authenticate shard, vault, asset profile, beneficiary, and destination;
2. bound the requested amount by the beneficiary's recorded liability;
3. execute the supported transfer;
4. reload and verify exact vault debit and destination credit under the asset
   profile; and
5. reduce liability by the profile-defined claimed amount.

Failure at any stage leaves vault balance, liabilities, claimed totals, and
recipient state unchanged. Donation surplus cannot be claimed through a fee
liability. A claim authority cannot redirect another beneficiary or claim more
than the Core-accounted amount.

## Sharding and writable-account discipline

The settlement hot path MUST NOT write a global treasury account, global fee
counter, global recipient accrual, global sequence, or mutable fee registry.
Such accounts serialize otherwise independent transactions.

A fee collection shard MUST be no coarser than the settlement's unavoidable
writable partition. Candidate identity inputs include:

```text
FeeShardId = hash(
  core_major,
  fee_asset_profile,
  fee_asset,
  policy_revision,
  collection_partition,
  shard_index
)
```

The exact seed codec requires a separate state-layout decision. Each shard owns
or authenticates its fee vault and keeps only shard-local funding, beneficiary
liabilities, and claimed totals.

Policy and recipient configuration are read-only during settlement. Fee shards
are created before the hot path; settlement MUST NOT rely on `init_if_needed`
for a collection account. Claims and sweeps write only the selected shard,
vault, shard-local beneficiary record if separate, and destination. They MUST
NOT take a global market, domain, or treasury write lock solely to update
totals.

Protocol-wide totals are indexer views over canonical events and onchain shard
state, not a required global writable counter.

Shard count is not fixed by this constitution. The smallest configuration that
meets accepted throughput, collision, packet, account-lock, compute, rent, and
claim-cost targets must be selected from measured SBF results. Adding shards
without measurement increases state and claim complexity without proving
throughput.

## State-only transitions, cancellation, and exit

A successful engine or Core state transition with no assessable protected asset
effect can fund only a fixed assessment through a supported fee asset. Without
an authenticated signer, delegate, escrow, or sponsor, Core cannot collect it.
The event and client MUST call it an execution or settlement assessment, not a
trade fee.

An engine-independent emergency exit MUST carry no protocol, builder,
integrator, or referral assessment. The user still pays network fees and any
unavoidable external asset-program tax. Fee policy, fee recipient, engine,
market pause, and offchain services MUST NOT be able to block or redirect this
exit.

Engine-independent does not imply issuer-independent. Every custody asset
profile MUST state whether freeze, pause, hook, or other asset-program authority
can still block transfer. A profile with such a dependency cannot claim an
unconditional exit. It must either be rejected for persistent strong-profile
custody or expose the narrower exit class before deposit authorization.

A cancellation that occurs before protected settlement SHOULD carry no protocol
assessment. If an accepted lifecycle requires bounded storage or rent cleanup,
that reimbursement must be fixed by the Core major, separately displayed, and
must not become mutable recipient revenue. Cancellation cannot require an
engine callback when it is the user's safety path.

A normal state-only execution may charge the policy-bound fixed assessment only
when it commits successfully and remains within every user limit.

## Governance boundaries

The closed assessment modes, basis semantics, rounding set, capability
monotonicity rule, liability truth, claim atomicity, emergency-exit exemption,
and asset-profile fail-closed rule belong to the Core major. Ordinary policy
governance cannot weaken them.

Governance may configure only explicitly delegated fields, within immutable
Core-major caps. Production policy changes require:

- an authenticated proposal and execution path;
- a public activation delay;
- an engine-independent user exit window where custody exists;
- a new monotonically increasing policy revision;
- no effect on previously authorized intents unless the user accepted the
  transition; and
- no reassignment of existing liabilities.

Protocol fee beneficiary, builder beneficiary, policy updater, security pause,
upgrade authority, and claim authority are distinct roles. No generic
administrator may move user or domain assets, claim another beneficiary's fees,
rewrite existing liability, bypass a user ceiling, or expand an asset profile.

A security pause may stop new execution where separately justified, but it MUST
NOT redirect funds or disable the minimal independent exit. A remaining Core,
engine, token, hook, or driver upgrade authority is part of the relevant trust
and liveness model and must be bound by the accepted code policy.

This constitution intentionally does not select a rate, flat amount, economic
cap, fee asset, recipient, governance key, activation delay, or shard count.
Those are separate accepted policy and deployment decisions and require
economic, security, resource, and exit evidence.

## Evidence and anti-manipulation rules

Canonical events SHOULD use objective names such as:

- `CoreEnvelopeCommitted`;
- `CoreVerifiedEffect`;
- `ProtocolAssessmentFunded`;
- `RecipientLiabilityCreated`; and
- `EngineAttestedDigest`.

Opaque engine meaning MUST NOT be emitted as `TradeExecuted`, `SwapVolume`,
`NFTSale`, or another Core-certified semantic event unless a separate accepted
profile proves that exact meaning.

Core cannot prove beneficial ownership, unique humans, organic volume, or the
absence of self-trading. Protocol emissions, rebates, referral rewards, or
builder payouts MUST NOT be based on raw volume, transaction count, unique
wallet count, or engine-declared trade events. Any incentive payout must be
bounded by an explicit funded bucket and must not be presented as Sybil-proof.

## Acceptance tests

All cross-program and token cases require real SBF artifacts and runtime-faithful
account, CPI, rollback, and lock behavior. Host unit tests alone are not
sufficient. Property and stateful tests use the exact accepted integer model.

### Authority and policy tests

- `protocol_policy_only_source`: caller and engine attempts to supply a zero,
  cheaper, duplicate, or alternate policy fail before protected movement.
- `recipient_substitution_rejected`: caller, router, and engine recipient
  replacements fail for every assessment class.
- `policy_revision_bound`: stale, future, substituted, or retired revisions fail
  unless the exact transition was user-authorized.
- `user_limit_is_reject_not_clip`: every per-class, external-tax, total-fee, and
  total-source limit causes full rollback when exceeded.
- `assessment_exactly_once`: nested callbacks, repeated phases, duplicate
  effects, and replay cannot omit or multiply one canonical assessment.
- `failed_transaction_no_protocol_revenue`: engine, token, fee, accounting, and
  postcondition failure leave no fee credit, liability, or recipient state.
- `capability_monotonicity_matrix`: every profile pair with overlapping
  expressiveness is tested for a cheaper emulation path.
- `semantic_label_has_no_fee_power`: changing only an engine product label does
  not change policy selection or fee output.

### Aggregation and arithmetic tests

- `permutation_invariant`: every permutation of the same canonical effects
  produces the same groups, basis, and assessment.
- `split_merge_invariant`: splitting or merging equivalent effects inside one
  envelope leaves the assessment unchanged.
- `duplicate_cycle_refund_matrix`: duplicates, cycles, pass-through legs,
  refunds, and aliases are canonicalized or rejected exactly as specified.
- `fee_leg_excluded_from_basis`: protocol, builder, integrator, external tax,
  and claim legs cannot create accidental fee-on-fee.
- `multi_asset_never_netted`: basis and liability in one asset cannot be offset
  by an effect or donation in another asset.
- `cross_asset_rate_requires_profile`: a different basis and fee asset fails
  unless the exact accepted conversion profile and all oracle bounds are bound.
- `source_account_split_same_principal`: moving one principal's basis across
  several source accounts does not create additional rounding groups.
- `rounding_boundary_vectors`: zero, smallest unit, every formula
  discontinuity, minimum, maximum, cap, maximum aggregate, and conversion
  boundaries match the accepted model.
- `checked_arithmetic_matrix`: multiply, sum, subtraction, conversion, inverse
  fee, and entitlement overflow all fail closed.
- `cross_envelope_dust_disclosure`: tests demonstrate that independent
  envelopes are independent fee events; no semantic aggregation claim is made.

### Partial-fill and replay tests

- `cumulative_fill_equivalence`: all partitions and orders of the same accepted
  cumulative basis produce the same final cumulative rate assessment; declared
  envelope-scoped components are accounted and disclosed separately.
- `partial_fill_policy_stability`: a revision change cannot alter a live intent
  without exact user authorization.
- `partial_fill_replay_matrix`: duplicate fill, zero-basis fill, fee-only fill,
  cancellation, expiry, replacement, and concurrent fills preserve exactly-once
  cumulative state.
- `cancel_execute_race`: runtime ordering commits either cancellation or the
  valid fill, never both.

### Funding and liability tests

- `fee_omission_redirect_duplication`: missing, redirected, and duplicated
  protocol funding all fail.
- `observed_credit_creates_liability`: liability equals only the accepted
  profile's verified spendable vault credit.
- `fee_vault_donation_no_liability`: raw donations, withheld amounts, and
  unsolicited credits do not create liability or entitlement.
- `split_conservation`: shard funding equals outstanding liabilities plus
  completed profile-defined claims at every state transition.
- `recipient_change_not_retroactive`: new policy recipients cannot claim old
  revision liabilities.
- `claim_redirect_overdraw_matrix`: wrong beneficiary, destination, mint,
  program, profile, revision, or amount fails without state change.
- `claim_late_failure_rollback`: token, hook where accepted, post-delta, and
  accounting failures leave vault and liability unchanged.
- `zero_entitlement_checkpoint`: a zero rounded entitlement cannot consume or
  lose future beneficiary value.

### Asset-profile tests

- `classic_spl_profile_matrix`: owner, mint, token program, delegate, close
  authority, freeze state, native wrapping, and exact pre/post deltas match the
  accepted profile.
- `unsupported_extension_fails_closed`: every unknown or unaccepted Token-2022
  extension combination fails before protected movement.
- `transfer_fee_gross_withheld_net`: `G`, `T`, and `N` match landing-time token
  behavior and only `N` funds liability.
- `transfer_fee_rounding_grid`: zero, smallest, capped, full-withhold,
  inverse-gross, and maximum-integer cases match the accepted profile.
- `transfer_fee_epoch_or_config_change`: a landing-time change either remains
  within all exact user bounds or reverts atomically.
- `withheld_harvest_no_protocol_claim`: fee-authority harvesting cannot change
  Core-accounted liability.
- `claim_outgoing_external_tax`: gross vault debit, external tax, recipient net
  credit, and liability reduction follow the declared claim denomination.
- `transfer_hook_extra_alias`: protected aliases or privilege escalation through
  hook extra accounts are detected after duplicate-key privilege union.
- `transfer_hook_reentry_matrix`: every Core execution, claim, exit, policy, and
  admission reentry attempt either fails or is covered by an explicitly
  accepted phase model with full rollback.
- `custom_asset_semantic_nonclaim`: custom metadata, price, or engine labels
  cannot select a percentage basis without an accepted CoreVerified profile.
- `state_only_requires_fixed_funding`: a state-only or zero-effect commit cannot
  manufacture a percentage basis and can charge only its authorized fixed
  assessment.

### Sharding and liveness tests

- `unrelated_settlements_no_global_fee_write`: runtime account graphs for
  unrelated partitions share no writable fee, recipient, sequence, or total
  account.
- `fee_shard_canonical`: wrong asset, profile, policy, partition, index, vault,
  authority, or bump fails before collection.
- `shard_collision_benchmark`: measured shard configurations report throughput,
  collision rate, packet bytes, unique locks, writable locks, compute, rent,
  and claim cost at target load.
- `claim_does_not_lock_market`: a claim requires no unrelated global market or
  domain write.
- `max_valid_resource_headroom`: maximum accepted recipients, effects, assets,
  shards, hook extras, and claims retain the separately approved runtime
  headroom; maximum plus one fails before movement.

### Exit, governance, and evidence tests

- `emergency_exit_has_no_protocol_assessment`: protocol, builder, integrator,
  and referral buckets remain zero on the independent exit; network and
  unavoidable external asset tax remain correctly disclosed.
- `exit_without_engine_or_fee_authority`: missing, malicious, paused, or upgraded
  engine and fee-policy actors cannot block the minimal exit.
- `exit_class_matches_asset_authority`: an asset with a blocking issuer, hook,
  freeze, or pause authority cannot be admitted under an unconditional-exit
  claim.
- `cancellation_fee_boundary`: pre-settlement cancellation carries no protocol
  revenue charge; any accepted fixed cleanup reimbursement is exact and cannot
  be redirected.
- `timelock_and_exit_window`: a policy or beneficiary change cannot activate
  before its accepted delay and custody exit window.
- `governance_role_matrix`: compromise of each fee, beneficiary, pause,
  admission, and upgrade role grants only documented powers.
- `event_accounting_parity`: events, onchain shard state, claims, and client quote
  fields agree for assessed, funded, liable, claimed, external-tax, and net
  amounts.
- `opaque_event_honesty`: an opaque engine cannot cause Core to emit certified
  trade, swap, sale, royalty, or volume semantics.
- `batching_and_external_entrypoint_nonclaim`: several opaque actions in one
  envelope do not create invented per-action fees, and an engine's external
  entrypoint is never reported as protocol-taxed.
- `wash_and_self_referral_scenarios`: circular flow, common funder, self-router,
  self-referral, and multi-wallet variants create no protocol claim of organic
  volume or unique users and unlock no raw-volume reward.

## Acceptance boundary

No fee ABI is accepted for production until:

1. the closed constitution and exact integer codec are approved;
2. user authorization binds every fee and external-tax limit;
3. recipient liabilities and engine-independent claims are implemented and
   adversarially tested;
4. every accepted asset profile has exact gross/net, authority, callback,
   liveness, and exit evidence;
5. the shard topology meets measured runtime and economic targets without a
   global writable hotspot;
6. client quotes and canonical events preserve the evidence distinctions in
   this document;
7. governance cannot retroactively change liabilities or block independent
   exit; and
8. an external security review covers assessment algebra, token behavior,
   claims, sharding, authority separation, and atomic rollback.

Until those gates pass, the current fee code remains experimental measurement
logic rather than accepted protocol economics.
