# Experiment provenance

This disposable workspace implements the corrected specification revision:

```text
commit c1150a0c6be896e8599ff98f85ecba8aa7fe0e22
tree   3fb719a3fa5a1ea225d546ce8a3dd848c11746a2
```

Those identifiers bind the specification only, not the implementation source.
The complete source of truth for the experiment requirements is
[`../../spec/generic-effect-capabilities-spike.md`](../../spec/generic-effect-capabilities-spike.md).
The earlier `38592d8`, `4ec495f`, `a132476`, `97984d1`, `d9bac36`, and
`3b1e767`, and `e2a2f7c` states are
historical pre-audit drafts and are not the implemented specification.

Source copied from another experiment is locally owned here and must identify
its semantic changes in the eventual result record. Shared source includes,
symlinks, cross-workspace path dependencies, and loading predecessor artifacts
are prohibited.

## Disposable program identities

```text
Core                    3mg7sM6RFEBHiiFotFNfvteH1WdFcc9cujKuPaqZdfDz
engine                  3qbR1eZRqXUWroWKKYhbDmR3FfqTHfqSU8zZSxtANzYh
replacement engine      3qbR1eZRqXUWroWKKYhbDmR3FfqTHfqSU8zZSxtANzYh
hostile router          3uWi9x2SRpmjztkpkr2WWeBoVq3exjXG2YfDWLvm8KsQ
callback helper         3yS1JFVT284y8z1LC9MRoWxZjzFrdoD5axKsZiyMsfC7
```

The two engine artifacts deliberately declare the same program ID so local
loader-v3 tests can replace code without changing identity. No matching private
key is generated, retained, or required.

The loader tuple proves only the Program/ProgramData/controller relation and the
ProgramData slot observed by the local runtime. It neither proves authority
history nor hashes the ELF per execution. PDA-controller governance remains
opaque. The immutable classification is conditional on active runtime and
loader semantics; mutable-controller admission explicitly carries controller
and liveness trust. Finalized-fork waiting remains separate release evidence.

## Build-output boundary

Only these SBF files may remain in `target/deploy`:

```text
target/deploy/programmable_generic_effect_core.so
target/deploy/generic_effect_engine_probe.so
target/deploy/replacement_effect_engine_probe.so
target/deploy/hostile_router_probe.so
target/deploy/callback_capability_probe.so
```

Artifact hashes and independent reproduction evidence belong in a later result
record. A local build is not deployment, onchain verification, production
readiness, or an accepted compatibility surface.
