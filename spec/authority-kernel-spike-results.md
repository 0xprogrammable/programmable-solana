# Authority-kernel spike results

Status: Implemented experiment, not an accepted protocol interface

Date: 2026-08-28

## Decision supported by this experiment

A Core-mediated capability boundary is executable on the current Solana model:
an arbitrary stateful engine can approve economic behavior without receiving a
user signer, a Core signer, a token account, a vault, a fee account, or a token
program capability from Core.

This result supports continuing the architecture experiment. It does not accept
the Probe V0 wire format, authorize persistent custody, or make any program in
this repository deployable.

## Implemented path

The disposable path is deliberately narrow:

```text
initialize one exclusive two-mint market/domain
  -> deposit classic SPL Token B
  -> execute exact A-to-B candidate plan
       -> one stateful engine CPI
       -> user A to domain vault A
       -> user A to protocol fee vault
       -> domain vault B to exact recipient
```

The Core derives a 30-basis-point, ceiling-rounded measurement fee from its
immutable experimental policy. The number is test data, not accepted product
economics.

The engine CPI contains exactly:

1. the selected engine state, writable and non-signer; and
2. the Instructions sysvar, read-only and non-signer.

The engine program is the CPI target, not a callee account. No remaining
accounts are accepted. The Core constructs the inner instruction manually; it
does not use an Anchor helper that could inherit outer account privileges.

## Executed evidence

The LiteSVM 0.16 suite uses Agave 4.2.1 semantics and real classic SPL Token
instructions. It creates two mints and token accounts, initializes both custom
SBF programs, deposits liquidity, and executes the Core path.

The successful execution measured:

| Property | Observed | Current launch baseline |
| --- | ---: | ---: |
| Serialized legacy transaction | 688 bytes | 1,232 bytes maximum |
| Transaction accounts | 16 | 64 locked accounts maximum |
| Writable transaction accounts | 9 | no global writable account |
| Compute consumed | about 52.9k CU (52,904–52,935 observed) | 1,400,000 CU transaction maximum |
| Engine accounts | 2 | fixed closure |
| Maximum call-tree depth | 2 levels | Core plus direct engine/token CPIs |

Eleven integration tests, including three table-driven receipt failures and six
table-driven cross-market substitutions, establish these statements for the
exact Probe V0 path:

- the engine inner instruction contains only engine state and the Instructions
  sysvar;
- the returned receipt is read immediately and authenticated by setter program
  and exact plan hash;
- missing, malformed, and wrong-plan receipts fail closed and roll back the
  engine mutation byte-for-byte;
- a correctly encoded direct top-level engine invocation is rejected before
  engine state changes;
- substituting a second market's domain, fee ledger, input or output vault, fee
  vault, or engine state fails before any engine or token CPI and preserves both
  markets byte-for-byte;
- selecting another market's same-mint vault as the output recipient produces
  only a raw token donation; its accounted liquidity, Core state, fee state, and
  engine state remain unchanged;
- the plan hash binds Core, market, domain, engine, user, mints, token accounts,
  vaults, fee ledger, token program, exact amounts, user bounds, policy
  revisions, accounted balances, and expiry;
- expiry, minimum-output, total-debit, and protocol-fee bound failures stop
  before the engine CPI and preserve all economic state;
- raw vault donations do not become accounted liquidity or fee liability;
- output cannot consume donated balance above accounted liquidity;
- a hostile engine can write its own state but cannot promote the read-only
  Instructions sysvar to writable; and
- the hostile engine explicitly flushes its state before the rejected privilege
  escalation, and that engine write still rolls back; and
- after an engine accepts and the first SPL transfer succeeds, a deliberately
  failed fee transfer rolls back the engine write and the earlier token
  movement byte-for-byte.

The deterministic codec suite mutates every authority-bearing plan field and
requires every mutation to change the plan hash. Decoders reject wrong lengths,
versions, discriminators, magic bytes, and trailing data.

## Reproduction

The repository pins the host, Anchor, Agave, LiteSVM, and granular Solana
dependency graph. Run:

```sh
./scripts/check-repository.sh
cargo fmt --all --check
./scripts/build-spike-sbf.sh
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
```

The platform-tools v1.52 compiler selected by cargo-build-sbf 3.1.10 uses its
own Rust 1.89 toolchain; host tests use Rust 1.96 because the Agave 4.2 test
graph requires newer host APIs. CI downloads checksum-pinned Agave 3.1.10 and
platform-tools v1.52 artifacts, builds both programs directly with
`cargo build-sbf`, rejects a retained deployment keypair, and verifies the
generated ELF files against
[`authority-kernel-sbf-v0.sha256`](authority-kernel-sbf-v0.sha256).

Agave 3.1.10 ships an empty legacy `syscalls.txt`, so its post-processing step
prints an alarming unknown-syscall warning for normal Solana syscalls. The
generated SBPFv0 artifacts execute successfully in the pinned Agave 4.2.1
LiteSVM runtime; the warning is recorded as toolchain debt and is not deployment
evidence.

The canonical Ubuntu 24.04 CI build produced:

- Core: `0826d2cf55b69908635cf5ed52c6a9f363413ce4dcb9858c3f0ee6bd7611c425`
- hostile engine: `c29d44ee25b6451877eb4bf86de1ffcd53c10cca15bf6524101ef7c5a6442d38`

Two consecutive macOS same-host builds were byte-identical but differed from
Ubuntu:

- Core: `12ef0b9c3da7a6e2a80aa199a9c579425414b9dab63a37628ddae5368e04d6d3`
- hostile engine: `39b627b258ba79fd7f1a7c482e2b5ce900651ba8027bfd07ae0f55d2e0965043`

The manifest deliberately pins the Ubuntu build, which is the canonical
experiment environment. The cross-OS difference means the current process is
environment-pinned, not platform-independent reproducibility. None of these
hashes is a signed release artifact or evidence of an onchain deployment. A
production release requires the canonical artifact to be reproduced by at least
two independent builders using the same hermetic environment.

## Not established

This experiment intentionally leaves material blockers open:

- there is no provider exit or protocol-fee claim instruction, so its custody
  accounts must never be deployed or funded outside an isolated test runtime;
- a numeric engine revision is not loader-backed code identity, and an
  upgradeable engine remains mutable-engine trust;
- the exclusive domain does not prove shared-domain admission;
- only one direct-user, A-to-B, classic SPL Token path exists;
- the rollback fixture intentionally lets an objectively underfunded source
  reach the engine before SPL settlement; any promoted execution route must
  reject that condition before invoking untrusted code;
- Token-2022 extensions, external settlement drivers, stored or multi-party
  intents, positions, partial fills, and asynchronous behavior are untested;
- the callback comparison and engine-generated-plan variants in the experiment
  plan remain unimplemented;
- packet, compute, and call-tree results are one measured fixture, not universal
  bounds for arbitrary engines;
- LiteSVM execution is not devnet, mainnet, formal verification, or an external
  audit; and
- SBPFv3 compatibility is not established; adopting it requires a separately
  pinned toolchain, an `ELF Flags: 0x3` assertion, and the exact-artifact
  runtime suite before any deployment or finalization on a cluster that can
  activate the SBPFv3-only deployment rule; and
- no public ABI, client compatibility promise, deployment key, upgrade policy,
  or release artifact exists.

These are gates, not fields to add speculatively to Probe V0. Each authority or
liveness expansion gets its own reduced experiment before a public interface is
accepted.
