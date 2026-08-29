# Competitive baseline

Status: source and documentation review observed on 2026-08-28. Mainnet RPC
facts were observed at `2026-08-28T18:34:56Z`, slot `442403492`; the Patcha
devnet fact was observed at `2026-08-28T18:35:21Z`, slot `489503904`.

This is a bounded comparison of named public systems, not a claim about every
Solana program or a private roadmap. A missing interface means only that it was
not demonstrated by the cited evidence.

## Evidence rule and defensible gap

The defensible claim is:

> Among the verified systems below, no general pool-enforced engine core was
> demonstrated.

`General` means a permissionless per-market program can define pricing, state
transitions, fees, and settlement rather than selecting from protocol-owned
parameters. `Pool-enforced` means every canonical market transition must pass
through that program; a router, keeper, API, or optional wrapper is not enough
when users can call the underlying venue directly.

Evidence is kept separate:

- **Source**: pinned program source or IDL establishes what the reviewed code
  can do. It does not prove that identical bytes are deployed.
- **Documentation**: an official interface or design claim, not proof of its
  implementation.
- **Onchain**: account existence, executability, loader state, and upgrade
  authority at the exact snapshot above. It does not prove source equivalence
  or safe behavior.

A **direct competitor signal** means the published product concept targets the
same programmable-liquidity category. A **verified implementation** additionally
needs a bindable program identity and enough public source, IDL/ABI, and onchain
evidence to inspect the claimed enforcement boundary. This distinction is why
Soliquid and h00k are signals, not proof that the gap is already closed.

## Venue evidence baseline

| System | Verified topology and extensibility | Creation, fees, authority, interfaces | Implication for Programmable |
| --- | --- | --- | --- |
| **Raydium** | Pinned [CPMM](https://github.com/raydium-io/raydium-cp-swap/tree/244e1241f3c8d90eb93f176dfbc35f2605ec5a5c) and [CLMM](https://github.com/raydium-io/raydium-clmm/tree/ed7c84a54ced59c55981780546adb0b4583dcf85) source show fixed AMM kernels, pool/config PDAs, and program-controlled vaults, not arbitrary market callbacks. Token-2022 support is [extension-filtered](https://docs.raydium.io/reference/token-2022-support). | Pool creation is permissionless within supported configurations. Fee tiers and dynamic-fee rules are protocol-defined; CPMM/CLMM trading fees are documented as [84% LP, 12% RAY buyback, 4% treasury](https://docs.raydium.io/ray/ray-buybacks). Public SDK/API/CPI surfaces exist; both reviewed programs are upgradeable in the onchain snapshot. | Match integration quality and explicit token-extension policy. Do not call a larger catalog of curves a general engine. |
| **Meteora** | Pinned [DAMM v2 source](https://github.com/MeteoraAg/damm-v2/tree/2565067bb5b0795c7f7e6200479eeb85b7422b40), [DBC source](https://github.com/MeteoraAg/dynamic-bonding-curve/tree/3b540e94b5b20ba37733de6e25f58522a0cd8961), and [DLMM SDK/IDL](https://github.com/MeteoraAg/dlmm-sdk/tree/fb02e51ae677bbd18e76543f702dae40632426db), plus official [DAMM v2 instructions](https://github.com/MeteoraAg/docs/blob/main/developer-guides/damm-v2/program/instructions.mdx), [DLMM docs](https://github.com/MeteoraAg/docs/blob/main/developer-guides/dlmm/index.mdx), and [DBC account model](https://github.com/MeteoraAg/docs/blob/main/developer-guides/dbc/program/accounts.mdx), expose bins, curve/config parameters, fee schedulers, token badges, migration, and pause controls. These are rich protocol-owned factories, not third-party engine execution. | Per-pool vault/config custody and multiple operational authorities are explicit. SDK/IDL/CPI surfaces exist. DAMM v2 source uses a [non-commercial license](https://github.com/MeteoraAg/damm-v2/blob/2565067bb5b0795c7f7e6200479eeb85b7422b40/license.md); the reviewed DLMM public material did not include complete program source. DAMM v2 and DLMM were upgradeable onchain. | Treat configurable launch, curve, and fee schedules as the minimum feature baseline, while keeping emergency controls withdrawal-safe and publicly specified. |
| **Orca Whirlpools** | Pinned [program source](https://github.com/orca-so/whirlpools/tree/3b47341e16110ba015ca0acf06a53c0fa12e49f3) shows a fixed concentrated-liquidity program with pool token vaults, `WhirlpoolsConfig`, fee authorities, adaptive-fee state, and TokenBadge controls. [Token-extension support](https://docs.orca.so/developers/architecture/token-extensions) resolves required accounts but does not install arbitrary pool callbacks. | Pools can be [created through the SDK](https://docs.orca.so/developers/sdks/pools/create-pool) under an existing config/fee tier. [Fee tiers and protocol fees](https://docs.orca.so/developers/architecture/whirlpool-fees) are configuration surfaces. The program is upgradeable; source is under Orca's custom license. | Separate immutable market state from narrowly scoped configuration authorities. Token-hook account plumbing is compatibility work, not the engine abstraction. |
| **PumpSwap** | The official pinned [IDL](https://github.com/pump-fun/pump-public-docs/blob/9c82f61cb711b044a17f770ab8ce9f9bdf78f333/idl/pump_amm.json) and [program documentation](https://github.com/pump-fun/pump-public-docs/blob/9c82f61cb711b044a17f770ab8ce9f9bdf78f333/docs/PUMP_SWAP_README.md) describe a fixed constant-product AMM. Each pool PDA owns its base/quote ATAs; global state controls admin, pause, and fee recipients. No official Rust program source was present in the reviewed repository. | `create_pool` is permissionless. Protocol, creator, and other fee paths are globally defined in the [current fee docs](https://pump.fun/docs/fees), and the official history includes a [breaking fee-recipient migration](https://github.com/pump-fun/pump-public-docs/blob/9c82f61cb711b044a17f770ab8ce9f9bdf78f333/docs/BREAKING_FEE_RECIPIENT.md). The program is upgradeable. | Publish versioned ABI and migration rules. A vertically integrated launch/AMM path is not an open engine system. |
| **OpenBook v2** | Pinned [`create_market`](https://github.com/openbook-dex/openbook-v2/blob/f3e17421e675b083b584867594bf3cf4f675d156/programs/openbook-v2/src/instructions/create_market.rs) and [`Market`](https://github.com/openbook-dex/openbook-v2/blob/f3e17421e675b083b584867594bf3cf4f675d156/programs/openbook-v2/src/state/market.rs) source show a permissionless CLOB with per-market base/quote vaults, event heap, and optional market/open-orders/prune/consume authorities. It is not a pool callback system. | The creator chooses maker/taker fees and optional authorities. Source, Anchor IDL, clients, and CPI are public. The deployed program is upgradeable; its upgrade-authority account was owned by the SPL Governance program at the snapshot, but that alone does not establish the governance policy. | Fine-grained market authorities and explicit event consumption are useful patterns; they must not become hidden engine approval gates. |
| **Phoenix** | Pinned [market initialization](https://github.com/Ellipsis-Labs/phoenix-v1/blob/5a34f7f901fd9e04057198d4fc7b7286f78b53f2/src/program/processor/initialize.rs) and [governance](https://github.com/Ellipsis-Labs/phoenix-v1/blob/5a34f7f901fd9e04057198d4fc7b7286f78b53f2/src/program/processor/governance.rs) source show a crankless atomic CLOB with per-market vault context, market authority, and successor. There is no arbitrary lifecycle callback. | Market initialization and creator-selected fees are native instructions; client/CPI interfaces are public. The program is upgradeable and its reviewed token path is classic SPL Token rather than a general Token-2022 capability layer. | Atomic settlement and self-contained event output are strong interface precedents. Market administration is not engine programmability. |
| **Manifest** | Pinned [core](https://github.com/jup-ag/manifest-amm/blob/093accab6f4f4f77765e3d7a86c037ac9db169a0/programs/manifest/src/lib.rs) and [wrapper](https://github.com/jup-ag/manifest-amm/blob/093accab6f4f4f77765e3d7a86c037ac9db169a0/programs/wrapper/src/lib.rs) source deliberately split a minimal orderbook core from composable wrappers. The pinned [design overview](https://github.com/jup-ag/manifest-amm/blob/093accab6f4f4f77765e3d7a86c037ac9db169a0/README.md) documents permissionless markets, zero core trading fees, Token-2022 support, global/reverse orders, and formal-verification/audit artifacts. | Core and reference wrapper are separate upgradeable programs. A wrapper can add order semantics, but direct core interaction bypasses wrapper-only policy unless the core itself requires it. | Adopt the small-core/versioned-wrapper discipline. Do not describe optional wrapper behavior as canonical pool enforcement. |
| **FluxBeam** | Official [Pool API](https://docs.fluxbeam.xyz/developers/pool-api), [Swap API](https://docs.fluxbeam.xyz/developers/swap-api), [pool creation](https://docs.fluxbeam.xyz/fluxtools-tutorials/pool-creation), and [fee manager](https://docs.fluxbeam.xyz/fluxtools-tutorials/fee-manager) docs establish a Token-2022-oriented AMM/API product. The official [web bundle](https://fluxbeam.xyz/js/api.0218f3ab.js) names program `FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X`; that program was executable and upgradeable onchain. | The documented flow returns LP tokens to the creator rather than protocol-locking them and exposes HTTP quote/swap/pool interfaces. No official program source, complete IDL, reproducible build, audit, or upgrade-governance description was found in the reviewed public material. | Treat it as deployed/API evidence, not source-verified arbitrary extension safety. Publish a per-extension capability matrix instead of saying only “Token-2022 support.” |

## Direct competitor signals and enforcement limits

| Signal | What the primary material says | What is actually verified | Baseline consequence |
| --- | --- | --- | --- |
| **Soliquid** | The official [overview](https://soliquid.gitbook.io/soliquid-docs/readme.md), [programmable-hooks](https://soliquid.gitbook.io/soliquid-docs/documentation/soliquid-hmm/programmable-hooks.md), [hook logic](https://soliquid.gitbook.io/soliquid-docs/documentation/hook-launchpad/hook-logic.md), and [singleton](https://soliquid.gitbook.io/soliquid-docs/documentation/soliquid-hmm/singleton-liquidity-pool.md) pages describe the closest direct design signal: a v4-like PoolManager, singleton/flash accounting, external hook programs, 14 callback/return-delta flags, and uploaded compiled hooks. | Documentation only. The reviewed corpus exposed no cluster/program ID, source repository, license, IDL/exact Solana ABI, vault/authority topology, upgrade model, verified build, or audit; its SDK was described as forthcoming. Therefore the material does not yet bind those claims to an inspectable deployment. | Track it as a direct competitor. Programmable must make every item missing here public and machine-verifiable; missing proof must not be converted into a broader product claim. |
| **Patcha** | Pinned [architecture](https://github.com/patcha-fi/patcha/blob/3d4123dec8b3e8ab535025964b1263b374b8a0f7/docs/architecture.md), [security notes](https://github.com/patcha-fi/patcha/blob/3d4123dec8b3e8ab535025964b1263b374b8a0f7/docs/security.md), and [`patcha-hook-executor`](https://github.com/patcha-fi/patcha/blob/3d4123dec8b3e8ab535025964b1263b374b8a0f7/programs/patcha-hook-executor/src/lib.rs) provide a concrete hook registry/executor for router/keeper integrations over existing venues. | Source inspection shows `trigger_hook` is invoked at the integration boundary: caller-supplied amount/tick data is checked and fee overrides are emitted, but the executor owns no underlying DEX pool and does not force Orca/Raydium/Meteora through it. Direct venue calls bypass it. Its claimed mainnet program was closed at the snapshot; the same ID was executable and upgradeable on devnet. The project labels itself pre-audit. | It validates demand for a common hook ABI, but also demonstrates why router-level hooks cannot support a “pool-enforced” claim. |
| **h00k.fun** | Official [docs](https://h00k.fun/docs.html) and [`hooks.js`](https://h00k.fun/hooks.js) describe ten fixed launch hooks with fee-routing behaviors. This is a launchpad/catalog signal, not an arbitrary developer engine interface. | No program source, IDL, audit, core program identity, or custody/authority proof was published in the reviewed material. At the mainnet snapshot, nine of the ten listed IDs did not exist; the remaining `7ijgKq7bJwMPXvGD7MLR6HwXBZTxm9CB6AiDNs3a3EZ1` was a zero-data, System Program-owned, non-executable account. | Do not count a named hook catalog as deployed code. Require executable-account and source/ABI evidence before using it in competitive claims. |

## Token-2022 is a substrate, not the engine

The official [Transfer Hook](https://solana.com/docs/tokens/extensions/transfer-hook)
and [integration](https://solana.com/docs/tokens/extensions/transfer-hook-integration)
documentation shows mint-scoped transfer middleware selected by the mint's hook
configuration and supplied through an `ExtraAccountMetaList`. It is enforced for
that token transfer, but it does not replace a venue's pricing curve, market
state machine, or settlement protocol and is not naturally per-pool. Transfer
fees are likewise [mint-level token behavior](https://solana.com/docs/tokens/extensions/transfer-fees).

Programmable therefore needs explicit compatibility and denial rules for each
extension, fresh extra-account resolution, simulation/compute budgeting, and a
clear policy for mutable hook/config authorities. It must not present
Token-2022 compatibility as equivalent to general market programmability.

## Onchain deployment and upgrade snapshot

The following was read from `solana program show -u mainnet-beta` at the exact
mainnet timestamp/slot at the top. A non-null authority proves upgradeability,
not who controls the key or what governance process applies.

| Program | Program ID | Upgrade authority at snapshot |
| --- | --- | --- |
| Raydium CPMM | `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C` | `FytDrVzDybM1TwFQPGb8qaxZR7dBCzNeqT3vtQsceZQK` |
| Raydium CLMM | `CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK` | `FytDrVzDybM1TwFQPGb8qaxZR7dBCzNeqT3vtQsceZQK` |
| Meteora DAMM v2 | `cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG` | `JADaUV8kvDpDbJr55wxXJHVaBS3VCj8thZZHjfeuCVLd` |
| Meteora DLMM | `LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo` | `JADaUV8kvDpDbJr55wxXJHVaBS3VCj8thZZHjfeuCVLd` |
| Orca Whirlpools | `whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc` | `GwH3Hiv5mACLX3ufTw1pFsrhSPon5tdw252DBs4Rx4PV` |
| PumpSwap | `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA` | `7gZufwwAo17y5kg8FMyJy2phgpvv9RSdzWtdXiWHjFr8` |
| OpenBook v2 | `opnb2LAfJYbRMAHHvqjCwQxanZn7ReEHp1k81EohpZb` | `CZoAmQErbMwhSNA5WtbWLcwGE1mhXEv4hTvyvvHXGkrr` |
| Phoenix | `PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY` | `8mv7G3fJq5a5ej7E14vgcSGeQKH79emjU9fVfuhyitEq` |
| Manifest core | `MNFSTqtC93rEfYHB6hF82sKdZpUDFWkViLByLd1k1Ms` | `CDFU8tEWsVU2ZMiek57Sgk3Huha2yBNcSHLAts3V3Cbf` |
| Manifest wrapper | `wMNFSTkir3HgyZTsB7uqu3i7FA73grFCptPXgrZjksL` | `B6dmr2UAn2wgjdm3T4N1Vjd8oPYRRTguByW7AEngkeL6` |
| FluxBeam | `FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X` | `AGmRsqwk1ShBGveWPd6Tdcmu7nQmm1EcWGXozBDLvSTq` |

Patcha program `EPcW7e8RxBNPpQK2XKoKG9maWH6QvmU3ejxifoU5rNRa`
returned `Program ... has been closed` on mainnet. At the separately recorded
devnet snapshot it was executable with upgrade authority
`6232d44FAZx3EAW1v5eafuLaAdKUQZz8JiP7YiFHt7cz`.

## Required competitive baseline for Programmable

Programmable is meaningfully differentiated only if the implementation proves
all of the following, rather than merely describing them:

- **Canonical enforcement:** every mutation of Core-owned state or protected
  Core custody enters one Core that authenticates the engine and enforces its
  invariants and every applicable protocol assessment. An external engine path
  is outside that claim and forces the affected execution profile to disclose
  `PARTIAL` or `NONE`; it cannot be reported as canonical Core volume.
- **Permissionless engines:** a versioned, public engine ABI and registration
  path without a Programmable allowlist or server on the execution path.
- **Safe capabilities:** declared accounts/effects, caller authentication,
  compute and CPI limits, reentrancy rules, balanced settlement, failure
  isolation, and deterministic return-delta semantics.
- **Explicit custody and authority:** vault ownership, pause/withdraw behavior,
  engine/config mutability, and the verified absence of every Production Core
  upgrade, config, fee, pause, sweep, or migration authority are inspectable.
  An opaque external program cannot receive general custody merely because it
  registered as an engine.
- **Honest fees:** Production V1 charges exactly five basis points with
  cumulative floor rounding only on the defined Core-observable,
  principal-funded gross-debit basis.
  Gross venue fee, volume, TVL, LP value, and creator revenue are not protocol
  revenue, and opaque engine-internal actions cannot support guaranteed
  percentage fees.
- **Public proof:** program IDs and clusters, pinned source, license, IDL/ABI,
  reproducible deployment evidence, security properties/audit status, and
  upgrade history ship together. Soliquid and h00k show why docs alone are not
  enough; FluxBeam and PumpSwap show why an executable program plus API/IDL is
  still not source equivalence.
- **Integration parity:** a forkable SDK, exact offchain/onchain quote parity,
  CPI examples, indexable events, and the official [Jupiter AMM integration
  requirements](https://developers.jup.ag/docs/swap/routing/amm/integration),
  including deterministic quotes without network calls.

The gap is not a moat by itself. Durable advantage would come from a safe
standard, reference engines, verifiable tooling, routing/indexing integration,
liquidity, and developer adoption. Refresh this file before any public
competitive claim.
