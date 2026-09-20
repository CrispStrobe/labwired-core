# ESP32-C6-DevKitC-1 — Known Limitations

Scope of this list: the LabWired `esp32c6` chip descriptor
([`configs/chips/esp32c6.yaml`](../../configs/chips/esp32c6.yaml)) and the
ESP32-C6-DevKitC-1 system
([`configs/systems/esp32c6-devkitc.yaml`](../../configs/systems/esp32c6-devkitc.yaml))
as validated by the tier-1 fixture
([`examples/tier1-fixture/esp32c6/`](../../examples/tier1-fixture/esp32c6/)).
If it is not listed under **Proven at tier-1**, treat it as unmodelled no
matter how plausible the register map looks. This is a
**SIM-DERIVED** target: nothing here has been diffed against live C6 silicon,
and no firmware beyond the in-tree fixtures/apps has been run.

## Proven at tier-1 (documented scenarios)

The six L3 rubric classes, proven by `tests/fixtures/tier1/esp32c6.elf`:

| Class | What is proven |
|-------|----------------|
| uart | Implicit: the `TIER1` transcript arrives over UART0 (Espressif UART twin, 128-byte FIFO, paced at the descriptor's 160 MHz). |
| clock | PCR is a real register file (full SVD map): `UART0_SCLK_CONF`/`SYSCLK_CONF` round-trip their documented values. `UART0_CONF.CLK_EN=0` makes UART0 reads return 0 and **drops** writes (the pre-gate value survives re-enabling); `CLK_EN=1` restores normal operation. The same gate mechanism is declared for UART1, TIMG0, TIMG1 and GDMA. |
| gpio | `OUT`/`ENABLE` stores plus real `W1TS`/`W1TC` set/clear side effects; `FUNCn_OUT_SEL_CFG`/`FUNCn_IN_SEL_CFG` words round-trip; `IN` does not follow the output latch. |
| timer | TIMG0 (0x6000_8000, shared `esp32_timg` model): `T0CONFIG.EN` set → `T0UPDATE`-latched `T0LO/T0HI` advances across a bounded spin; `EN` cleared → the counter is frozen. |
| dma | GDMA (0x6008_0000, `esp32c6_gdma`): a real in-RAM linked-list mem→mem transfer. Descriptors are walked, bytes land in the destination, `IN_SUC_EOF/IN_DONE` and `OUT_TOTAL_EOF/OUT_DONE` latch, the EOF-descriptor addresses are recorded, RX descriptors get owner-cleared with the received length, and `OUT_AUTO_WRBACK` returns TX descriptors. |
| irq | A real RISC-V trap: `CPU_INTR_FROM_CPU_0` (matrix source 22) → MAP → enabled line 9 → `mcause=0x8000_0009`; the handler runs and acknowledges. Disabling the line masks a second doorbell. |

## Partially modelled

| Area | What exists | What is missing / could mislead |
|------|-------------|---------------------------------|
| PCR (`esp32c6_pcr`, full SVD register map) | All documented PCR registers store/read back with SVD reset values; `CLK_EN` gates are resolved and enforced through the shared bus gate. | `RST_EN` is recorded but **not enforced** (the generic gate cannot express its active-low/reset-at-boot polarity without gating the console at reset). No clock tree: dividers, `SYSCLK_CONF` mux, `PLL_DIV_CLK_EN` are storage only; nothing is paced from them (`cpu_hz` stays the single clock fact). Only HP PCR; LP clock registers are absent. |
| UART0/UART1 (`esp32c6_uart`) | Head register map (FIFO/INT/CLKDIV/STATUS/CONF0/CONF1), 128-byte FIFOs, interrupt RAW/ST/ENA/CLR, baud-paced shifting, UART0 echoes to the host sink. | C6 tail registers (`SLEEP_CONF0-2` @0x30-0x38, `CLK_CONF` @0x88, `DATE` @0x8C, `ID` @0x9C) are unmodelled: those offsets answer with the C3/S3 tail layout. Flow control, RS-485, IR modulation, wake-up, and DMA (UHCI) coupling are not modelled. `INT_RAW` bits latch, but **no UART interrupt has been routed through the C6 matrix by a fixture** (see below). |
| GPIO (`esp32c6_gpio`) | Shared C3-sized register block: OUT/W1TS/W1TC, ENABLE, IN, FUNC0_IN_SEL_CFG, FUNC0_OUT_SEL_CFG side effects. | Sized for 26 pins: **GPIO26–GPIO30 are not modelled** (C6 has 31). No GPIO interrupt generator fed to matrix source 30. IO_MUX pad routing is a register stub — the console shifts bytes regardless of `MCU_SEL`. |
| TIMG0/TIMG1 (`esp32_timg`) | T0 counter with EN gate, LO/HI latch via UPDATE, preload, INT_RAW/INT_CLR plumbing, generic round-trip for other offsets. | One general-purpose timer per group on C6 (the model's T1 head at 0x24 is inert phantom storage, as on C3). The counter advances 1 per simulated bus tick, **not** at the programmed divider/clock-source. Watchdog timing/reset is not modelled (the WDT feed/write-protect offsets even differ from classic; the model's WDT writes are documented no-ops). **No TIMG interrupt is fired** — `TG0_T0_LEVEL` (51) is declared on the descriptor but nothing routes it. |
| GDMA (`esp32c6_gdma`) | 3 channels, C6 register map (SVD), M2M descriptor walk with owner/CHECK_OWNER/OUT_AUTO_WRBACK semantics, RAW/ST/ENA/CLR interrupts, EOF-descriptor addresses. | **No peripheral-coupled pumps** (SPI2/UHCI0/I2S/AES/SHA/ADC/PARLIO): a START with a real peripheral bound stalls visibly (no EOF, `needs_bus_tick` stays true) rather than moving bytes. FIFO data ports (`IN_POP`/`OUT_PUSH`) are stubs; `*FIFO_STATUS` always reports empty. No priority arbitration, ETM, burst tuning, `IN_RST`/`OUT_RST`. Descriptor address reconstruction assumes the 20-bit LINK field prefixes `0x4080_0000` (HP SRAM) — see the model docs for the TRM inference. No fixture has routed the DMA matrix sources (66–71) through the fabric. |
| Interrupt fabric (`interrupt_core0` + `intpri`) | Matrix MAP words are real; asserted sources route through enable/priority/threshold into the RISC-V core; the software doorbells (`CPU_INTR_FROM_CPU_n`) deliver real traps. | **Peripheral-sourced interrupts are unproven**: UART0 (43), UART1 (44), TIMG (51–56), GDMA (66–71) source ids are declared, but no in-tree fixture drives one. C6 GPIO does not emit source 30. No CLIC/`CPU_INT_TYPE` edge-vs-level programming, no `CPU_INT_CLEAR` write path, no U-mode/privilege handling. |
| INTPRI register block | `CPU_INT_ENABLE`, `CPU_INT_PRI_n`, `CPU_INT_THRESH`, `CPU_INTR_FROM_CPU_n` are functional (the fixture proves level delivery + masking). | Remaining SVD registers in the window (`CPU_INT_TYPE`, `CPU_INT_CLEAR`, `CLOCK_GATE`, `DATE`) are declarative storage only. |

## Not modelled

- **LP core** (RV32IMC) and the LP/LP_IO/LP_AON periphery; deep sleep, retention
  and wake-up. LP SRAM is mapped as plain memory so RTC/LP code does not fault.
- **ROM boot path / bootloader**: the 320 KB mask ROM region is optional
  (`LABWIRED_ESP32C6_ROM`) and is not the reset path of the tier-1 ELF. No
  flash-controller model, partition parsing, secure boot, or flash encryption.
- **Radios**: Wi-Fi 6 (MAC/PHY), Bluetooth 5 LE, IEEE 802.15.4, and the
  coexistence block. No MAC, no PHY, no packet path.
- **USB Serial/JTAG** (`USB_DEVICE` window): unmapped (accesses fault loudly).
- **SYSTIMER, RMT, LEDC/PWM, I2C, SPI0/1/2, I2S, TWAI, PCNT, ETM, PARL_IO,
  SAR ADC, TSENS, AES/SHA/RSA/ECC/HMAC/DS, eFuse, PMU, trace** — not declared;
  accesses to these windows fault loudly rather than pretending to work.
- **Watchdog resets**: WDT counters/feed semantics are not simulated.
- **Electrical layer**: IO_MUX drive strength/pull/function, pin muxing,
  analog, power, antenna/EMI. Do not run board-level electrical validation.
- **Cycle accuracy**: no bus arbitration, cache, flash wait states, or
  pipelining model; timing is functional, not cycle-accurate.

## Evidence boundaries

- No silicon bench diff: register/reset values come from the vendored
  `tests/fixtures/real_world/esp32c6.svd` and the ESP32-C6 TRM/datasheet,
  cross-checked against esp-idf v5.3 register headers where cited.
- The tier-1 transcript is generated by the committed fixture
  (`tests/fixtures/tier1/esp32c6.elf`, hash-pinned in `MANIFEST.json`) on the
  committed chip descriptor; see
  [`VALIDATION.md`](VALIDATION.md) for the exact commands and observed output.
- The instruction audit covers the fixture's dynamic instruction stream only;
  it is not a static proof for arbitrary firmware.

## Known follow-ups (in rough priority order)

1. Route a **peripheral-sourced** C6 interrupt (e.g. UART0 `RXFIFO_FULL`
   source 43 → `UART0_INTR_MAP` → INTPRI → trap) and add it to the fixture.
2. Implement **TIMG alarm interrupts** (`TG0_T0_LEVEL`) from the shared model.
3. Add **coupled GDMA pumps** (UHCI0 first — UART DMA — then SPI2/I2S) and
   cross-check the C6 PERI_SEL encoding.
4. GPIO interrupt generator (source 30) and 31-pin sizing.
5. PCR `RST_EN` enforcement and a clock-tree/frequency model.
6. C6 UART register tail (SLEEP_CONF/CLK_CONF/DATE/ID).
