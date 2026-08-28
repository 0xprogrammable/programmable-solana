# Experiment provenance

This disposable workspace starts from the authority-kernel implementation at:

```text
e9b58267c7bd405bc781307a1ebaa5fc3a18e7af
```

At scaffold creation, this workspace contains no program or crate source. Any
later source copied from the baseline must remain locally owned by this
experiment and identify its deliberate semantic changes in the experiment
result. Shared source includes, symlinks, and cross-workspace path dependencies
are prohibited.

## Frozen authority-kernel artifacts

The existing canonical Ubuntu SBPFv0 artifacts are:

```text
0826d2cf55b69908635cf5ed52c6a9f363413ce4dcb9858c3f0ee6bd7611c425  target/deploy/programmable_core.so
c29d44ee25b6451877eb4bf86de1ffcd53c10cca15bf6524101ef7c5a6442d38  target/deploy/programmable_spike_engine.so
```

Those hashes belong to the repository-root authority-kernel experiment. Nothing
in this workspace supersedes them or changes their evidentiary meaning. The root
build script, root lockfile, root workspace members, old source, and existing
hash manifest remain outside this experiment.

## New artifact boundary

This workspace uses three different disposable program IDs and writes only:

```text
target/deploy/programmable_generated_settlement_core.so
target/deploy/generated_plan_engine.so
target/deploy/opaque_capability_probe.so
```

Its artifact hashes require a separate manifest and result record. No generated
program keypair is retained, and a successful local build is not deployment or
release evidence.
