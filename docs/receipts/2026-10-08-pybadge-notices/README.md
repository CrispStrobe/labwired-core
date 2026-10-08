# PyBadge notice candidates and remaining review — 2026-10-08

This follows the [source-identity receipt](../2026-10-08-pybadge-origins/README.md).
[PR183](https://github.com/CrispStrobe/labwired-core/pull/183)'s first focused
[run37801846339, job113395797438](https://github.com/CrispStrobe/labwired-core/actions/runs/37801846339/job/113395797438)
passed. Reviewed source: `b7dd3182259741b7f19bba60f9883c45fb51060a`.
Actual checkout: `aaafd0750e571030f589be17ad95015a47d6e492`, tree
`309afdf348b154f5d9d157eab23887834b060e70`. PR183 then merged at
`f6e2d89a026e44a6b86b46e693ab5ba5c181e36c` after all 15 enabled checks passed;
official landed tree equals that tested tree and the reviewed merge prediction.
Merged-main checks are separate. No runtime or licence clearance follows.

Official [artifact11563635469](https://api.github.com/repos/CrispStrobe/labwired-core/actions/artifacts/11563635469/zip):
1383626 B; unchanged ZIP SHA256
`b28c60f99ef802ccc162c8f16556ec5cc7c8f0e53b63333c3ef9a7db33268dd6`.

## Qualified capture, not classifications

All 542 dependency records bind to source/origin/report hashes. There are 409
captured leading-comment candidates, totalling 486412 B, and 133 records with no
leading comment. Captures stop before implementation/preprocessor text. Absence
never grants permission; descriptive comments may not be licence notices.

Five captured repository licence files were checked against their pinned Git
contents: root CODAL plus codal-core, codal-itsybitsy-m4, codal-samd and
samd-peripherals. Earlier draft prose counted only the four library files and
omitted root CODAL; the qualified total is **five**. These contain MIT notices,
not whole-firmware clearance. The ASF4 root-notice list is empty, not permissive.

| Installed package | Recorded version | Captured copyright SHA256 |
| --- | --- | --- |
| libnewlib-dev | `4.4.0.20231231-2` | `0383bc85177c121b1ee5e00ff07f51d0cab2dd5bbe51875fdff01a91c234c3ae` |
| libstdc++-arm-none-eabi-dev | `15:13.2.rel1-2+26` | `3b7987047d8ba1d4b1e01dcc8cc73583db32822bb991921fade91333d64ef68e` |
| gcc-arm-none-eabi | `15:13.2.rel1-2` | `ba40708beab7133c5b5bc25f28b4fc677617aabdb1368c1889b859b335edf0a1` |

These package records account for all 36 toolchain headers. Both GCC-related
copyright files explicitly cover packaging only and point to additional GCC
notices. Those references are **not captured or resolved** by this collector.
Nineteen comment candidates contain runtime-library-exception phrases; 93 contain
vendor-use phrases. Phrase detection is evidence for review, not a permission
decision or complete exception/obligation analysis. Installed package ownership
is not exact package-archive/source-to-binary provenance.

A separate root read-only standard-library audit verified ZIP bounds, safe paths,
absence of firmware binaries, candidate/source/origin/report graph correspondence,
every captured prefix hash and comment-only grammar, package capture hashes and
five repository-notice capture hashes. It imported no production source helper
and ran no compiler/guest. Pinned licence-byte comparisons and package ownership
remain hosted observations. This is integration evidence, not isolated authorship.

## Bounded map-family scope for R1

A separate read-only metadata pass over the qualified original map (SHA256
`2322e427353ba623b3efadbccfcef1a2ee50c09322d8016dc05b1b782bdb1f64`)
reconciled 74 allocated ASF4 input sections with the previous six-object inventory:

| Section family | Allocated bytes |
| --- | --- |
| SPI HAL and low-level SPI sections | 452 |
| Atomic entry/leave | 28 |
| ADC HAL and low-level ADC sections | 540 |
| I2C HAL and low-level I2C sections | 1464 |
| USART interrupt helper | 116 |
| 24 SERCOM vector-handler sections | 288 |
| Two rodata sections | 48 |

Total 2936 B includes 48 B rodata, not pure instructions; the existing 24 B BSS
is separate. These are naming groups of retained sections, not a complete call
graph, executed-route evidence or runtime cost share. SPI-only replacement does
not clear other retained or header/startup/transitive contributions.

## Next bounded tasks

1. Capture the additional GCC notice references and applicable exception texts
   from the exact installed package context. Preserve raw bytes and reference
   chains; fail or explicitly retain missing/ambiguous links. Correct the earlier
   four-notice draft count when next changing that collector's documentation.
2. Review candidate notices against exact component/file origins. Record immutable
   notice locations and applicable attribution/obligations without extending a
   root MIT notice to nested vendor components. Unknowns remain unadmitted.
3. Verify generated-header derivation, excluded assembly/startup and remaining
   linker/script/table contributions. Header inclusion does not prove retained
   inline instructions; do not claim complete source-to-binary correspondence.
4. Use the map families plus exact ABI/register contracts to choose attributed
   permissive R1 slices and authored positive/negative guests. No historical
   image-policy bypass, invented completion or hardware-flashing approval.

For documentation publication, preserve [CI37798410376](https://github.com/CrispStrobe/labwired-core/actions/runs/37798410376)'s
failed outcome: shard 1 exceeded its 45-minute job limit after the cross-compiler
installation consumed about 29 minutes. It uploaded no shard report, and the
aggregate correctly failed. This is not a test PASS or evidence of a particular
guest defect. Updated documentation needs its own final-head checks; do not merge
using earlier or incomplete checks.

No native-runtime boot, active/WASM RTx, app adoption, image admission, physical
capture, hardware safety or legal/store-clearance claim is established here.
