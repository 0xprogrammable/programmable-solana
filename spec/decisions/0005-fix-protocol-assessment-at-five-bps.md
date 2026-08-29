# 0005: Fix Protocol Assessment V1 at five basis points

- Status: Accepted
- Date: 2026-08-29

## Context

An arbitrary engine label cannot prove “trade volume.” A production fee must be
derived from objective Core facts, remain inside exact user limits, and avoid a
mutable revenue switch.

## Decision

`ProtocolAssessmentV1` is immutable:

```text
rate                  = 5 / 10_000 = 1 / 2_000
rounding              = floor after canonical cumulative aggregation
flat component        = 0
minimum               = 0
maximum               = none
fee asset             = basis asset
funding               = additive
assessment amount     = gross source debit
```

The basis is `PrincipalFundedGrossDebitV1`: the canonical aggregate of exact
Core-verified protected gross debits that consume one assessment principal's
spend capability in one authorization scope and asset. Valid causally linked
same-envelope refunds reduce the basis only when each refund has one unique
origin allocation, Core proves a segregated unused-debit release or exact
pre-use reversal without trusting an engine label, and aggregate refunds cannot
exceed the originating debit. Credits from mixed or unrelated inventory do not
reduce the basis.
The protocol assessment itself, network fees, output and unsolicited-donation
credits that consume no principal spend, exact same-asset one-for-one transfers
between Core-authenticated accounts or claims of the same principal with no
beneficial-owner change or new protected right, internal inventory movements
without principal spend, and exact closed Core-major lifecycle exemptions are
excluded.

Builder, integrator, referral, interface, LP, maker, cleanup, rebate, sponsor,
claim, external-tax, and similar labels do not exempt a principal-funded
protected debit. External tax inside an applicable gross source debit is an
inclusive reported subcomponent, not an amount subtracted from or added again
to the basis. In `CORE_ENFORCED` mode, an unclassifiable principal-funded
protected debit is rejected before movement rather than silently assigned zero
basis.

For cumulative basis `B`:

```text
A(B) = B div 2_000
fee_delta = A(B_after) - A(B_before)
```

The formula output is the exact gross source debit of the protocol-assessment
leg. Asset-program withholding may reduce funded vault credit and liability,
but Core does not inverse-gross-up the V1 protocol debit to target a net credit.
A net-defined assessment amount requires a new Core major.

Multi-hop internal legs are not reassessed. Unlike assets and different
principals form separate groups. Failed envelopes, pre-settlement cancellation,
the exact engine-independent exit, a proven one-for-one reversible
same-principal custody relocation, and an exact closed Core-major fee or
entitlement claim that only credits the entitled principal and consumes no new
principal-funded spend capability have zero protocol assessment.

Integer indivisibility leaves less than one base unit uncollected per new
independent assessment group. Deliberately creating new authorization scopes
can multiply that undercollection; partial fills of one intent cannot, because
they retain a cumulative basis. V1 accepts this disclosed revenue boundary
instead of a ceiling, minimum fee, global mutable remainder, or oracle-priced
alternate fee asset. Changing that choice requires a new Core major.

The immutable deployment descriptor binds one `protocol_collector_id`. A
different rate, basis, rounding rule, exemption, fee asset rule, or collector
requires a new Core major and Program ID.

Existing fee vaults and accrued liabilities remain permanently bound to their
original collector. An opt-in user migration may create new state under a new
Core but cannot rewrite, redirect, or reassign an old liability.

## Honest reporting consequence

Five basis points applies independently to every applicable
`PrincipalFundedGrossDebitV1` group, not all engine activity, all DEX volume,
oracle-valued notional, or external entrypoints. Assessed, funded, claimable,
claimed, and offchain-valued revenue remain separate facts.
`CoreAssessedGrossDebitByAsset` is only the reporting aggregate of committed
group bases with the same asset profile and asset after their independent
assessments, not an alternate basis definition or a cross-group fee formula.
Unlike assets remain separate until explicitly priced offchain as
`OffchainValuedCoreAssessedGrossDebit`.

## Prior-decision consequence

This decision resolves the fee amount, basis, rounding, and update-rule items
left open by ADR 0002. Disposable experiments and their test-only 30-basis-point
or variable policies remain unchanged as historical evidence.
