# 0003: Bind the SVM implementation to a portable protocol specification

- Status: Accepted
- Date: 2026-08-29

## Context

Programmable will have native EVM and SVM implementations. Treating the Solana
repository as the chain-neutral source would either force EVM concepts into the
SVM ABI or allow the two implementations to drift while using the same product
name.

## Decision

The Programmable Protocol specification owns the portable semantics for:

- Market, Liquidity Domain, Engine, Effect, Capability, and Intent;
- Protocol Assessment V1;
- evidence classes and receipt meaning;
- major-version compatibility; and
- language-neutral golden vectors.

`programmable-solana` is the canonical native SVM binding. It owns Program IDs,
PDAs, account layouts, CPI and privilege rules, SPL and Token-2022 profiles,
SBF/runtime resource limits, Solana events, clients, artifacts, and deployment
manifests.

The EVM binding owns contract calls, reentrancy, ERC asset profiles, gas,
events, addresses, and its own deployment evidence. Robinhood Chain is the
first planned production deployment through that separate binding. SVM work
continues in parallel and has independent release gates.

The bindings share semantics and conformance vectors. They do not share
bytecode, state, liquidity, custody, fee accounting, addresses, deployment
keys, or security evidence.

## Repository consequence

This decision partially supersedes ADR 0001 only where “canonical protocol
monorepo” could be read as canonical for every runtime. ADR 0001 remains
accepted for the structure and atomic release boundary of the SVM binding.

## Compatibility consequence

The same portable major on two runtimes means the named semantics and golden
vectors conform. It does not mean identical code or that an audit, deployment,
or safety claim transfers from one runtime to another.
