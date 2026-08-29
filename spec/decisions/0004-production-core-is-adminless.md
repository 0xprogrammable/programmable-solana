# 0004: Make every production Core adminless and immutable

- Status: Accepted
- Date: 2026-08-29

## Context

An upgrade, configuration, fee, pause, quarantine, sweep, or proxy authority
can replace or bypass code-level guarantees. Calling such a deployment
immutable or owner-compromise-safe would be false.

## Decision

Every Core major that can receive production assets is immutable and adminless
at deployment. There is no funded mutable production alpha. Future production
majors inherit this release invariant and use new deployment identities.

Every Production Core major has no:

- upgrade authority or replaceable program data;
- proxy, beacon, implementation pointer, or equivalent code indirection;
- protocol configuration or fee setter;
- privileged pause or quarantine;
- admin sweep or asset-redirection path; or
- automatic migration authority.

Testnet and disposable release candidates may be replaced before production.
They accept no real user assets and are not described as immutable or
owner-compromise-safe.

A later Core major receives a new Program ID and runs side-by-side with the old
major. Users and domains migrate only through an exact, opt-in path bound by the
original custody and exit profile. A later program cannot sign for old PDAs or
retroactively add an escape path.

Governance may publish standards, new deployments, and offchain registry or
routing policy. It has no authority over an existing Production Core.

## Evidence consequence

A Production manifest must prove the source and ELF identity, loader state,
deployment transaction and slot, and `upgrade_authority = None`. Any remaining
Core authority invalidates the Production classification for that major.
