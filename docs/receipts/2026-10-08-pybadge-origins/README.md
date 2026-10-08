# PyBadge dependency source identities — 2026-10-08

This follows the [dependency-only probe receipt](../2026-10-08-pybadge-dependencies/README.md).
[PR182](https://github.com/CrispStrobe/labwired-core/pull/182)'s first focused
[run37794963356, job113371912038](https://github.com/CrispStrobe/labwired-core/actions/runs/37794963356/job/113371912038)
passed. Reviewed source: `6ba388076c83ffe0238be5cc6c15b768b069ec43`.
Actual checkout: `844deb7228cc0b983a8d64520318055c6c0d4808`, tree
`b3caafa8c2e59fcbb14670ec93f73fc171667df8`. That tree matches the reviewed
merge prediction against main `9402ea6788ddca21e1e265d301e7e389675d5a68`;
the branch source tree differs only by PR178's three already merged Markdown
paths. PR182's other enabled checks remain required before merging. This is
source identity evidence, not component licence clearance or runtime admission.

Official [artifact11559576651](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11559576651/zip):
948805 B; unchanged ZIP SHA256
`85361c5387b4eddf7bb0622b64cc89c09f4f367e465e9b12a8a36723620b7f27`.
The collector binds its result to the diagnostic report, original request and
builder manifest hashes. It compares original dependency bytes to exact regular
Git blobs, original request text, or the captured generated forced header. No
implementation body or firmware binary is copied into this artifact by the
origin collector, and no firmware is run/disassembled.

| Identity group | Files | Exact source boundary |
| --- | --- | --- |
| codal-core | 182 | [312ae57e](https://github.com/lancaster-university/codal-core/tree/312ae57e0b31f5b9df07a81e9d846945828e3c5a) |
| ASF4 | 178 | [6664673f](https://github.com/lancaster-university/asf4/tree/6664673f70d9170b4374a726c5322e9be6b3f237) |
| codal-samd, excluding nested submodules | 44 | [5bd6b93c](https://github.com/lancaster-university/codal-samd/tree/5bd6b93c219c7e784e885ba2d6812809fb6289a8) |
| samd-peripherals | 25 | [96563308](https://github.com/lancaster-university/samd-peripherals/tree/96563308fc7b97646cbe429953e79cb3405846f0) |
| codal-itsybitsy-m4 | 8 | [6ecd80cc](https://github.com/lancaster-university/codal-itsybitsy-m4/tree/6ecd80ccf1abcc126d06653f178ca03a4d1c6421) |
| Root toolchain header | 1 | [codal 7cf09a3a](https://github.com/lancaster-university/codal/tree/7cf09a3a37c8aca6e100b7265c81e6cdf6129dd2) |
| Original request application files | 67 | Exact captured `replaceFiles` bytes |
| Generated forced header | 1 | Captured bytes; full derivation still open |
| Toolchain headers | 36 | Explicitly unreviewed |

The first six groups total 438 pinned repository files; the complete diagnostic
inventory totals 542. Longest repository-prefix ownership handles nested
submodules; directory names do not assign licences. The 93 vendor-use phrase
hints all occur under ASF4 but are not component classifications. Missing,
modified, symlink or ambiguous pinned source fails rather than being re-pinned.

## Separate audit and limits

A root standard-library-only read-only audit checked ZIP bounds/paths/absence of
firmware binaries, exact record census, report/request/manifest hash bindings,
all 172 raw diagnostic-rule hashes and the one assembly exclusion. It rehashed
all 67 request files and the captured forced header, then independently compared
all 438 blob IDs, paths and regular-file modes against six official recursive
Git tree API responses at the exact pinned commits. No production source helper,
compiler or guest was imported/executed by the audit. It downloaded metadata,
not the 438 implementation bodies. Repository-file SHA256 comparisons remain
hosted observations; this is not independent replay of every source byte.

The hosted collector does read source bodies for hashing. Preserve that actual
access record; neither separate audit code nor hash-only access establishes
isolated implementation, whole-firmware clean-room origin or training independence.

## Next review lanes

1. Finish all PR182 exact-head checks, merge normally and verify the landed tree.
   Do not repeat the successful guest-free probe just to read its evidence.
2. Review exact file/component notices and obligations for the 438 repository
   files and 67 request files. Link immutable notice locations; distinguish
   nested component notices, file-specific restrictions and actual attribution.
   Unknowns remain unadmitted; phrase absence and a permissive directory label
   are insufficient.
3. Review the 36 toolchain headers' exact package/component origins and applicable
   notices/obligations. Verify generated-header derivation separately; captured
   byte equality is not full generator provenance.
4. Review excluded assembly/startup and remaining linker/script/table origins,
   then combine this with allocated/discarded map evidence for the
   [R1 replacement plan](../../engineering/samd-permissive-runtime-lanes.md).
   Diagnostic header inclusion does not identify retained inline instructions.

No historical-image admission, complete source-to-binary proof, native-runtime
boot, loaded CF2/path, active/WASM RTx, app pin, physical capture, hardware safety
or store approval is established here. The translated/PXT route remains separate.
