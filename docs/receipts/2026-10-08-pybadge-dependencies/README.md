# PyBadge dependency evidence — 2026-10-08

This extends the [original-map receipt](../2026-10-08-pybadge-link-map/README.md).
It distinguishes surviving original dependency output from separately generated
diagnostic rules. Neither is licence clearance or retained-inline attribution.
PR179 and PR180's remaining merge checks are separate from focused results;
refresh their state before integration. No engine/app pin or image classification
changes are authorized by these results.

## Original-rule discovery and preserved failures

| Attempt | Actual outcome | Official artifact | ZIP SHA256 |
| --- | --- | --- | --- |
| [37785853900](https://github.com/CrispStrobe/labwired-core/actions/runs/37785853900), source `dc495bbf74244d654dfeb9a27791bcec92d55318` | Collector failed: no `*.o.d`; build, preprocessing and map passed | [11555080636](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11555080636/zip), 485630 B | `a74e9f80b7e2460de7345931af1bf75abee723d52adfcd9cf52b0e08d83076ba` |
| [37787234826](https://github.com/CrispStrobe/labwired-core/actions/runs/37787234826), source `43f3c9fc3a5ff8c02fb67f673212f9b6754a02aa` | Same collector requirement failed; retained 23 generated metadata files and 273-file name/size inventory | [11555001931](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11555001931/zip), 526064 B | `b2d61819793c928fce098073fa9c40ead3ebb794e7a8be9e80f8033748887c4d` |
| [37787957105](https://github.com/CrispStrobe/labwired-core/actions/runs/37787957105), source `cda15ec40c4c1d451b3519ef36d2daf38594c4f2` | Surviving original-rule capture passed; coverage explicitly incomplete | [11555187980](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11555187980/zip), 553777 B | `108073ae593872721dd0f5ae8b06f37e4214f11a4f256120a2c81a392fa2eae6` |

The actual generated CMake commands use the shared literal `-MF DEPFILE`, so
successive compilations overwrite earlier lists. Four surviving rules cover
246 unique dependency files: screen `image.cpp` (not `screen.cpp`), codal-core
`Image.cpp`, `Itsy.cpp` and `ZSingleWireSerial.cpp`. The report explicitly states
`completeCompiledCoverage=false` and `screenTranslationUnitObserved=false`.
These are collector failures followed by partial evidence, not firmware failures
or a recovered complete original dependency inventory.

A separate root read-only standard-library audit reproduced raw-rule graph
correspondence after lexical path normalization. Its initial assumption that raw
paths were already normalized failed and was corrected without changing the
artifact. No source helper, compiler or guest was imported/executed by that audit.

## Separate dependency-only probes

[PR180](https://github.com/CrispStrobe/labwired-core/pull/180)'s first focused
[run37790231661, job113355379328](https://github.com/CrispStrobe/labwired-core/actions/runs/37790231661/job/113355379328)
passed. Reviewed source `27d9400e7e6ac1ad053f2cdca1e9452b32a22aef` and actual
checkout `c166b8cc656153cf9588f893713b5b42e1f26fd0` share tree
`5a8a70fea4d962524d622eded06f4688206ecbdd`.

Official [artifact11555963815](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11555963815/zip):
902535 B; ZIP SHA256
`c311c3ffa3f20c473b1cfadce0da65fa75521d7cdc2d279bb629d6eb65210f9b`.
Original archives were acquired once and retained unchanged; artifact expiration
does not authorize inventing replacement bytes or altering their recorded hashes.

| Observed boundary | Result |
| --- | --- |
| Generated C/C++ commands probed with separate `-M` invocations | 172 |
| Explicitly excluded assembly commands | 1, `CortexContextSwitch.s` |
| Unique dependencies | 542: 506 build-source, 36 toolchain |
| Exact original `screen.cpp` dependencies | 232 |
| Diagnostic stderr | All 172 files empty |
| Original ELF identity before/after, hosted observation | `034a07b60e022c818e2ef51e98bfd0a9452ac920b21de079aaff32c9aaf7bf73` |

The exact screen source hash is
`dea9ea175d65d885275eb0715d56353674feae88d64d29a5eb2809f899a0d958`.
Generated commands/flags are parsed without a shell. Source/recipe/flags/compiler
hashes, raw rules and compiler identity records are retained. This uses separate
preprocessing only; it does not modify original flags, compile objects, relink,
run/disassemble the firmware or upload firmware binaries. The 93 vendor-phrase
hints are review aids, not component licence classifications.

A separate root read-only standard-library audit verified ZIP bounds, safe paths,
absence of firmware binaries, raw-rule hashes/targets/dependency graphs, exact
generated-command/flags/diagnostic-invocation binding, all 172 command records
plus the assembly exclusion, compiler-record hashes and original-map/ELF-record
correspondence. Source/compiler/image byte hashes remain hosted observations;
the audit did not fetch all source bodies or independently replay the binary.
This is separate audit code, not a claim of a second independent author.

## Remaining R0 work and handoff

1. Finish every enabled exact-head check and merge PR179 before PR180. Compare
   reviewed/tested/landed trees; later documentation is not an executable change.
2. Review the 542 dependency records against exact immutable component origins
   and notices, including generated headers and toolchain obligations. Directory
   names or vendor-phrase absence never grant permission; keep unknowns explicit.
3. Review the excluded assembly/startup and remaining script/table contributions.
   Diagnostic C/C++ dependencies do not establish which inline bytes were retained.
4. Combine this inventory with allocated/discarded map evidence to choose the
   [R1 replacement boundary](../../engineering/samd-permissive-runtime-lanes.md).
   Preserve attribution and implement separately named permissive slices with
   authored guests; do not weaken historical-image admission to make them run.

No native-runtime boot, loaded CF2/path selection, module parity, active/WASM RTx,
physical capture, hardware safety, app artifact adoption or store approval is
established here. Existing translated/PXT routes remain separate.
