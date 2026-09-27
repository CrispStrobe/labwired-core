# Representative workload performance

The all-chip `firmware-perf-spin` gate is the deterministic engine-cost and
absolute-RTx contract. It intentionally cannot describe peripheral-heavy user
workloads. `scripts/perf/workload_perf.py` complements it with three real
firmware cases:

| Case | Product path | Evidence |
|---|---|---|
| `esp32c3-oled` | ROM boot, FreeRTOS, UART, I²C and SSD1306 | setup and first-paint latency, RTx, batch width, idle fast-forward, serial/display liveness |
| `nrf54l15-embassy-events` | Embassy executor, GRTC, GPIO and WFE | accelerated/reference throughput, exact GPIO event cycles, complete CPU/RAM/peripheral identity |
| `esp32c3-oled-identity` | same OLED firmware at tick 1 and the shipped recommendation | byte-identical framebuffer plus the cost of each lane |

The harness builds each Rust test first and runs its executable directly. Its
per-case `/usr/bin/time` receipt therefore includes process cold start and lab
construction, but excludes Cargo and compiler work. The structured JSON also
separates simulator setup from the measured run where the test exposes it.

Run all cases:

```sh
python3 scripts/perf/workload_perf.py --status-json workload-perf-status.json
```

Run a subset:

```sh
python3 scripts/perf/workload_perf.py \
  --workloads esp32c3-oled,nrf54l15-embassy-events
```

Wall time, CPU time and peak RSS vary by host and are diagnostic measurements,
not merge-blocking thresholds. The deterministic contracts remain:

- `scripts/perf/baselines.json` for host instructions per simulated step;
- the 1024 minimum recommended tick interval in `board_perf.py`;
- the workload tests' framebuffer, event-cycle and complete-state assertions.

The GitHub `Representative Workload Performance` workflow supplies a pinned
Ubuntu 24.04 comparison host and uploads both the human report and JSON receipt.

## First local receipt

Measured on the shared validation VPS from the 1024-interval tree:

| Workload | Result |
|---|---:|
| C3 OLED | 0.218x RTx; 333.4 mean batch; first paint at 137.5 ms guest / 851 ms wall; 21.8 MiB peak RSS |
| C3 tick 1 → 1024 | framebuffer identical; 5.51 s → 1.37 s (4.0x) |
| nRF54 Embassy GRTC | 298.3x accelerated RTx; GPIO intervals 32,000,323 and 32,000,256 cycles; complete state identical |

The C3 result is the important new finding: synthetic execution has ample
headroom, but peripheral-heavy boot-to-display is still below real time. The
receipt isolates that as follow-up optimization work rather than weakening the
fleet contract. GitHub receipts should be used for host-to-host comparison.

## Pinned main receipt and poll-loop follow-up

GitHub run `36294791073` measured commit `15bd583` at **0.405x RTx** for the
C3 OLED workload (462.7 ms run time for 187.5 ms of guest time). The paired
fleet run `36294779382` measured the synthetic C3 fixture at **12.12x RTx**.
Guest-PC attribution explained the difference: four addresses in the C3 mask
ROM's `lw; srli; andi; bnez` status-poll loop accounted for **73.29%** of all
interpreted instructions.

The follow-up RISC-V path recognizes only that decoded loop shape inside an
already permission-vetted fetch window. It still performs every load at its
exact guest cycle and preserves MMIO/memory accounting; it merely removes four
rounds of fetch, decode and dispatch. It is disabled with interrupts, observers
or cycle-accurate devices, and the interval-1 versus interval-1024 framebuffer
identity remains byte-exact. On the shared VPS, the 30M-cycle workload improved
from the original 858 ms receipt to 470–513 ms in repeated runs (about
**1.7x**, subject to shared-host noise), with identical 1,318 lit pixels, 3,287
serial bytes, CPU instruction count and final PC.

Pinned GitHub follow-up run `36297809406` measured the landed loop executor at
**0.692x RTx** (270.9 ms), a **1.71x** improvement over 0.405x, while the
tick-1/tick-1024 framebuffer remained identical. A second-stage optimization
lets a peripheral explicitly declare a register value stable only until its
next scheduled event. The C3 UART opts in solely for `STATUS` when no external
RX producer has ever been exposed; the bus then preserves the full MMIO access
count while avoiding millions of identical virtual reads inside one
already-event-clamped batch. On the VPS this improved the median again from
about 0.38x to 0.56x (roughly 1.5x); the pinned GitHub measurement is the
authoritative test of the 1.0x target.

Pinned merged-main run `36299064789` measured commit `a037b8d4` at **1.020x
RTx**: 183.7 ms wall time for 187.5 ms of guest time, first paint at 178.4 ms
wall / 137.5 ms guest, with 1,318 lit pixels and 3,287 serial bytes. The paired
identity lane again matched tick 1 and tick 1024. This meets the target on the
comparison host, though shared-VPS samples remain load-sensitive. A follow-up
profile-guided cleanup removed redundant WFI/deadline virtual queries and cut
Callgrind instruction references from 2.662B to 2.641B (0.79%) without changing
any guest receipt.
