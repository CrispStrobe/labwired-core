# AVR `INC`/`RJMP` coalescing

The all-chip RTx gate made ATmega328P the fleet's lowest-margin part at 3.57x.
A 3,000,000-step Callgrind profile of the production batched CLI attributed
72% of its host instructions to AVR fetch/decode for the two-word throughput
fixture loop:

```text
INC r18
RJMP -2
```

Machine orchestration was below 1%, so widening the existing 113.8-step
machine window could not address the remaining cost. The AVR core now folds
this exact opcode pair in `step_batch`, from either entry phase and for odd or
even budgets. It computes the final register, `V/Z/N/S`, PC, and accumulated
AVR instruction cycles in closed form.

The path refuses near misses and falls back to ordinary instruction stepping
when batch mode is disabled, tracing or push capture is active, Timer0 is
running, or an interrupt is takeable. Those conditions make intermediate
instruction boundaries observable. Differential tests cover both PCs,
budgets 1 through 17, register/flag wrap boundaries, trace event counts, and a
Timer0 overflow interrupt.

On the same VPS and release binary:

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Batch Callgrind cost | 223.9 Ir/step | 12.0 Ir/step | -94.6% |
| One-simulated-second RTx, 3-run median | 3.57x | 21.43x | 6.0x faster |
| RTx range | — | 18.47x-22.13x | all passes |
| Steps per machine batch | 113.8 | 113.8 | unchanged |

Startup is included in the RTx measurement, matching `board_rtx.py` and the
GitHub all-chip acceptance gate.
