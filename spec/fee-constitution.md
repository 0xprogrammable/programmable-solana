# Fee constitution

Status: Candidate V1; not accepted for production until the acceptance boundary
in this document has passed.

This document is the Solana binding of the selected V1 protocol-fee
constitution. V1 charges a mandatory nominal rate of five basis points on the
exact objective basis defined below. It selects no flat protocol assessment,
minimum charge, cross-asset conversion, oracle-valued notional, or
engine-defined semantic fee. It is not a claim that the current experiments
implement these rules.

## Purpose

Programmable permits engines with arbitrary and partly opaque semantics. The fee
system must therefore charge only for facts that Core can authenticate without
pretending to know what an engine-defined state transition means.

This constitution separates:

- what Core can enforce inside a committed Core settlement envelope;
- what Core can observe but must describe without product semantics;
- what remains an engine attestation; and
- what no onchain mechanism can make non-bypassable.

The constitution is part of a Core major. Production Core V1 fixes its protocol
assessment mode, basis, rate, rounding, exemptions, same-asset rule, and
collector. A market policy can define only separate builder or integrator
economics within exact user bounds; it cannot alter the protocol constitution.

## Normative language

`MUST`, `MUST NOT`, `SHOULD`, and `SHOULD NOT` describe candidate requirements
that must be satisfied before the affected feature can be accepted. They do not
describe the disposable probe unless a separate result document proves that
they hold.

## Honest enforceability boundary

### Enforceable claims

Within a successful Core settlement route, Core can enforce:

1. one authenticated protocol-assessment set for each committed settlement
   envelope, with exactly one canonical entry for every applicable V1
   assessment group;
2. the immutable five-basis-point rate on an exact canonical group of objective
   Core-verified principal-funded gross debits;
3. additive same-asset funding through an authenticated principal or sponsor;
   and
4. user ceilings over every Core assessment, external asset tax, and total
   source debit.

Core MUST derive every mandatory protocol assessment from the immutable
Production Core V1 constitution and collector identity. The caller and engine
MUST NOT supply a zero policy, alternate collector, cheaper semantic class,
optional assessment flag, or second protocol schedule.

An assessment is non-bypassable only inside the Core envelope. Permissionless
engines and external programs may expose entrypoints outside Core. The protocol
MUST NOT claim to tax those entrypoints.

All Core writes, engine writes, transfers, fee accounting, and emitted state
evidence are atomic. A failed transaction cannot leave protocol fee revenue or
liability behind. Solana network fees and failed-transaction metadata are
outside this rollback statement.

### Fee-enforcement disclosure

Every market and execution profile MUST expose `fee_enforcement` as
`CORE_ENFORCED`, `PARTIAL`, or `NONE`.

- `CORE_ENFORCED` means every applicable principal-funded protected debit
  inside the declared Core route is assessed by V1.
- `PARTIAL` means some value settles through Core but the same engine, market,
  asset, or economic activity has another reachable path outside that boundary.
- `NONE` means Core cannot authenticate and collect the percentage assessment
  for the relevant value movement.

Public reporting MUST distinguish Core-assessed gross debit, protocol gross
debit, fee-vault net credit, outstanding liability, claimed vault debit,
engine-attested activity, and offchain-valued analytics. “Programmable charges
five basis points” means the V1 integer formula on every applicable
`PrincipalFundedGrossDebitV1` group, not five basis points of all engine
activity, all DEX volume, oracle-valued notional, or off-Core activity.

`CoreAssessedGrossDebitByAsset` is the reporting aggregate of committed group
bases with the same asset profile and asset after their independent assessment.
It is not a second fee basis or an alternate onchain formula, and Core MUST NOT
recompute one fee over that cross-group aggregate. Unlike assets remain separate
series. Only offchain analytics with an explicit price source, observation time,
and methodology may combine them as `OffchainValuedCoreAssessedGrossDebit`.

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
Consequently, the V1 rate on a Core-verified debit is enforceable as a rate on
that debit, not automatically as a trade fee or all engine activity.

Core MUST NOT sum different asset units into a single volume value. Any offchain
conversion to a quote currency needs an explicit price source, observation
time, and manipulation policy and remains indexer evidence rather than onchain
fee truth.

## Assessment classes

Every Core-collected amount belongs to exactly one class. Solana base and
priority fees remain outside this taxonomy.

### Protocol assessment

#### ProtocolAssessmentV1

Every successful Core envelope containing an applicable
`PrincipalFundedGrossDebitV1` MUST create exactly one mandatory
`ProtocolAssessment` for every canonical assessment group.

The immutable V1 constants are:

```text
protocol_constitution_id = ProtocolAssessmentV1
assessment_mode         = RateOnPrincipalFundedGrossDebit
rate_numerator          = 5
rate_denominator        = 10_000
reduced_denominator     = 2_000
rounding_rule           = FloorAfterCanonicalCumulativeAggregation
flat_component          = 0
minimum                 = 0
policy_maximum          = None
fee_asset               = basis_asset
fee_funding_mode        = AdditiveSameAsset
assessment_amount_rule  = GrossSourceDebit
```

Five basis points means exactly `5 / 10_000`, equivalently `1 / 2_000`, before
integer rounding. The caller, router, engine, builder, integrator, recipient,
policy authority, or security authority MUST NOT disable, discount, duplicate,
redirect, or change these constants.

The amount produced by the V1 integer formula is the exact gross source debit
of the protocol-assessment leg. Asset-program withholding may reduce the
spendable fee-vault credit but MUST NOT cause Core to inverse-gross-up that
protocol debit.

A different rate, denominator, rounding rule, fee asset, flat component,
minimum, maximum, basis definition, or protocol collector requires a new Core
major and a new deployment identity. It cannot be introduced through an
in-place policy revision.

`ProtocolAssessment` is the mandatory Core-derived bucket. In V1 its mode,
basis, rounding, rate, lack of minimum or maximum, same-asset funding, and
collector identity come only from the immutable Core deployment descriptor.
Market policy cannot change them.

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
user can self-integrate or self-refer. V1 does not split the protocol assessment
with an integrator or referrer in Core. Any later treasury-funded campaign is a
separate treasury distribution and MUST be described as a rebate program rather
than proof of unique users or organic routing.

### External asset tax

`ExternalAssetTax` covers issuer- or asset-program-imposed amounts, including a
supported Token-2022 transfer fee. It is not selected by Core, is not protocol
revenue, and MUST be quoted, bounded, observed, and emitted separately.

Solana base and priority fees are also external to protocol assessments and
MUST be displayed separately by clients.

### Separation from other economics

`ProtocolAssessmentV1` is always the immutable five-basis-point assessment
defined above.

A `BuilderAssessment` is optional, additional, market-revision-bound,
separately displayed, and subject to exact user ceilings. An
`IntegratorAssessment` exists only when the user authorization binds its
identity, recipient, asset, formula, and maximum. Direct Core execution MUST
NOT silently create an integrator or referral assessment.

LP or maker compensation is market-defined economics, not Programmable
protocol revenue. It may be an explicit bucket, spread, inventory change,
auction result, or another engine economic effect. Core reports it as
Core-verified only when it can authenticate the exact bucket; otherwise it
remains engine-attested. Embedded engine economics do not reduce the protected
protocol basis.

Separate classification is an accounting distinction, not a basis exemption.
Any builder, integrator, referral, campaign, interface, LP, maker, cleanup,
rebate, sponsor, or similarly named amount that consumes an authenticated
principal spend capability remains part of `PrincipalFundedGrossDebitV1`,
unless a closed objective exemption of this Core major applies. The principal
whose spend capability funds that debit is the assessment principal; calling
that principal a sponsor does not create an exemption.

Protocol, builder, integrator, referral, LP, interface, external-asset, and
base-chain costs MUST remain separately named in quotes, receipts, events, and
analytics. V1 does not share its protocol assessment with another bucket. The
protocol assessment debit itself is excluded from its basis, so the protocol
assessment is not recursively assessed. Other principal-funded charge buckets
remain inside the inclusive gross basis and cannot be relabeled out of it.

## Policy and user authorization

The immutable V1 deployment descriptor MUST bind at least:

```text
ProtocolDeploymentFeeDescriptorV1 {
  core_deployment_id
  protocol_constitution_id = ProtocolAssessmentV1
  protocol_collector_id
  assessment_mode = RateOnPrincipalFundedGrossDebit
  rate_numerator = 5
  rate_denominator = 10_000
  reduced_denominator = 2_000
  rounding_rule = FloorAfterCanonicalCumulativeAggregation
  flat_component = 0
  minimum = 0
  policy_maximum = None
  fee_asset_rule = SameAsBasisAsset
  fee_funding_mode = AdditiveSameAsset
  assessment_amount_rule = GrossSourceDebit
  collection_partition_rule
  shard_count
}
```

The shape is illustrative rather than a public ABI. Exact codecs and field
widths require their own accepted specification. None of the fixed values or
the collector identity has an in-place setter.

A user authorization MUST contain one record per principal and asset group and
bind at least:

```text
FiveBpsUserLimitRecord {
  core_deployment_id
  protocol_constitution_id
  protocol_collector_id
  authorization_scope_id
  assessment_principal
  fee_funding_principal
  basis_asset_profile
  basis_asset
  exact_builder_policy_digest
  exact_integrator_and_recipient_digest
  maximum_cumulative_gross_protected_debit
  maximum_cumulative_protocol_assessment
  maximum_cumulative_builder_assessment
  maximum_cumulative_integrator_assessment
  maximum_cumulative_explicit_lp_compensation
  maximum_cumulative_external_asset_tax
  maximum_cumulative_other_authorized_debit
  maximum_cumulative_actual_total_source_debit
  expiry
}
```

Every limit is denominated in the single asset identified by that record. A
multi-asset authorization binds the complete ordered digest of all records. No
scalar total may span unlike asset units, and Core MUST NOT use implicit
decimals, prices, or oracles to compare them.

Core MUST NOT silently clip a computed amount to the user's maximum. If any
post-fill cumulative user limit is exceeded, the complete envelope fails. The
fixed V1 formula has no policy cap, minimum, flat component, or mutable policy
revision.

## Objective fee bases

### PrincipalFundedGrossDebitV1

The V1 protocol basis is an objective protected debit, not a trade, swap,
deposit, sale, price, volume, or other engine-defined semantic label.

A protected amount is included in `PrincipalFundedGrossDebitV1` only when all
of the following hold:

1. it consumes an authenticated spend capability belonging to the assessment
   principal;
2. the source is that principal's authenticated asset account or an exact
   Core-accounted claim attributable to that principal;
3. the asset and settlement behavior belong to an accepted fungible asset
   profile;
4. Core verifies the exact gross source debit in the asset's smallest native
   unit;
5. the debit commits successfully inside the current Core envelope; and
6. the debit is not an objectively exempt Core-major capability.

The basis is measured before issuer withholding or other external asset tax.
An external tax is reported separately but does not convert the gross basis
into a net-credit basis.

The basis MUST exclude:

- the protocol assessment itself;
- Solana base and priority fees;
- internal router, engine, pool, vault, or domain inventory movements that do
  not consume an assessment principal's spend capability;
- output credits, price-improvement or reward credits, unsolicited donation
  credits, and unrelated credits that do not consume the assessment principal's
  spend capability;
- an exact same-asset one-for-one transfer between two Core-authenticated
  accounts or claims of the same assessment principal, with no beneficial-owner
  change and no new protected right;
- an exact same-asset reversible custody relocation accepted under the narrow
  profile below; and
- an exact closed Core-major cancellation, claim, migration, withdrawal, or
  exit exemption that commits no other protected economic effect.

A builder, integrator, referral, campaign, interface, LP, maker, cleanup,
rebate, sponsor, claim, external-tax, or other semantic label does not by
itself create an exemption. If the amount consumes an authenticated principal
spend capability and is not objectively exempt above, it is included. The
protocol assessment itself remains excluded to prevent recursive assessment.

An external asset tax withheld inside an applicable gross source debit is an
inclusive typed subcomponent of that gross debit: it is reported separately but
is neither subtracted from the basis nor added to it a second time. External
asset behavior on the protocol-assessment transfer remains with the excluded
protocol-assessment leg.

In `CORE_ENFORCED` mode, Core MUST reject before movement any principal-funded
protected debit it cannot classify objectively. It MUST NOT assign zero basis
because classification would require an engine-supplied semantic label.
`PARTIAL` and `NONE` remain the only honest modes for unsupported or off-Core
paths.

An embedded spread or opaque engine economic charge cannot be separated from
the principal's protected input. When the complete protected input is debited,
the complete applicable gross debit is basis. Separately identifying a fee leg
does not exclude it; only the protocol-assessment leg itself and the enumerated
closed objective Core-major exemptions are excluded.

An engine cannot create an exemption by naming an effect `deposit`,
`liquidity`, `withdrawal`, `migration`, `refund`, or another product action.

### Deposits, withdrawals, claims, cancellation, migration, and exit

A liquidity or inventory contribution is assessable when it consumes the
principal's protected spend capability. Receiving an engine-owned position,
share, receipt, point, reward, or opaque claim does not make that debit exempt.

A debit is an exempt `ReversibleCustodyRelocationV1` only when Core proves all
of the following without an engine callback:

1. the same principal retains the complete beneficial claim;
2. the claim is denominated one-for-one in the same asset and asset profile;
3. no other principal, domain, or engine receives an asset or protected claim;
4. the relocation creates no cross-asset right;
5. the principal can reverse it through an engine-independent Core path; and
6. reversal returns the same asset subject only to disclosed unavoidable
   external asset behavior.

A normal withdrawal or entitlement claim has no V1 rate basis when it only
credits the principal and consumes no new principal-funded spend capability.

Cancellation before protected settlement, an engine-independent emergency
exit, an exact Core-major fee-vault or entitlement claim that only credits the
entitled principal and consumes no new principal-funded spend capability, a
qualifying one-for-one migration, and a failed or reverted envelope MUST create
zero protocol assessment. A completed partial fill is not reversed by later
cancellation, expiry, migration, withdrawal, or exit.

A state-only action with no applicable protected debit has zero V1 protocol
assessment. For an NFT or other non-fungible action, an accepted fungible
payment leg is assessed normally; Core MUST NOT invent an oracle-valued basis
from the NFT, metadata, floor price, or engine declaration.

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

### Canonical assessment groups

Applicable debits are grouped by:

```text
FiveBpsAssessmentGroupKey = (
  core_deployment_id,
  protocol_constitution_id,
  authorization_scope_id,
  assessment_principal,
  basis_asset_profile,
  basis_asset
)
```

`authorization_scope_id` is the immutable intent identity for a stored or
partially filled intent and the immutable envelope authorization identity for a
one-shot execution. Different principals, assets, asset profiles, Core
deployments, or authorization scopes form different groups. Market, engine,
router, instruction, callback, account, and hop identity MUST NOT split an
otherwise identical group.

For one fill:

```text
B_fill =
  sum(applicable principal-funded gross debits)
  - sum(valid linked same-envelope refunds)
```

A valid linked refund restores spendable value to the exact same principal in
the same asset and profile, identifies the originating debit, occurs in the
same atomic Core envelope, is measured from actual restored balance or claim,
and has exactly one allocation to exactly one originating debit.

Core MUST also prove refund provenance without trusting an engine label. The
restored value MUST either leave an exact Core-controlled debit escrow or claim
in which the unconsumed portion of that originating debit remained segregated,
or atomically reverse the exact original Core movement before that value is
mixed, consumed, or used by any downstream protected effect. A credit sourced
from engine, pool, router, sponsor, third-party, or unrelated inventory is not a
refund even if it has the same asset and amount.

Each refund MUST be allocated exactly once, and the sum of all refunds allocated
to one originating debit MUST NOT exceed that debit. It cannot offset another
principal, asset, authorization scope, or envelope. Outputs, rebates, rewards,
donations, reverse trades, and later transactions are not refunds.

Core MUST use checked arithmetic for debit aggregation, refund aggregation,
allocation caps, and subtraction, enforcing:

```text
0 <= B_fill <= sum(applicable principal-funded gross debits)
```

Duplicates, aliases, cycles, and pass-through effects MUST be deterministically
classified exactly once or rejected. A final net account delta alone is not
sufficient evidence of gross basis.

Each canonical assessment binds the Core deployment, constitution, complete
authorization scope, fill sequence, principal, asset profile, asset, and exact
group key. Core synthesizes it exactly once. The engine cannot address the fee
vault or construct an authoritative protocol fee leg.

### Five-basis-point integer formula

Let `B_before` be the previously committed cumulative basis and:

```text
B_after = B_before + B_fill
A(B)    = B div 2_000
F_fill  = A(B_after) - A(B_before)
```

`div` is unsigned integer division rounded down and `A(0) = 0`. An equivalent
remainder implementation is:

```text
r_before = B_before mod 2_000
F_fill   = (r_before + B_fill) div 2_000
r_after  = (r_before + B_fill) mod 2_000
```

Core MUST use checked arithmetic. It SHOULD use the reduced `1 / 2_000`
formula and MUST NOT multiply by five if doing so could overflow.

Rounding occurs once after canonical same-asset aggregation, never per effect,
account, leg, hop, callback, instruction, or recipient. A one-shot envelope
uses `B_before = 0`. A positive basis below 2,000 base units may produce a
zero-unit assessment. Core MUST NOT round upward or add a hidden one-unit
minimum because that would charge more than five basis points.

For every partially filled intent, Core atomically stores and advances
`cumulative_basis`, `cumulative_protocol_assessment`, `rounding_remainder`, and
`fill_sequence`. Reordering or partitioning one accepted cumulative basis MUST
produce the same final assessment. Replay, replacement, resume, or retry cannot
reset or duplicate this state. Cancellation or expiry discards a final
sub-unit remainder without creating liability; it is never transferred to an
independent intent.

Envelopes with independent `authorization_scope_id` values are independent fee
events. Fill envelopes sharing one stored intent's authorization scope are not.
V1 introduces no global writable dust remainder, flat fee, minimum, or
cross-scope semantic claim.

### Integer indivisibility and independent-envelope fragmentation

For every independent assessment group, the difference between the exact
rational five-basis-point amount and the collected integer amount is:

```text
0 <= (B / 2_000) - floor(B / 2_000) < 1 base unit
```

This is uncollected fractional protocol revenue, not a settlement deficit or a
claim against user assets. Creating new independent authorization scopes can
deliberately multiply the undercollection. Cumulative partial fills of one
intent cannot do so because they retain one cumulative basis.

V1 accepts this disclosed economics boundary instead of charging more than
five basis points through ceiling or a one-unit minimum, or introducing a
global mutable remainder whose ordering, identity, contention, and Sybil
semantics would become part of Core. Asset-profile and release economics MUST
quantify this fragmentation boundary. A minimum, ceiling rule,
cross-envelope remainder, alternate fee asset, or oracle conversion requires a
new Core major.

### Multi-leg, multi-hop, and multi-asset execution

Intermediate movements among routers, engines, liquidity domains, vaults, and
markets are not assessed again when they only propagate already funded route
inventory. One principal input routed through several internal hops is assessed
once. A fresh debit consuming a principal's spend capability is assessed.

If one principal supplies several source assets, each asset forms its own
group. Core MUST NOT sum, normalize, net, price, or convert unlike units. If
several principals fund one envelope, each principal forms independent groups.
A route split among several markets or engines inside one atomic envelope does
not create additional protocol assessments.

### Exact-input and exact-output behavior

Exact-input and exact-output are engine or client semantics; Core assesses the
verified actual protected debit. A quoted maximum, prefunded amount, requested
output, or unused authorization is not basis. Unused value is excluded only
when it never leaves the principal or is restored as a valid linked
same-envelope refund.

The V1 protocol assessment is additive and uses the basis asset. Source
accounting MUST treat `PrincipalFundedGrossDebitV1` as an inclusive gross
amount. Builder/integrator assessments, explicit LP or maker compensation,
external asset tax, and other principal-funded charges are overlapping typed
subcomponents for disclosure and MUST NOT be added to that gross basis a second
time. The protocol assessment gross debit is excluded from the basis and is
separately additive. Source accounting MUST distinguish all of those fields and
the actual total source debit. Exceeding any signed limit rejects the complete
settlement; Core does not clip the fee, reduce output, substitute an asset, or
partially commit.

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

For the `ProtocolAssessmentV1` leg, the only accepted charge model is
gross-defined:

```text
F = policy-computed five-basis-point assessment
G_protocol = F
T_protocol = observed issuer-controlled withheld amount
N_protocol = G_protocol - T_protocol
```

`F` and `G_protocol` are the assessed protocol amount. `N_protocol` is funded
vault credit and the maximum liability that credit can create. V1 MUST NOT
inverse-gross-up `G_protocol` to target a desired `N_protocol`. A net-defined
protocol assessment is a different amount rule and requires a new Core major.
Engine-defined non-protocol transfers may use profile-specific gross or net
targets inside their own exact user limits, but cannot alter the protocol
formula or reporting fields.

Landing-time transfer-fee configuration and epoch MUST be revalidated. A stale
quote, configuration change, invalid observed withholding, non-positive net
credit for a positive `F`, or user-limit breach fails the transaction. A zero
formula result creates no protocol transfer and no liability.

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
because an engine labels a movement or price. V1 has no fixed fallback fee. If
Core cannot authenticate a supported fungible principal-funded debit, a
`CORE_ENFORCED` route MUST reject before movement. An unsupported value path can
exist only with an honest `PARTIAL` or `NONE` profile outside that enforcement
claim. A successful Core envelope cannot assign zero basis merely by ignoring
an unclassifiable protected debit.

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

- policy-computed assessment, equal to the protocol leg's gross source debit;
- gross source debit;
- external asset tax or withheld amount;
- spendable fee-vault credit;
- liability created; and
- immutable protocol collector identity plus any separate builder or integrator
  revision.

`assessed`, `funded`, `claimable`, `claimed`, `recipient net received`, and
offchain-valued revenue are different states and MUST NOT be reported as one
number.

### Recipient authority

Every Core deployment binds exactly one immutable `protocol_collector_id` in
its deployment descriptor. Core contains no setter, governance instruction,
upgrade-time migration, router parameter, or emergency path capable of changing
it. Changing the protocol collector requires a new Core major and new program
ID. Existing markets, intents, fee vaults, and liabilities remain bound to the
old collector. An opt-in user migration creates new state under the new Core;
it never rewrites, redirects, or reassigns an old fee vault or accrued
liability.

A builder beneficiary comes from an accepted market-policy revision.
Integrator and referrer identities come from exact user authorization. None may
be replaced by an execution-time destination supplied only by the caller or
engine. No accrued liability may expire, be swept, forfeited, reassigned, or
applied to another beneficiary.

A recipient-signed claim may choose a validated account owned by that recipient.
A permissionless keeper may claim only to the canonical policy-bound
destination. Claims MUST reject a wrong mint, token program, asset profile,
owner, destination, policy revision, or beneficiary.

### Split accounting

Settlement MUST make one Core-owned protocol fee-vault credit and account the
immutable collector liability rather than invoking collector code in the hot
path. Builder and integrator buckets, when present, remain separate.

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

A Core-accounted protocol-liability claim is pull-based, creates no new
protocol assessment because it consumes no user principal spend capability,
and MUST atomically:

1. authenticate shard, vault, asset profile, beneficiary, and destination;
2. bound the requested amount by the beneficiary's recorded liability;
3. execute the supported transfer;
4. reload and verify exact vault debit and destination credit under the asset
   profile; and
5. reduce liability by the profile-defined claimed amount.

Failure at any stage leaves vault balance, liabilities, claimed totals, and
recipient state unchanged. Donation surplus cannot be claimed through a fee
liability. A claim authority cannot redirect another beneficiary or claim more
than the Core-accounted amount. A claim cannot touch user principal, provider
collateral, recovery reserves, another fee bucket, or another beneficiary.

## Sharding and writable-account discipline

The settlement hot path MUST NOT write a global treasury account, global fee
counter, global recipient accrual, global sequence, or mutable fee registry.
Such accounts serialize otherwise independent transactions.

A fee collection shard MUST be no coarser than the settlement's unavoidable
writable partition. Candidate identity inputs include:

```text
FeeShardId = hash(
  core_deployment_id,
  protocol_constitution_id,
  protocol_collector_id,
  fee_asset_profile,
  fee_asset,
  collection_partition,
  shard_index
)
```

The exact seed codec requires a separate state-layout decision. Each shard owns
or authenticates its fee vault and keeps only shard-local funding, beneficiary
liabilities, and claimed totals.

The protocol constitution and collector identity are immutable. Fee shards are
created before the hot path; settlement MUST NOT rely on `init_if_needed` for a
collection account. Claims write only the selected shard, vault, shard-local
collector liability, and destination. There is no sweep path. Claims MUST NOT
take a global market, domain, or treasury write lock solely to update totals.

Protocol-wide totals are indexer views over canonical events and onchain shard
state, not a required global writable counter.

Shard count is not fixed by this constitution. The smallest configuration that
meets accepted throughput, collision, packet, account-lock, compute, rent, and
claim-cost targets must be selected from measured SBF results. Adding shards
without measurement increases state and claim complexity without proving
throughput.

## State-only transitions, cancellation, and exit

A successful engine or Core state transition with no assessable protected asset
effect has zero V1 protocol assessment. V1 has no fixed or minimum protocol
fee.

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

A cancellation before protected settlement that consumes no new
principal-funded spend capability MUST carry no protocol assessment. A
separately authorized storage or rent cleanup reimbursement is a distinct
principal-funded protected debit and remains in
`PrincipalFundedGrossDebitV1` unless an exact closed Core-major lifecycle
exemption applies. The reimbursement itself is not protocol revenue and MUST
be displayed in its own bucket. Cancellation cannot require an engine callback
when it is the user's safety path.

## Governance boundaries

Every Production Core major is adminless under ADR 0004. For V1, this means no
upgrade authority, mutable protocol policy, protocol fee authority, privileged
pause, quarantine, sweep, or path that can change the five-basis-point rate,
basis, rounding, exemptions, same-asset rule, collector identity, user rights,
or accepted asset profiles.

Governance may discuss standards, publish a separate side-by-side Core major,
and operate offchain registry, UI, or routing policy. It has no authority over
any Production Core major. A new rate, collector, basis, rounding rule,
exemption, or asset authority requires a new program ID and explicit opt-in
migration.

Builder beneficiaries remain market-revision facts. Integrator identities
remain user-authorization facts. Engine, token, hook, and driver authorities
remain visible third-party trust and liveness facts; none is a Core V1 admin.

The only production parameters this document leaves for later evidence are
exact supported asset profiles, binary codecs, bounded resource maxima,
collection partitioning, and measured shard count. They must be frozen into the
deployed Core or its immutable descriptors before real assets are accepted.

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
- `five_bps_constants_immutable`: no caller, engine, router, policy, governance,
  upgrade, or pause path can alter `5 / 10_000`, floor rounding, same-asset
  funding, zero flat component, zero minimum, or collector identity.
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
- `protocol_leg_excluded_without_recursion`: the protocol assessment debit is
  excluded from its own basis and cannot recursively assess itself.
- `secondary_bucket_relabeling_no_bypass`: builder, integrator, referral,
  interface, LP, maker, cleanup, rebate, sponsor, claim, and equivalent labels
  cannot remove a principal-funded protected debit from the inclusive basis.
- `external_tax_is_inclusive_not_double_counted`: external tax inside an
  applicable gross source debit is reported as a subcomponent, neither
  subtracted nor added twice; protocol-leg transfer behavior remains excluded.
- `multi_asset_never_netted`: basis and liability in one asset cannot be offset
  by an effect or donation in another asset.
- `multi_asset_independent_records`: unlike assets are never summed, netted,
  normalized, or converted, and a different basis and fee asset fails.
- `source_account_split_same_principal`: moving one principal's basis across
  several source accounts does not create additional rounding groups.
- `five_bps_floor_vectors`: basis `0`, `1`, `1_999`, `2_000`, `2_001`, maximum
  minus one, and maximum equal `basis div 2_000`.
- `no_hidden_minimum`: every basis below `2_000` produces zero, not one.
- `checked_arithmetic_matrix`: multiply, sum, subtraction, conversion, inverse
  fee, and entitlement overflow all fail closed.
- `cross_scope_dust_disclosure`: tests demonstrate that independent
  authorization scopes are independent fee groups, quantify the strictly
  sub-unit loss per group and its deliberate-fragmentation amplification, and
  show that fill envelopes of one stored intent retain one cumulative basis.
- `multi_hop_principal_boundary_only`: internal route inventory is not assessed
  again while every fresh principal-funded debit is assessed.
- `linked_refund_only`: only an exact causal same-envelope restoration with one
  unique origin allocation and Core-proven segregated-escrow release or exact
  pre-use reversal reduces basis; aggregate refunds for an originating debit
  cannot exceed it, and mixed inventory, outputs, rebates, later refunds, and
  reverse trades do not reduce basis.
- `gross_assessment_amount_fixed`: the five-basis-point formula output equals
  the protocol leg's gross source debit for every accepted asset profile;
  withholding can reduce funded vault credit but cannot trigger inverse
  gross-up or change assessed amount.
- `exact_input_actual_consumed`: maximum, prefunding, and returned unused value
  are not basis.
- `exact_output_actual_input`: output amount and maximum input are not basis;
  actual committed input is basis.
- `reversible_custody_profile_narrow`: exemption succeeds only for exact
  same-principal, same-asset, one-for-one, engine-independent relocation.

### Partial-fill and replay tests

- `cumulative_fill_equivalence`: all partitions and orders of the same accepted
  cumulative basis produce the same final cumulative rate assessment; declared
  envelope-scoped components are accounted and disclosed separately.
- `replacement_cannot_reset_remainder`: replacement, resume, retry, and replay
  cannot reset cumulative basis, fee, or rounding remainder.
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
- `protocol_collector_immutable`: no in-place transition can change the
  collector; a different collector requires a new Core deployment.
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
- `protocol_transfer_fee_rounding_grid`: zero, smallest, capped,
  full-withhold, and maximum-integer cases preserve `G_protocol = F`; a
  positive `F` with no spendable credit fails and no case inverse-grosses the
  protocol assessment.
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
- `state_only_has_zero_protocol_assessment`: a state-only or zero-effect commit
  cannot manufacture a percentage basis or a fixed V1 protocol fee.

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
- `cancellation_fee_boundary`: pre-settlement cancellation with no new
  principal-funded protected debit carries no protocol assessment; any accepted
  cleanup reimbursement is exact, cannot be redirected, and is assessed unless
  an exact closed Core-major lifecycle exemption applies.
- `production_core_has_no_admin`: the production artifact exposes no upgrade,
  protocol configuration, fee, pause, quarantine, sweep, or migration authority.
- `new_major_requires_new_identity`: changing protocol rate, basis, collector,
  rounding, exemption, or asset authority requires a different program ID.
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
7. the production artifact and deployment manifest prove there is no Core
   upgrade, protocol configuration, fee, pause, quarantine, sweep, proxy, or
   migration authority; and
8. an external security review covers assessment algebra, token behavior,
   claims, sharding, authority separation, and atomic rollback.

Until those gates pass, the current fee code remains experimental measurement
logic rather than accepted protocol economics.
