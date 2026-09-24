# Xtensa interpreter step loop — measured plan

Status: **plan, nothing implemented.** Every number below is from one
measurement, cited, so a later reader can re-take it rather than trust it.

## Provenance

- Tree: `origin/main` at `af544586` (before #61 merged; #61 removes one
  vtable dispatch from `step_batch`, −4 Ir/step, and changes nothing else
  below).
- Run: Core Profile `36038680956`, `esp32 esp32s3`, 10,000,000 steps,
  `batch`, perf-spin fixtures. Raw callgrind profiles are in that run's
  `core-profile` artifact (`callgrind-out/`), kept since this branch.
- Disassembly: Core Profile `36039877058` (`disasm` input, same tree).
- Per-line and per-call attribution was parsed straight from the callgrind
  format (self cost keyed by function × file × line; the line after
  `calls=` is the callee's inclusive cost and is kept separate). The parse
  sums to the file's `PROGRAM TOTALS` exactly (3,167,965,081 on esp32), so
  nothing below is a sample. `callgrind_annotate`'s per-file mode could not
  be used: on these profiles it reports "no information" for
  `xtensa_lx7.rs`.

## What the fixture is, and what it is not

`crates/firmware-perf-spin-xtensa` is `acc = acc.wrapping_add(1);
black_box(acc)` — about four guest instructions per iteration, **one of
which is a 32-bit store to the stack** (`black_box` spills `acc`), and **no
loads**. It is a fine instrument for per-instruction overhead. It is a poor
proxy for the mix of real firmware, which has 20–30% loads/stores, IRQs,
windowed calls and FreeRTOS task switches. Every hit rate this plan leans on
(rule 3 in the handover) must be re-measured on real firmware before a
change is called a win. Step 0 below exists for that reason.

## Where one guest instruction goes (esp32, batch, Ir per step)

Self cost unless marked inclusive. esp32s3 has the same shape to within
1 Ir on every interpreter line (same `step`/`execute` lines, same store
hooks); it differs only in where `pending_cpu_irqs` looks (intmatrix array,
no DPORT).

### A. The data-side bus path: ~103 Ir/step (inclusive), 32% of the program

`execute → SystemBus::write_u32` is called 2,499,302 times (one store per
four instructions) at **411 Ir per store**. The store itself —
`RamPeripheral::write_u32` — is **15** of those. On Xtensa, DRAM is a
`RamPeripheral` in the peripheral table, not the bus's `ram` field, so
every stack store falls through the `ram`/`flash` probe and takes the full
MMIO path:

| per store | Ir |
|---|---|
| `write_u32` own body (gates, alias/bit-band tests, probes) | 175 |
| `collect_scheduled_events` | 51 |
| `find_peripheral_index` | 44 |
| `is_peripheral_clocked` (a hashbrown lookup) | 30 |
| `refresh_bus_tick_index` | 30 |
| `maybe_latch_dc` | 27 |
| `sync_esp32c3_irq_cache_write` — **on a classic ESP32** | 22 |
| `RamPeripheral::write_u32` (the actual store) | 15 |
| `sync_esp32s3_irq_write` | 9 |
| `refresh_legacy_tick_index`, `mmio_access_class`, other | ~8 |

None of those hooks can do anything for plain RAM. This is rule 2 again —
the cost of a hook that cannot apply is the call — on the largest target
left. Loads are not in this fixture at all; on real firmware they take the
matching read path, and that must be measured, not assumed.

### B. Call scaffolding in the interpreter: ~75 Ir/step

| | Ir/step |
|---|---|
| `step` entry + exit (`xtensa_lx7.rs:2191`, `:2434`) | 18 |
| `step` line 0 (no line info: spills, moves) | 11 |
| `step_batch` → `step` call + `Result` check (`lib.rs:309`) | 9 |
| `execute` entry + exit (`:1117`, `:1857`) | 15 |
| `execute` line 0 | 16 |
| `execute` `match ins` dispatch (`:1174`) | 6 |

The disassembly says what the entries are: `step` pushes all six
callee-saved registers and reserves a 0xf8-byte frame; `execute` pushes six
and reserves 0x88, and returns `SimResult<()>` through memory. The
instruction *semantics* on this fixture cost 0.25–0.5 Ir/step per
`exec/*.rs` line. The handover's "execute: 39 Ir of semantics" is really
~2 Ir of semantics inside ~37 Ir of function entry, exit and argument
traffic.

### C. The IRQ poll: ~39 Ir/step

`pending_irq_level` (15 self) → `Bus::pending_cpu_irqs` (vtable) →
`dport_cross_core_pending` → `Peripheral::cross_core_pending` (vtable) →
`from_cpu_armed == false` → `0`. Inclusive from `step` it is 42 Ir/step:
two dynamic dispatches per instruction to compute zero.

### D. Everything else in `step`: ~45 Ir/step

Decode-cache hit ~25 (two separate `Vec`s, `decode_gen` and
`decode_cache`, so two runtime bounds checks, two cache lines and a
by-value entry copy; `:2294`–`:2297` plus `slice/index.rs` and `raw_vec`
lines inlined into `step`); CCOUNT/CCOMPARE0 ~9 (three SR accesses);
halted/parked/`faithful_windows`/preserve-stack checks ~8; zero-overhead
loop check ~4.

`maybe_restore_task_preserve` is worth naming although it costs nothing
here. The disassembly shows that when `faithful_windows` is false and
`call_preserve_stack` is empty, it calls `px_current_tcb(bus)` — a
thread-local plus a `bus.read_u32` through the vtable — **before** it
checks whether `task_preserve_by_tcb` is empty. On this fixture the call
edge is absent. On FreeRTOS firmware it may be on the per-instruction path.
Step 0 decides.

## The invariant the redesign leans on

Within one `step_batch`, the only code that runs is this CPU's `step`.
Peripheral events are delivered at batch boundaries: `plan_cpu_window`
clamps each window to the next scheduler deadline, and the parked-secondary
wake deadline, and the tick/drain happens in `boundary.rs` between batches.
So bus-side state the CPU reads per instruction (`pending_cpu_irqs`, DPORT
cross-core triggers) can only change **mid-batch** through this CPU's own
bus accesses, or through CPU-internal changes (WSR, `dispatch_irq`,
`clear_cpu_irq_pending`).

Two exits from that invariant are already in the tree and must be
preserved, not optimised:

- `boundary.rs` calls `cpu.step()` directly per instruction on the
  parked-secondary + RTC_CNTL arm, and `step_batch(…, 1)` on the quantum-1
  arm with a tick between each instruction. So nothing hoisted may live in
  `step`. It lives in an **Xtensa-owned `step_batch` override**, and
  standalone `step` keeps polling every instruction exactly as today.
- The dual-core parked path steps the secondary *after* the primary's batch.
  A cross-core IPI written by the primary is seen by the secondary at its
  next step, which is a batch entry. That is unchanged by a per-batch poll.

This invariant is an argument, not a measurement. The byte-identity oracle
below is what holds it.

## Correctness oracle (the bar each step must hold)

The one the 2026-09-18 batched work set (`2026-09-18-xtensa-batched.md`):
**stdout sha256 and bus-trace sha256 identical, step vs batched, at 5M and
40M steps** on `tests/fixtures/tier1/esp32s3-flash.bin` (`--rom-boot
--allow-sim-error`), plus `esp32s3_doom_frame_oracle`,
`esp32s3_irq_cache_differential`, `esp32s3_walk_differential` and the
C3/classic walk differentials. Each step below also has to show the
**before/after** hashes identical on both paths. That is a stronger claim
than step-vs-batched agreement: a change that moves both paths the same way
passes the latter and fails the former.

## Steps, in order

Order is by measured size divided by risk. Each step is its own PR, A/B on a
branch differing by that step alone (rule 5), Core Perf over all 59
board-modes split by mode for anything touching shared code (rule 4), and
the program total — not a per-function line — is the claim (rule 1).

### Step 0 — measure real firmware (instrument only)

Teach `profile_board.py` / Core Profile to profile a named firmware image
(`tests/fixtures/tier1/esp32.elf`, `esp32s3-flash.bin --rom-boot`) as well
as the spin fixtures. Re-take A–D on it. **Falsifier for this plan:** if on
real firmware the data-side path is under ~15% of the total, or the
`px_current_tcb` edge is hot, the order below changes before any code does.

### Step 1 — a plain-RAM data path on `SystemBus` (target: A, up to ~100 Ir/step here)

For a peripheral that *is* plain memory, the MMIO obligations reduce to:
the store itself, observer notification (`notify_peripheral_store`, already
one length check when unobserved), the memory-write counter, and whatever
`note_mmio_activity` records for RAM today (which feeds the timer-poll
coalesce eligibility — changing its classification changes behaviour, so it
is preserved, not dropped). Design: tag plain-memory peripherals once in the
range table built by `rebuild_peripheral_ranges`, and test that tag on the
caller's side of the hooks (rule 2's shape: inline presence test, the hooks
unchanged behind it). No raw-pointer data cache in the CPU in this step —
that is a second, riskier lever (RefCell-backed buffer, DMA, snapshot
coherence) and is not needed to delete the hook calls.

Open question the step must answer first: whether a clock-gated RAM
(`is_peripheral_clocked`) exists on any chip. If one does, the gate stays
on the fast path.

Shared code: all 59 board-modes, both modes. A chip whose RAM is served by
the bus `ram`/`flash` probe ahead of this path should pay only the new
branch, but that is to be confirmed per chip, not assumed, and a step-mode
cost is still a cost (see #57).

### Step 2 — hoist the IRQ poll out of the per-instruction path (target: C, ~39 Ir/step)

In an Xtensa `step_batch` override: poll `pending_cpu_irqs` at batch entry
and cache it; re-poll after any instruction that performed a bus access, or
changed INTERRUPT/INTENABLE/PS, or dispatched/returned from an IRQ. Pure
ALU/branch instructions (the majority) skip the bus poll. The SR half
(`INTERRUPT & INTENABLE`, level vs `PS.INTLEVEL`) stays per instruction —
it is CPU-local and cheap. `step` is unchanged.

Rejected alternative, recorded so it is not rediscovered: a shared
"IRQ epoch" atomic that the bus bumps (the `CycleClock` pattern). It would
be cheaper per instruction, but every writer of `pending_cpu_irqs` (a `pub`
field) would have to bump it. The batch-local rule needs no writer census.

Handover warning applies: reordering `pending_irq_level`'s tests was tried
and cost +1.94%. This step removes the call; it does not reorder it.

### Step 3 — fuse the step body into the batch loop (target: B, ~27 Ir/step)

Move `step`'s body into an `#[inline(always)]` helper used by both `step`
and the new `step_batch` loop, so the batch path pays `step`'s entry/exit
and the `step_batch→step` call once per batch rather than per instruction.
Hoist per-batch invariants: `observers.is_empty()`, `halted`, `jit_enabled`,
`live_step`, the logic tap. `execute` stays out of line for now. Inlining a
match of about 170 arms into two call sites doubles its code in the wasm
bundle the browser downloads. Measure that size before taking it on.

### Step 4 — decode cache as one array (target: part of D, ~10 Ir/step)

One `Box<[Entry; DECODE_CACHE_SIZE]>` with the generation inside the entry:
the mask then proves the index in range (no bounds checks), one cache line
per hit, and the hit returns by reference.

### Step 5 — CCOMPARE0 armed flag (the handover's warm-up, ~5 Ir/step)

Cache "CCOMPARE0 != 0" on WSR/XSR/snapshot restore; skip the compare read
when unarmed. Small, and last because it is small.

### Not planned yet: `execute`'s entry (15 + 16 Ir/step)

A 0x88-byte frame, six callee-saved pushes and an sret `SimResult` are the
shape of a large match with some heavy arms. A hot/cold split (common
ALU/branch/load/store arms in a small function, the rest out of line)
is the standard remedy. It is also the largest diff of anything here. It
waits until steps 1–3 have moved the total and the remaining profile says
it is still worth it.

## What would make this plan wrong

- Step 0 shows a different mix on real firmware (see its falsifier).
- A byte-identity hash moves on either path at any step. That stops the
  step. It is not a thing to rebaseline.
- Core Perf shows a step-mode cost across the Cortex-M board-modes larger
  than the Xtensa batch gain it buys. The #57 precedent: record it as a
  trade in the rebaseline, and don't let it pass as a win.
