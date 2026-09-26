# All-chip real-time floor

The performance gate now answers two different questions:

- `board_perf.py` uses deterministic Callgrind instruction counts to detect
  small regressions.
- `board_rtx.py` runs one simulated CPU-second through the production batched
  CLI path three times and requires the median to be at least 1.0x real time.

The absolute gate includes process startup and uses the configured silicon
clock, so a 600 MHz i.MX RT1064 has to execute 600 million simulated cycles in
at most one wall second to pass. Wall time remains unsuitable as a small-delta
regression metric; the deterministic gate continues to serve that purpose.

## Cortex compiler-loop repair

The current Rust fixture compiles its hot loop to:

```text
STR Rt,[SP,#imm]; MOV Rd,SP; ADDS Rt,Rt,#imm3; B loop
```

The specialized Cortex path recognized an older `ADDS; STR; LDR; B` shape, so
this loop fell into the generic decoded-block executor. Callgrind attributed
57% of a representative H735 run to that per-operation loop. A guarded
closed-form coalescer now handles the current four-instruction shape from every
entry phase while preserving PC, registers, NZCV, final RAM, and memory-access
accounting. It refuses non-RAM and self-modifying stores and is reachable only
under the existing observer, interrupt, debug, IT-state, and decode-cache
guards.

The Cortex batch cost fell from roughly 54 Ir/step to 2.7–5.0 Ir/step. On the
validation VPS, all 38 modeled chips cleared 1.0x. The slowest median was the
ATmega328P at 1.75x; the highest-clock Cortex targets were i.MX RT1064 at 4.36x
and STM32H735 at 2.54x. ESP32, ESP32-C3, ESP32-C6, and ESP32-S3 measured 8.15x,
4.88x, 4.36x, and 4.28x respectively.

Eight formerly waived chips now have real fixtures and baselines: ATSAMD21,
ATSAMD51, ESP32-C6, i.MX RT1064, nRF52833, RA4M1, STM32F746, and STM32G071.
That brings the deterministic matrix to 74 board-modes across 38 chips and ten
memory maps, with no coverage waivers.
