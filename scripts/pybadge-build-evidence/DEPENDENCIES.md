# Compiled header/source dependencies — qualification pending

The first hosted attempt, [run37785853900](https://github.com/CrispStrobe/labwired-core/actions/runs/37785853900)
at source `dc495bbf74244d654dfeb9a27791bcec92d55318`, failed because the
collector found no `*.o.d` rules after the original build and map capture passed.
It remains a collector failure, not dependency qualification or firmware failure.
The follow-up retains bounded generated CMake dependency metadata plus a build
file-name/size inventory before requiring GCC rules. Inspect that actual discovery
before adding support for another format; no synthetic dependency list substitutes
for evidence. No object, ELF, HEX or source bodies are copied by discovery.

The original-map qualification in PR177 establishes a bounded object-level
inventory, not the provenance of headers or inlined code. This follow-up captures
the clean build's original GCC `.o.d` rules and inventories their dependency
paths/hashes, separating build-source from toolchain files. Raw source bodies
are not copied into the artifact; raw dependency rules are retained unchanged
for independent parsing. Presence of a vendor-use phrase is a review
hint only; its absence never grants permission or classifies a file as clean.

The scope is deliberately conservative: **all observed compiled units**,
with `.o.d` rules, including code the linker may discard. Missing rules and
assembly coverage remain explicit gaps. It does not establish which header bytes
became retained instructions, complete binary correspondence, or licence
clearance. Review against the actual map and immutable component origins before
choosing a permissive replacement boundary. Source access must remain recorded;
this is not an isolated implementation or whole-firmware clean-room claim.

Require an actual hosted artifact and all enabled exact-head checks. Missing or
unsupported GCC rule syntax fails explicitly; no fallback invents a dependency
list. No compiler/linker/guest execution is needed by this collector, and no app
pin, historical-image classification, hardware ACK or benchmark floor changes.
