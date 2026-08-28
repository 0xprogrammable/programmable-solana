# Security policy

## Current status

No program from this repository is deployed or approved for production use.
There is currently no supported production version and no invitation to deposit
real assets.

## Reporting a vulnerability

Report a vulnerability through
[GitHub private vulnerability reporting](https://github.com/0xprogrammable/programmable-solana/security/advisories/new).
Do not open a public issue for an undisclosed vulnerability and do not include
private keys, seed phrases, RPC credentials, or personal data in a report.

Include the affected commit or release, the violated security property, a minimal
reproduction, and the expected impact when possible. Do not test against systems
or assets you do not own or have permission to use.

## Scope

Once implemented, this policy covers:

- the onchain core;
- the published engine interface;
- maintained reference engines and canonical clients; and
- repository-controlled build and release automation.

Third-party engines, tokens, interfaces, RPC providers, indexers, and applications
have separate trust boundaries. A weakness in one of them may still affect its
users, but its integration does not make it maintained or certified by
Programmable.

## Security claims

The intended properties are tracked in
[`spec/security-properties.md`](spec/security-properties.md). A draft property is
not an implementation claim. Verified source provenance does not replace code
review, testing, or an assessment of administrative authority.
