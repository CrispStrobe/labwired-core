# Store-loop coalescing (2026-09-26)

The cross-architecture performance fixture uses `black_box` in a tight
accumulator loop. Current Rust compilers materialize that as a fixed stack
store on every iteration:

- ESP32-C3 RV32IMC: `C.SWSP; C.ADDI; C.J`.
- ESP32/ESP32-S3 Xtensa: `S32I.N; ADDI address-copy; ADDI.N; J`.

The ordinary ALU spin coalescers could not accept those loops because the
store is an architectural side effect. The new narrow coalescers recognize
only those exact compiler-emitted shapes, from every possible batch-entry
phase. They require a fixed ordinary-RAM destination and decline when an
interrupt, timer compare, observer, logic capture, permission gate,
cycle-accurate bus, or zero-overhead loop could distinguish the retired
instructions. They commit the exact final stored value and preserve register,
PC, cycle-counter, timer, reservation, and bus-access accounting.

The RISC-V recognizer decodes only from its permission-vetted mapped-code
window. The Xtensa recognizer uses the existing generation-tagged decode cache,
so it works for both IRAM and the classic/S3 MMU-backed XIP windows and inherits
their self-modifying-code invalidation.

## Native A/B

Measured on the same VPS and release binary with
`scripts/perf/board_perf.py`, using Callgrind's deterministic slope and a
512-cycle scheduler interval:

| Board | Before (Ir/step) | After (Ir/step) | Reduction |
|---|---:|---:|---:|
| ESP32 | 202.0 | 2.6 | 98.7% |
| ESP32-S3 | 196.9 | 4.5 | 97.7% |
| ESP32-S3-Zero | 196.9 | 4.5 | 97.7% |
| ESP32-C3 | 201.8 | 4.6 | 97.7% |

Wall-clock checks simulated one second of CPU time in 0.12 s (ESP32 at
240 MHz), 0.25 s (ESP32-S3 at 240 MHz), and 0.21 s (ESP32-C3 at 160 MHz):
approximately 8.3x, 4.0x, and 4.8x real time respectively on this host.

Focused differential tests compare the aggregate path with single-instruction
interpretation from every loop phase and multiple odd/even budgets, including
CPU snapshots, RAM contents, CCOUNT/mtime, and access counters. Negative tests
pin refusal for non-RAM destinations and a changing store base.
