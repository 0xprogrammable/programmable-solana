# Routed callback authentication experiment

> [!CAUTION]
> **DISPOSABLE LOCAL EXPERIMENT. PRIVATE WIRE FORMAT. NO PUBLIC ABI. NO DEPLOYMENT.**

This isolated workspace tests whether the same exact protected settlement can
be authorized directly or through an arbitrary caller program without trusting
that caller, and whether a narrowly derived Core callback signer can authenticate
the selected engine without becoming a general Core authority.

It is not a DEX release, a supported engine interface, an SDK, or protocol code
approved or supported for deployment. Its program IDs and all wire bytes are
disposable. Do not deploy these programs to devnet or mainnet and do not place
funded custody under them.

## Isolation boundary

This directory is a standalone Cargo and Anchor workspace with its own lockfile,
toolchain declaration, build output, and disposable program IDs. It is not a
member of the repository-root Cargo workspace and must not introduce path
dependencies to or from that workspace.

The authority-kernel and engine-generated-settlement experiments remain frozen.
This experiment may copy the smallest necessary code at the provenance baseline,
but it must not edit, include, or import that source. See
[`PROVENANCE.md`](PROVENANCE.md).

## Intended components

- `routed-callback-probe-wire`: private request, result, and digest codec;
- `routed-callback-core`: disposable protected settlement and authorization program;
- `routed-plan-engine`: configurable callback-authenticated engine;
- `hostile-router-probe`: untrusted caller that forwards or mutates Core requests;
- `callback-capability-probe`: downstream fixture for signer-forwarding tests.

The names above describe test roles only. They do not reserve production package
names or compatibility guarantees.

## Result

The local executable evidence selects the single writable `TRANSITION` before
settlement as the minimal shape for the next private design gate. The fully
read-only `PREPARE` plus writable `COMMIT` path also passes, but adds a second
engine call and more compute in the controlled maximum fixture. Neither wire is
a public interface. See
[`../../spec/routed-callback-auth-spike-results.md`](../../spec/routed-callback-auth-spike-results.md)
for the exact measurements and remaining limits.

Despite the `routed-plan-engine` test name, the settlement control remains a
fixed exact-input classic-SPL A-to-B envelope. The experiment compares callback
and caller-authentication shapes; it does not prove a generic plan ABI,
arbitrary settlement topology, multi-leg effects, orders, auctions, NFTs, or
asynchronous execution.

The routed path deliberately does not receive the user signer. A separate
top-level exact-intent authorization installs an exactly sized, request-specific
classic-SPL delegate. That delegate is the spend capability and one-shot state;
successful execution must consume it to zero. The callback PDA is a different,
signer-only capability scoped to one engine, market, intent digest, and phase. A
hostile engine may forward that callback signer, but no Core custody, fee,
administration, or upgrade path may accept it. Unsolicited lamports at either
PDA address carry no protocol meaning.

Core execution authenticates the exact owner-approved delegate state, not the
historical instruction that created it. The top-level Core authorization is the
canonical full-intent validation path and rejects CPI, but an owner can create
the same exact delegate directly through classic SPL Token. Distinguishing
those identical states would require instruction introspection or persistent
Core intent state, which this experiment intentionally excludes.

## Checks

Run every host-side check without building SBF artifacts:

```sh
SKIP_SBF=1 ./scripts/check.sh
```

Run the complete local gate, including the four SBF programs:

```sh
./scripts/check.sh
```

Both commands require the pinned tools declared in this workspace. Generated
artifacts stay under `target/deploy`; deployment keypairs must never be retained.
