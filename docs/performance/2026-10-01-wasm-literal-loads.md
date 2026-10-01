# WASM cached literal-load experiment

Baseline: landed bounded-run core `43b2d62f5a0fa24ae0b38a645069f5aaa78af685`.
The previously measured fixed runtime source `2f5d9355` has identical runtime
source to that landed baseline; artifact provenance must keep its actual source.

This candidate adds only cached T16 `LDR (literal)` to the WASM bounded run.
Native production dispatch is unchanged. PC alignment/wrapping, destination
registers, flags, retirement counts and memory-read accounting must match the
ordinary interpreter. Existing debugger/observer/IRQ/IT/scheduler guards and
the 16-retirement limit are unchanged.

The new bus helper admits a complete word below `0x20000000`, only with optimized
bus routing, no overlapping extra-memory window and RX-buffer tracing disabled.
It preserves RAM-before-flash precedence and the existing flash boot alias.
Unrelated high nRF errata-probe windows are allowed; overlapping or partial-word
windows, high addresses, MMIO and incomplete/unmapped memory decline before
retirement or accounting. Every access checks live mappings and memory; no
address/value cache or snapshot field is added.

Tests cover all eight T16 destinations, both PC alignment phases, three immediate
offsets, direct low/STM32 flash, boot aliases, RAM precedence, unrelated extra
windows, partial/complete extra-window overlaps, incomplete words and declines.
The existing all-halfword/three-flag-state differential now also covers literal
loads. Mixed-run budget/barrier tests include a literal load followed by ALU/NOP
and a WFI barrier.

This is an experiment, not a measured speedup or realtime qualification.
Before landing, require the full reference tests, independently deterministic
WASM builds, non-skipped actual-WASM integration, fresh every-window qualification
and repeated same-runner exact-artifact A/B against the bounded-run baseline.
Keep original paired glue and raw samples; do not compare absolute rates across
hosts. Do not publish or promote app pins while the unchanged floor fails.
Existing hardware acknowledgement expiry/capture evidence is not modified by
this candidate; any content-bound acknowledgement update needs explicit review
after verification, and physical re-capture remains owed.
