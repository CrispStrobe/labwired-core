# Diagnostic linker-map collector — hosted qualification pending

`link_map.py` is an R0 preparation step. It has not run an ARM linker, captured
an actual map or established licence clearance. Do not confuse passing pure
admission controls with successful diagnostic relinking.

PR175's repaired focused run37752827121 passed original build/configuration and
effective ARM preprocessing. Its artifact lacks the link recipe/map. This
follow-up captures the actual generated recipe before admission and accepts
only one direct original compiler
command with one output, rejects shell/response/existing-map recipes, and adds
only map reporting plus a separate diagnostic output. An unsupported recipe
requires explicit review, not an automatic fallback.

The original ELF must remain unchanged and the diagnostic ELF must be
byte-identical. Preserve the original recipe, linker output, actual map and
hash report, including any failure. Neither ELF belongs in the uploaded evidence
directory. No firmware executes or is disassembled. This separate diagnostic
invocation must not be described as capture of the original build's invocation.

The named hosted step follows effective preprocessing and precedes artifact
upload. Require all
enabled exact-head checks and an independent raw-map audit. The current helper
has no passing hosted result yet. Timeout and spawn failures retain explicit
failure state and any partial output. It does not inventory headers/inline contributions or decide which
components are permissible; those remain the separate R0/R1 review boundary.
