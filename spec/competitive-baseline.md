# Competitive baseline

Status: Observed on 2026-08-28

This note tests one narrow product claim against current official public source,
IDLs, SDKs, documentation, and licenses. It is not a claim about every Solana
project or private roadmap.

## Defensible gap

In the reviewed Raydium, Meteora, Orca Whirlpools, and PumpSwap interfaces, no
permissionless native per-market engine lets an arbitrary third-party program
define the venue's pricing function, state machine, fee logic, and settlement
rules inside one common DEX kernel.

That is narrower than saying "Solana has no programmable liquidity." Existing
protocols expose real programmability through configured curves, bins, dynamic
fees, limit orders, launch parameters, and Token-2022 transfer hooks. Those
surfaces do not make the pool's entire market engine replaceable.

## Current surfaces

| Protocol | Public customization surface | General per-market engine? | Public source status |
| --- | --- | --- | --- |
| Raydium | CPMM, CLMM, configurable fees, a bounded dynamic-fee model, and LaunchLab curve families | No native arbitrary engine found | CPMM, CLMM, and AMM v4 program source are Apache-2.0; LaunchLab's official public material inspected here is SDK and IDL rather than complete program source |
| Meteora | DLMM bins and strategies, DAMM v2 parameters and dynamic fees, DBC piecewise liquidity and migration rules, Token-2022 hooks | No general engine; transfer hooks are mint-wide token callbacks, not free pool pricing | DAMM v2 and DBC publish source under non-commercial licenses; the reviewed DLMM repository exposes SDK, IDL, and artifacts rather than complete program source |
| Orca Whirlpools | Concentrated liquidity, fee tiers, adaptive fees, delegated fee authority, Token-2022 hook accounts | No general engine found | Full program source is public under a restrictive Orca license that requires permission for specified commercial and competing-DEX use |
| PumpSwap | Constant-product pool instructions plus protocol, creator, cashback, and reserve configuration | No general engine found | Official repository exposes documentation and IDL, not full Rust program source or a license in the reviewed snapshot |

Token-2022 transfer hooks are important but not equivalent to a market engine.
They execute mint-selected logic around a token transfer. They do not let one
market replace the DEX's curve, accounting, or settlement state machine.

A wrapper program that calls an existing DEX through CPI is also different. If
users can trade the underlying pool directly, the wrapper's rules are a separate
and bypassable layer rather than the pool's canonical engine.

## Product implication

Programmable creates a distinct protocol category only if it preserves all of
the following:

- permissionless engine selection per market;
- open market semantics rather than a catalog of approved curve types;
- one common Core for intent, supported settlement, protocol fees, evidence, and
  non-participating-domain isolation;
- no Programmable allowlist or server on the execution path; and
- a future general external settlement boundary for asset models that the Core
  cannot safely interpret itself.

If the product stops at several curves, bonding-curve templates, dynamic fees,
and transfer-hook compatibility, it becomes a broader configuration product,
not the claimed general-purpose engine protocol.

The gap is not itself a moat. Open-source code can be copied. Durable advantage
would come from safe interface standards, engine and client tooling, reference
implementations, integrations, liquidity, routing, indexing, and developer
adoption.

## Fee comparison lesson

Existing DEX protocol revenue is generally a protocol share of an already
charged trading fee, while the remaining economics belong to LPs, creators,
hosts, or other participants. It is wrong to treat gross trading fees, volume,
TVL, or LP value as protocol revenue.

Programmable's stronger honest statement is:

- one mandatory fee for every successfully committed Core envelope with an
  authenticated Core-native funding leg; and
- optional volume-based protocol fees only on exact effects the Core can observe
  and assess once.

Opaque programs can batch internal actions or expose direct entrypoints. No open
protocol can guarantee a percentage of semantics it cannot observe or charge an
execution that does not route through it.

## Primary snapshots

- Raydium CPMM [`244e1241f3c8d90eb93f176dfbc35f2605ec5a5c`](https://github.com/raydium-io/raydium-cp-swap/tree/244e1241f3c8d90eb93f176dfbc35f2605ec5a5c)
- Raydium CLMM [`ed7c84a54ced59c55981780546adb0b4583dcf85`](https://github.com/raydium-io/raydium-clmm/tree/ed7c84a54ced59c55981780546adb0b4583dcf85)
- Raydium LaunchLab curve switch [`35fe2bad4d17864157c2c918d7602efe62dcde40`](https://github.com/raydium-io/raydium-sdk-V2/blob/35fe2bad4d17864157c2c918d7602efe62dcde40/src/raydium/launchpad/curve/curve.ts#L517-L527)
- [Raydium protocol fees](https://docs.raydium.io/raydium/protocol/protocol-fees)
- Meteora DAMM v2 [`2565067bb5b0795c7f7e6200479eeb85b7422b40`](https://github.com/MeteoraAg/damm-v2/tree/2565067bb5b0795c7f7e6200479eeb85b7422b40)
- Meteora DBC [`3b540e94b5b20ba37733de6e25f58522a0cd8961`](https://github.com/MeteoraAg/dynamic-bonding-curve/tree/3b540e94b5b20ba37733de6e25f58522a0cd8961)
- Meteora DLMM SDK and IDL [`fb02e51ae677bbd18e76543f702dae40632426db`](https://github.com/MeteoraAg/dlmm-sdk/tree/fb02e51ae677bbd18e76543f702dae40632426db)
- Orca Whirlpools [`3b47341e16110ba015ca0acf06a53c0fa12e49f3`](https://github.com/orca-so/whirlpools/tree/3b47341e16110ba015ca0acf06a53c0fa12e49f3)
- Pump public docs and IDL [`9c82f61cb711b044a17f770ab8ce9f9bdf78f333`](https://github.com/pump-fun/pump-public-docs/tree/9c82f61cb711b044a17f770ab8ce9f9bdf78f333)

This baseline must be refreshed before a public competitive claim. A missing
interface in these snapshots is evidence about the reviewed versions, not proof
that no smaller project or later release has a similar design.
