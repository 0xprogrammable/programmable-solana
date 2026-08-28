# Engine-generated settlement experiment

> [!CAUTION]
> **DISPOSABLE LOCAL EXPERIMENT. PRIVATE WIRE FORMAT. NO PUBLIC ABI. NO DEPLOYMENT.**

This isolated workspace tests whether an engine can generate exact settlement
effects and use an ordered opaque capability closure while the Core retains sole
authority over protected token movement, user limits, and the protocol fee.

It is not a DEX release, a supported engine interface, an SDK, or protocol code
approved or supported for deployment. Its program IDs and all wire bytes are
disposable. Do not deploy these programs to devnet or mainnet and do not place
funded custody under them.

## Isolation boundary

This directory is a standalone Cargo and Anchor workspace with its own lockfile,
toolchain declaration, build output, and disposable program IDs. It is not a
member of the repository-root Cargo workspace and must not introduce path
dependencies to or from that workspace.

The implemented authority-kernel experiment remains frozen under the repository
root. This experiment may copy the smallest necessary code at the provenance
baseline, but it must not edit, include, or import that source. See
[`PROVENANCE.md`](PROVENANCE.md).

## Intended components

- `generated-settlement-probe-wire`: private request, result, and digest codec;
- `generated-settlement-core`: disposable protected settlement program;
- `generated-plan-engine`: configurable generated-plan and hostile engine;
- `opaque-capability-probe`: external-program fixture for nested CPI tests.

The names above describe test roles only. They do not reserve production package
names or compatibility guarantees.

Despite the `generated-plan-engine` test name, this engine returns only one
generated output scalar for a fixed exact-input A-to-B envelope. The experiment
does not prove a generic plan ABI, arbitrary settlement topology, multi-leg
effects, orders, auctions, NFTs, or asynchronous execution.

## Checks

Run every host-side check without building SBF artifacts:

```sh
SKIP_SBF=1 ./scripts/check.sh
```

Run the complete local gate, including the three SBF programs:

```sh
./scripts/check.sh
```

Both commands require the pinned tools declared in this workspace. Generated
artifacts stay under `target/deploy`; deployment keypairs must never be retained.
