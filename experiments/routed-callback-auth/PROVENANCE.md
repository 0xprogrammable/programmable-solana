# Experiment provenance

This disposable workspace starts from the repository state at:

```text
commit 1028824fdd22e4058a9ac97cd009283cdb838e63
tree   463d96bc1326a9e8a57d06e84a57088495b85936
```

Any source copied from that baseline remains locally owned by this experiment
and must identify its deliberate semantic changes in the eventual experiment
result. Shared source includes, symlinks, cross-workspace path dependencies, and
loading an older workspace's build output are prohibited.

## Frozen predecessor artifacts

The existing canonical Ubuntu SBPFv0 artifacts are:

```text
0826d2cf55b69908635cf5ed52c6a9f363413ce4dcb9858c3f0ee6bd7611c425  target/deploy/programmable_core.so
c29d44ee25b6451877eb4bf86de1ffcd53c10cca15bf6524101ef7c5a6442d38  target/deploy/programmable_spike_engine.so
abaa15b87555aae6fb78f657a667a08ab1709f148c63442c569c71aa1bf776ba  experiments/engine-generated-settlement/target/deploy/programmable_generated_settlement_core.so
6c42a3e845b1d5ce93fe9fc069d05c96e88841b3110a33125e3cf830bc4d5bfa  experiments/engine-generated-settlement/target/deploy/generated_plan_engine.so
5bbd777c59894b60c533abd50f99bbe3afc24c2784c9b00714e341e99071ee77  experiments/engine-generated-settlement/target/deploy/opaque_capability_probe.so
```

Those hashes belong to the two predecessor experiments. Nothing in this
workspace supersedes them or changes their evidentiary meaning. Their build
scripts, lockfiles, workspace members, source, and hash manifests remain outside
this experiment.

## New artifact boundary

This workspace uses four different disposable program IDs and writes only:

```text
Core                    Bwhiw9S9ZdHkEhFF2Ps89HMxa5dHX1xSbdsGZ8W3qR2b
engine                  5UNyG5GQpPwyoDgsvt4JzdqJxJzPh52pVbUDjEa5Gikh
hostile router          F62maceZqpLAayyBLsXNGdrmKg9cZWdpSDbzoHuNgk6Q
callback-capability     6QXXm7aqjRxQGJ6V3nvtS5taHuojM9SisVrHg3Xrj1Vj
```

The build output allowlist is:

```text
target/deploy/programmable_routed_callback_core.so
target/deploy/routed_plan_engine.so
target/deploy/hostile_router_probe.so
target/deploy/callback_capability_probe.so
```

Its artifact hashes require a separate manifest and result record. No generated
program keypair is retained, and a successful local build is not deployment or
release evidence.
