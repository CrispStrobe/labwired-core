//! ESP32-C6 Tier-1 fixture firmware.
//!
//! Validates the simulator's ESP32-C6 chip model peripheral-by-peripheral with
//! RAW REGISTER accesses and reports one line per peripheral class over UART0
//! using the TIER1 protocol:
//!
//! ```text
//! TIER1 <class> PASS
//! TIER1 <class> FAIL code=<reason>
//! TIER1 done
//! ```
//!
//! The `uart` class is implicit: receiving `TIER1 done` over UART0 proves UART.
//!
//! # Peripheral coverage against esp32c6.yaml
//!
//! | YAML id           | CLASS_MARKER  | class  | status  |
//! |-------------------|---------------|--------|---------|
//! | uart0             | uart          | uart   | implicit PASS via `done` |
//! | gpio              | gpio          | gpio   | PASS — OUT/ENABLE W1TS+W1TC side effects, matrix selectors, IN ≠ OUT |
//! | interrupt_core0   | interrupt     | irq    | PASS — a real CPU trap from CPU_INTR_FROM_CPU_0 through the matrix |
//! | pcr               | pcr           | clock  | PASS — PCR register round-trip + CLK_EN=0 really kills UART0 MMIO |
//! | timg0             | timg          | timer  | PASS — T0UPDATE latches an advancing counter; disabled T0 frozen |
//! | gdma              | gdma          | dma    | PASS — descriptor-driven mem→mem copy + status flags + owner writeback |
//!
//! Everything else the C6 exposes (I2C, SPI, ADC, PWM/LEDC, RTC, WDT, SYSTIMER,
//! TWAI, USB-Serial/JTAG, radios, LP core) is deliberately NOT declared in
//! `configs/chips/esp32c6.yaml`, so the harness renders those classes `na`;
//! this fixture makes no claim there.
//!
//! # clock: what the PCR check proves — and does not
//!
//! `pcr` is a native register file (the full SVD PCR map) and is the clock
//! controller the chip yaml's `clock:` gates resolve through. The check
//! (1) round-trips `UART0_SCLK_CONF` and `SYSCLK_CONF` (real clock-source
//! registers, reset 0x2800_0200 for SYSCLK_CONF), and (2) closes
//! `UART0_CONF.CLK_EN`, observes that a UART0 register read returns 0 and
//! that a write while gated is DROPPED (the pre-gate value survives), then
//! reopens the gate and observes the register live again. `RST_EN` is
//! recorded but not enforced — the fixture does not claim reset semantics.
//!
//! # timer: what the TIMG check proves — and does not
//!
//! The C6 TIMG0 is at 0x6000_8000 and reuses the shared `esp32_timg` model.
//! The check writes `T0CONFIG` (EN|INCREASE, divider 1), latches `T0LO/T0HI`
//! twice around a bounded spin and requires the value to advance; it then
//! clears `EN`, latches twice more and requires the counter to be frozen.
//! Peripherals are clocked at reset (`TIMERGROUP0_CONF.CLK_EN` = 1); the
//! check does not claim alarm/IRQ delivery or the watchdog.
//!
//! # dma: what the GDMA check proves — and does not
//!
//! The C6 GDMA is at 0x6008_0000 (`esp32c6_gdma`, 3 channels, C6 register
//! layout). The check builds a real in-RAM linked-list pair (TX descriptor →
//! source buffer, RX descriptor → destination buffer), sets
//! `IN_CONF0.MEM_TRANS_EN`, binds both PERI_SELs to Dummy-1 (TRM §3.4.3),
//! starts both links, polls `IN_INT_RAW` for `IN_SUC_EOF`, then verifies the
//! bytes actually landed in the destination, the DONE/TOTAL_EOF flags
//! latched, and the owner bits were returned to the CPU (`OUT_AUTO_WRBACK`).
//! It does NOT claim peripheral-coupled DMA (SPI/UART/I2S/AES/...): that
//! datapath is unimplemented and stalls visibly in the model.
//!
//! # irq: real interrupt delivery, not a register round-trip
//!
//! The C6 splits the interrupt fabric across two blocks: source MAP registers in
//! `INTERRUPT_CORE0` @0x6001_0000 (SVD offsets src*4) and the
//! enable/priority/threshold gates plus the `CPU_INTR_FROM_CPU_n` doorbells in
//! `INTPRI` @0x600C_5000. The engine routes asserted sources through those
//! gates into the RISC-V core's external lines
//! (`crates/core/src/bus/routing.rs`, C6 INTPRI layout arm).
//!
//! This check proves the full path end to end: it installs its own `mtvec`
//! trap entry, maps doorbell source 22 to CPU line 9, enables line 9 with a
//! passing priority, sets `mstatus.MIE`, rings the doorbell, and requires the
//! handler to RUN with `mcause = 0x8000_0009` (machine external interrupt on
//! line 9) and to acknowledge the doorbell. It then proves the enable gate is
//! real by disabling line 9 and ringing again without a second trap. A
//! declarative register file cannot produce a trap.
//!
//! Register offsets follow the ESP32-C6 SVD (`tests/fixtures/real_world/
//! esp32c6.svd`) and are cross-checked against the simulator's declarative
//! models in `configs/peripherals/esp32c6/`.

#![no_std]
#![no_main]

use panic_halt as _;
use riscv_rt::entry;

// ── Peripheral base addresses (esp-idf v5.3 reg_base.h) ───────────────────
const UART0_BASE: u32 = 0x6000_0000;
const INTERRUPT_CORE0_BASE: u32 = 0x6001_0000;
const TIMG0_BASE: u32 = 0x6000_8000;
const GDMA_BASE: u32 = 0x6008_0000;
const PCR_BASE: u32 = 0x6009_6000;
const GPIO_BASE: u32 = 0x6009_1000;
const INTPRI_BASE: u32 = 0x600C_5000;

/// `CPU_INTR_FROM_CPU_0` matrix source (`ETS_FROM_CPU_INTR0_SOURCE`); the C3
/// numbers the same doorbell 50. `MAP` word @ INTERRUPT_CORE0 + 22*4.
const SOURCE_FROM_CPU_0: u32 = 22;
/// `CPU_INTR_FROM_CPU_0` doorbell register inside INTPRI.
const CPU_INTR_FROM_CPU_0: u32 = INTPRI_BASE + 0x90;
/// CPU interrupt line the doorbell is routed to (1..31).
const IRQ_LINE: u32 = 9;
/// `mcause` for a machine external interrupt (`0x8000_0000 | line`).
const EXPECT_CAUSE: u32 = 0x8000_0000 | IRQ_LINE;

#[inline(always)]
fn rd32(addr: u32) -> u32 {
    unsafe { core::ptr::read_volatile(addr as *const u32) }
}

#[inline(always)]
fn wr32(addr: u32, value: u32) {
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

// ── UART0 raw byte output ─────────────────────────────────────────────────
//
// UART0 (0x6000_0000): FIFO @ 0x00 (32-bit write pushes the low byte), STATUS
// @ 0x1C with TXFIFO_CNT in bits [25:16]. The C6 binds the shared Espressif
// UART twin: a 128-entry TX FIFO that drops a byte written while full, so poll
// for space exactly as driver firmware does (same shape as the C3 fixture).
fn uart0_write_byte(byte: u8) {
    const FIFO: u32 = UART0_BASE;
    const STATUS: u32 = UART0_BASE + 0x1C;
    const TXFIFO_LEN: u32 = 128; // SOC_UART_FIFO_LEN

    for _ in 0..1_000_000 {
        if ((rd32(STATUS) >> 16) & 0x3FF) < TXFIFO_LEN {
            break;
        }
    }
    // On timeout, write anyway; a garbled line beats a hung fixture.
    wr32(FIFO, byte as u32);
}

fn uart0_write_str(s: &str) {
    for b in s.as_bytes() {
        uart0_write_byte(*b);
    }
}

fn uart0_write_line(s: &str) {
    uart0_write_str(s);
    uart0_write_str("\r\n");
}

fn report(class: &str, result: Result<(), &'static str>) {
    uart0_write_str("TIER1 ");
    uart0_write_str(class);
    match result {
        Ok(()) => uart0_write_line(" PASS"),
        Err(code) => {
            uart0_write_str(" FAIL code=");
            uart0_write_line(code);
        }
    }
}

// ── gpio: output enable + W1TS/W1TC side effects + matrix words ────────────
//
// configs/chips/esp32c6.yaml wires `gpio` (type esp32c6_gpio) to the C3 GPIO
// model (offset-identical register head): OUT @0x04, OUT_W1TS @0x08,
// OUT_W1TC @0x0C, ENABLE @0x20 (+W1TS 0x24/W1TC 0x28), IN @0x3C,
// FUNC0_IN_SEL_CFG @0x154, FUNC0_OUT_SEL_CFG @0x554. Pins used are GPIO4/GPIO5
// — inside the modelled 26-pin block and not boot straps.
//
// What is proven: the set/clear aliases are NOT independent storage on this
// model — OUT_W1TS/OUT_W1TC really mutate the OUT latch and ENABLE_W1TS/W1TC
// really mutate ENABLE, read back through OUT/ENABLE. The pad-selector words
// of both matrices retain what firmware writes. IN is the pad-input word, not
// the output latch: with no external drive on these pins it stays clear while
// OUT drives ones.
//
// What is NOT claimed: IO_MUX pad routing is declarative on the C6 (the yaml
// says so), so this does not prove electrical pad behaviour.
fn check_gpio() -> Result<(), &'static str> {
    const OUT: u32 = GPIO_BASE + 0x04;
    const OUT_W1TS: u32 = GPIO_BASE + 0x08;
    const OUT_W1TC: u32 = GPIO_BASE + 0x0C;
    const ENABLE: u32 = GPIO_BASE + 0x20;
    const ENABLE_W1TS: u32 = GPIO_BASE + 0x24;
    const ENABLE_W1TC: u32 = GPIO_BASE + 0x28;
    const IN: u32 = GPIO_BASE + 0x3C;
    const FUNC0_IN_SEL_CFG: u32 = GPIO_BASE + 0x154;
    const FUNC0_OUT_SEL_CFG: u32 = GPIO_BASE + 0x554;
    const P4: u32 = 1 << 4;
    const P5: u32 = 1 << 5;
    // SIG_GPIO_OUT — pad driven by the plain GPIO_OUT latch.
    const SIG_GPIO_OUT: u32 = 128;
    // C6 matrix output signal index used by the UART0 transmitter
    // (esp-idf v5.3 `gpio_sig_map.h` for esp32c6: `U0TXD_OUT_IDX`).
    const SIG_U0TXD: u32 = 6;

    // ENABLE: plain store, then the W1TS/W1TC aliases must set/clear bits.
    wr32(ENABLE, P4);
    if rd32(ENABLE) & P4 == 0 {
        return Err("gpio-enable-store");
    }
    wr32(ENABLE_W1TS, P5);
    if rd32(ENABLE) & (P4 | P5) != (P4 | P5) {
        return Err("gpio-enable-w1ts");
    }
    wr32(ENABLE_W1TC, P4);
    if rd32(ENABLE) & (P4 | P5) != P5 {
        return Err("gpio-enable-w1tc");
    }

    // OUT: plain store, then W1TS set / W1TC clear read back through OUT.
    wr32(OUT, P4);
    if rd32(OUT) & P4 == 0 {
        return Err("gpio-out-store");
    }
    wr32(OUT_W1TS, P5);
    if rd32(OUT) & (P4 | P5) != (P4 | P5) {
        return Err("gpio-out-w1ts");
    }
    wr32(OUT_W1TC, P4);
    if rd32(OUT) & (P4 | P5) != P5 {
        return Err("gpio-out-w1tc");
    }

    // GPIO matrix: the output selector for pad 4 (FUNC4_OUT_SEL_CFG) and one
    // input-matrix word (FUNC6_IN_SEL_CFG) must round-trip.
    wr32(FUNC0_OUT_SEL_CFG + 4 * 4, SIG_GPIO_OUT);
    if rd32(FUNC0_OUT_SEL_CFG + 4 * 4) & 0x1FF != SIG_GPIO_OUT {
        return Err("gpio-matrix-out-sel");
    }
    wr32(FUNC0_OUT_SEL_CFG + 4 * 4, SIG_U0TXD);
    if rd32(FUNC0_OUT_SEL_CFG + 4 * 4) & 0x1FF != SIG_U0TXD {
        return Err("gpio-matrix-out-sel-rewrite");
    }
    // Input selector: MATRIX_INPUT_SELECT (bit 6) | source pad 3.
    let in_sel_word: u32 = (1 << 6) | 3;
    wr32(FUNC0_IN_SEL_CFG + 6 * 4, in_sel_word);
    if rd32(FUNC0_IN_SEL_CFG + 6 * 4) != in_sel_word {
        return Err("gpio-matrix-in-sel");
    }

    // IN must NOT follow the OUT latch. No external device drives GPIO4/5 on
    // the C6 system, and the C6 IO_MUX is a declarative stub (no pull-up
    // electrical model), so the released input word is clear even while the
    // output drivers are enabled and the latch is high.
    wr32(ENABLE, P4 | P5);
    wr32(OUT, P4 | P5);
    if rd32(IN) & (P4 | P5) != 0 {
        return Err("gpio-in-follows-out");
    }

    // Leave the block quiet for the rest of the run.
    wr32(OUT, 0);
    wr32(ENABLE, 0);
    Ok(())
}

// ── irq: a real trap delivered through INTERRUPT_CORE0 + INTPRI ────────────
//
// ESP32-C6 matrix layout (crates/core/src/bus/routing.rs, C6 arm):
//   * source MAP: INTERRUPT_CORE0 @0x6001_0000 + source*4, low 5 bits = line;
//   * CPU_INT_ENABLE @ INTPRI+0x00, CPU_INT_PRI_n @ INTPRI+0x0C+n*4,
//     CPU_INT_THRESH @ INTPRI+0x8C;
//   * CPU_INTR_FROM_CPU_0 doorbell @ INTPRI+0x90 (matrix source 22).
// The RISC-V core takes a machine external interrupt at the end of the store
// instruction whose write choke re-routes the asserted source (or at the next
// peripheral tick), landing on `mtvec` with `mcause = 0x8000_0000 | line`.

/// Trap handler state, touched by the assembly entry through `trap_dispatch`.
static mut TRAP_COUNT: u32 = 0;
static mut TRAP_CAUSE: u32 = 0;

core::arch::global_asm!(
    r#"
    .section .text.trap_entry
    .align 4
    .global trap_entry
trap_entry:
    addi sp, sp, -128
    sw ra,   0(sp)
    sw t0,   4(sp)
    sw t1,   8(sp)
    sw t2,  12(sp)
    sw t3,  16(sp)
    sw t4,  20(sp)
    sw t5,  24(sp)
    sw t6,  28(sp)
    sw a0,  32(sp)
    sw a1,  36(sp)
    sw a2,  40(sp)
    sw a3,  44(sp)
    sw a4,  48(sp)
    sw a5,  52(sp)
    sw a6,  56(sp)
    sw a7,  60(sp)
    call trap_dispatch
    lw ra,   0(sp)
    lw t0,   4(sp)
    lw t1,   8(sp)
    lw t2,  12(sp)
    lw t3,  16(sp)
    lw t4,  20(sp)
    lw t5,  24(sp)
    lw t6,  28(sp)
    lw a0,  32(sp)
    lw a1,  36(sp)
    lw a2,  40(sp)
    lw a3,  44(sp)
    lw a4,  48(sp)
    lw a5,  52(sp)
    lw a6,  56(sp)
    lw a7,  60(sp)
    addi sp, sp, 128
    mret
"#
);

extern "C" {
    fn trap_entry();
}

/// Rust body of the trap entry. Records why the trap happened and acknowledges
/// the doorbell so the level de-asserts.
#[no_mangle]
pub extern "C" fn trap_dispatch() {
    let cause: u32;
    unsafe { core::arch::asm!("csrr {}, mcause", out(reg) cause) };
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(TRAP_CAUSE), cause);
        let count = core::ptr::read_volatile(core::ptr::addr_of!(TRAP_COUNT));
        core::ptr::write_volatile(core::ptr::addr_of_mut!(TRAP_COUNT), count.wrapping_add(1));
    }
    // CPU_INTR_FROM_CPU_0 is a level bit: writing 0 clears the pending source.
    wr32(CPU_INTR_FROM_CPU_0, 0);
}

#[inline(always)]
fn mstatus_mie_enable() {
    unsafe { core::arch::asm!("csrs mstatus, {0}", in(reg) 8u32) };
}

#[inline(always)]
fn mstatus_mie_disable() {
    unsafe { core::arch::asm!("csrc mstatus, {0}", in(reg) 8u32) };
}

fn check_irq() -> Result<(), &'static str> {
    const MAP_FROM_CPU0: u32 = INTERRUPT_CORE0_BASE + SOURCE_FROM_CPU_0 * 4;
    const CPU_INT_ENABLE: u32 = INTPRI_BASE + 0x00;
    const CPU_INT_PRI_BASE: u32 = INTPRI_BASE + 0x0C;
    const CPU_INT_THRESH: u32 = INTPRI_BASE + 0x8C;

    // The MAP word itself must be a real register first (the C3-tier check
    // still holds on the C6): write, read back, overwrite, read back.
    wr32(MAP_FROM_CPU0, IRQ_LINE);
    if rd32(MAP_FROM_CPU0) & 0x1F != IRQ_LINE {
        return Err("intmatrix-map-readback");
    }
    wr32(MAP_FROM_CPU0, 5);
    if rd32(MAP_FROM_CPU0) & 0x1F != 5 {
        return Err("intmatrix-map-rewrite");
    }
    wr32(MAP_FROM_CPU0, IRQ_LINE);

    // Install our trap entry and program the C6 gates: line 9 enabled with a
    // priority above the reset threshold.
    unsafe {
        core::ptr::write_volatile(core::ptr::addr_of_mut!(TRAP_COUNT), 0);
        core::ptr::write_volatile(core::ptr::addr_of_mut!(TRAP_CAUSE), 0);
        core::arch::asm!("csrw mtvec, {0}", in(reg) trap_entry as *const () as usize);
    }
    wr32(CPU_INT_ENABLE, 1 << IRQ_LINE);
    wr32(CPU_INT_PRI_BASE + IRQ_LINE * 4, 1);
    wr32(CPU_INT_THRESH, 0);

    if unsafe { core::ptr::read_volatile(core::ptr::addr_of!(TRAP_COUNT)) } != 0 {
        return Err("irq-trap-early");
    }

    // Ring the doorbell with machine interrupts enabled. The write choke
    // recomputes the routed line on the store, so the trap lands immediately;
    // the bounded poll tolerates a tick-quantised delivery.
    mstatus_mie_enable();
    wr32(CPU_INTR_FROM_CPU_0, 1);
    let mut delivered = false;
    for _ in 0..1_000_000 {
        if unsafe { core::ptr::read_volatile(core::ptr::addr_of!(TRAP_COUNT)) } != 0 {
            delivered = true;
            break;
        }
    }
    mstatus_mie_disable();
    wr32(CPU_INTR_FROM_CPU_0, 0);

    if !delivered {
        return Err("irq-not-delivered");
    }
    if unsafe { core::ptr::read_volatile(core::ptr::addr_of!(TRAP_COUNT)) } != 1 {
        return Err("irq-trap-count");
    }
    if unsafe { core::ptr::read_volatile(core::ptr::addr_of!(TRAP_CAUSE)) } != EXPECT_CAUSE {
        return Err("irq-trap-cause");
    }

    // The enable gate is real: with line 9 disabled the same doorbell must NOT
    // trap. (This also proves the first trap's doorbell was acknowledged.)
    wr32(CPU_INT_ENABLE, 0);
    mstatus_mie_enable();
    wr32(CPU_INTR_FROM_CPU_0, 1);
    for i in 0u32..20_000 {
        core::hint::black_box(i);
    }
    let traps_while_masked = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(TRAP_COUNT)) };
    wr32(CPU_INTR_FROM_CPU_0, 0);
    mstatus_mie_disable();
    if traps_while_masked != 1 {
        return Err("irq-enable-gate");
    }

    // Leave the matrix quiet for the rest of the run.
    wr32(CPU_INT_ENABLE, 0);
    wr32(MAP_FROM_CPU0, 0);
    Ok(())
}

// ── clock: PCR register round-trip + enforced UART0 clock gate ─────────────
//
// configs/chips/esp32c6.yaml declares `pcr` as `esp32c6_pcr` — a native
// register file (full SVD map) that also serves `clock_gate_reg_offset` for
// the chip yaml's `clock:` gates. The bus gate
// (crates/core/src/bus/device_hooks.rs::is_peripheral_clocked) reads the LIVE
// PCR register at every UART0 access: with CLK_EN clear, reads return 0 and
// writes are dropped; with it set, the console UART behaves normally.
//
// What is proven:
//   * UART0_SCLK_CONF (a clock-source/divider register) round-trips a value
//     that is not its reset, and SYSCLK_CONF reads its SVD reset value and
//     round-trips a write — PCR is a real register file, not read-as-zero.
//   * UART0_CONF.CLK_EN=0 makes a stored UART0 register read 0 (dead), and a
//     write issued while gated is DROPPED: after reopening the gate the old
//     value is still there. Then a normal read works again.
//
// What is NOT claimed: RST_EN semantics (recorded, not enforced), clock
// divisors/frequencies, and the SYSCLK mux. No silicon diff — the observed
// gating contract comes from the model + the SVD register map.
fn check_clock() -> Result<(), &'static str> {
    const UART0_CONF: u32 = PCR_BASE + 0x00;
    const UART0_SCLK_CONF: u32 = PCR_BASE + 0x04;
    const SYSCLK_CONF: u32 = PCR_BASE + 0x110;
    const CLK_EN: u32 = 1 << 0;

    // UART0_SCLK_CONF: SVD reset 0x0070_0000; write something else and read it.
    if rd32(UART0_SCLK_CONF) != 0x0070_0000 {
        return Err("pcr-sclk-reset-value");
    }
    wr32(UART0_SCLK_CONF, 0x0050_1234);
    if rd32(UART0_SCLK_CONF) != 0x0050_1234 {
        return Err("pcr-sclk-roundtrip");
    }
    wr32(UART0_SCLK_CONF, 0x0070_0000);

    // SYSCLK_CONF: SVD reset 0x2800_0200 (LS_DIV/HS_DIV); round-trip a change.
    if rd32(SYSCLK_CONF) != 0x2800_0200 {
        return Err("pcr-sysclk-reset-value");
    }
    wr32(SYSCLK_CONF, 0x2800_0202);
    if rd32(SYSCLK_CONF) != 0x2800_0202 {
        return Err("pcr-sysclk-roundtrip");
    }
    wr32(SYSCLK_CONF, 0x2800_0200);

    // Gate proof. CLK_EN is 1 at reset (the console runs), RST_EN is 0.
    if rd32(UART0_CONF) & CLK_EN == 0 {
        return Err("pcr-uart0-ungated-at-reset");
    }
    const UART0_CLKDIV: u32 = UART0_BASE + 0x14;
    const UART0_STATUS: u32 = UART0_BASE + 0x1C;

    // The gate experiment is a closure whose result is returned only AFTER
    // the gate has been reopened: a failure return while UART0 is dead would
    // hang the transcript instead of reporting it.
    let gate_probe = || -> Result<(), &'static str> {
        wr32(UART0_CLKDIV, 0x234);
        if rd32(UART0_CLKDIV) != 0x234 {
            return Err("clock-probe-store");
        }

        // Close the gate (CLK_EN=0): every UART0 register access is dead.
        wr32(UART0_CONF, 0);
        if rd32(UART0_CLKDIV) != 0 {
            return Err("clock-gated-read-not-dead");
        }
        if rd32(UART0_STATUS) != 0 {
            return Err("clock-gated-status-not-dead");
        }
        // A gated write must be dropped, not queued.
        wr32(UART0_CLKDIV, 0x0BAD_0BAD);

        // Reopen the gate (restore the SVD reset value): the peripheral
        // answers again, and the gated write is GONE — state, not a delayed
        // store.
        wr32(UART0_CONF, 1);
        if rd32(UART0_CLKDIV) != 0x234 {
            return Err("clock-gated-write-not-dropped");
        }
        if (rd32(UART0_STATUS) >> 16) & 0x3FF >= 128 {
            return Err("clock-ungated-status-dead");
        }
        Ok(())
    };
    let result = gate_probe();
    wr32(UART0_CONF, 1); // unconditional reopen before reporting
    result
}

// ── timer: timg0 counter advance + EN gate ─────────────────────────────────
//
// configs/chips/esp32c6.yaml wires timg0 at 0x6000_8000 as the shared
// `esp32_timg` model (register head offset-identical to the C3's; the C6
// carries one general-purpose timer per group, and the model's T0 head is the
// part this check drives). T0CONFIG.EN (bit 31) gates the live counter;
// T0UPDATE latches the 64-bit value into T0LO/T0HI.
//
// What is proven: with EN set the latched value advances across a bounded
// spin, and with EN cleared the latched value is frozen. A register file
// cannot produce a counter that both runs and stops.
//
// What is NOT claimed: real-time rate (the model ticks 1 µs per bus tick,
// not at the programmed divider), alarm interrupts, or the watchdog.
fn check_timer() -> Result<(), &'static str> {
    const T0CONFIG: u32 = TIMG0_BASE + 0x00;
    const T0LO: u32 = TIMG0_BASE + 0x04;
    const T0HI: u32 = TIMG0_BASE + 0x08;
    const T0UPDATE: u32 = TIMG0_BASE + 0x0C;
    const EN: u32 = 1 << 31;
    const INCREASE: u32 = 1 << 30;
    const DIVIDER_1: u32 = 1 << 13;

    // T0CONFIG is writable and the EN bit stores.
    wr32(T0CONFIG, EN | INCREASE | DIVIDER_1);
    if rd32(T0CONFIG) & EN == 0 {
        return Err("timg0-config-store");
    }

    let latch = || -> u64 {
        wr32(T0UPDATE, 1);
        ((rd32(T0HI) as u64) << 32) | rd32(T0LO) as u64
    };

    let a = latch();
    for i in 0u32..20_000 {
        core::hint::black_box(i);
    }
    let b = latch();
    if b <= a {
        return Err("timg0-not-counting");
    }

    // Clear EN: the counter must stop. (A second latch pair must be equal.)
    wr32(T0CONFIG, INCREASE | DIVIDER_1);
    let c = latch();
    for i in 0u32..20_000 {
        core::hint::black_box(i);
    }
    let d = latch();
    if d != c {
        return Err("timg0-counts-when-disabled");
    }
    Ok(())
}

// ── dma: GDMA mem→mem descriptor transfer ──────────────────────────────────
//
// esp32c6.yaml declares `gdma` (0x6008_0000, `esp32c6_gdma`, 3 channels).
// C6 register layout (SVD): per-channel IN block @0x70+n*0xC0 (IN_CONF0
// +0x00, IN_LINK +0x10, IN_PERI_SEL +0x30), OUT registers from 0xD4+n*0xC0
// (OUT_LINK +0x0C, OUT_PERI_SEL +0x2C) and OUT_CONF0 @0x190+n*0xC0; flat
// INT_* registers 0x00/0x30 + n*0x10. INLINK/OUTLINK carry the low 20 bits of
// the descriptor address; buffers and descriptors live in HP SRAM at
// 0x4080_0000.
//
// What is proven: a real linked-list copy. The fixture arms MEM_TRANS_EN and
// a Dummy-1 PERI_SEL on both directions (TRM §3.4.3), starts both links, and
// polls IN_INT_RAW. Completion requires the bytes to have actually landed in
// the destination buffer, the IN/OUT DONE+EOF flags, and the owner bits
// written back to the CPU (OUT_AUTO_WRBACK for TX; RX writeback always).
//
// What is NOT claimed: peripheral-coupled DMA (no pump exists in the model;
// such a start stalls visibly), FIFO data ports, priority arbitration, and
// interrupt delivery of the DMA sources.
#[repr(C)]
struct DmaDescriptor {
    dw0: u32,
    buffer: u32,
    next: u32,
}

static mut DMA_SRC: [u8; 16] = *b"TIER1-GDMA-M2M!\0";
static mut DMA_DST: [u8; 16] = [0u8; 16];
static mut DMA_TX_DESC: DmaDescriptor = DmaDescriptor {
    dw0: 0,
    buffer: 0,
    next: 0,
};
static mut DMA_RX_DESC: DmaDescriptor = DmaDescriptor {
    dw0: 0,
    buffer: 0,
    next: 0,
};

fn check_dma() -> Result<(), &'static str> {
    const IN_CONF0: u32 = GDMA_BASE + 0x70;
    const IN_INT_RAW: u32 = GDMA_BASE + 0x00;
    const IN_INT_CLR: u32 = GDMA_BASE + 0x0C;
    const IN_LINK: u32 = GDMA_BASE + 0x80;
    const IN_PERI_SEL: u32 = GDMA_BASE + 0xA0;
    const OUT_INT_RAW: u32 = GDMA_BASE + 0x30;
    const OUT_INT_CLR: u32 = GDMA_BASE + 0x3C;
    const OUT_LINK: u32 = GDMA_BASE + 0xE0;
    const OUT_PERI_SEL: u32 = GDMA_BASE + 0x100;
    const OUT_CONF0: u32 = GDMA_BASE + 0x190;
    const MEM_TRANS_EN: u32 = 1 << 4;
    const OUT_AUTO_WRBACK: u32 = 1 << 2;
    const IN_DONE: u32 = 1 << 0;
    const IN_SUC_EOF: u32 = 1 << 1;
    const OUT_DONE: u32 = 1 << 0;
    const OUT_TOTAL_EOF: u32 = 1 << 3;
    const INLINK_START: u32 = 1 << 22;
    const OUTLINK_START: u32 = 1 << 21;
    const LINK_ADDR_MASK: u32 = 0x000F_FFFF;

    // Config round-trips + M2M binding (both PERI_SELs to the same Dummy).
    wr32(IN_PERI_SEL, 1);
    wr32(OUT_PERI_SEL, 1);
    wr32(IN_CONF0, MEM_TRANS_EN);
    if rd32(IN_CONF0) & MEM_TRANS_EN == 0 {
        return Err("gdma-conf-roundtrip");
    }
    wr32(OUT_CONF0, 0x8 | OUT_AUTO_WRBACK); // OUT_EOF_MODE reset | writeback

    let (src_addr, dst_addr, tx_desc_addr, rx_desc_addr, len) = unsafe {
        let src = &raw mut DMA_SRC;
        let dst = &raw mut DMA_DST;
        let len = (*src).len() as u32;
        // owner=DMA(1) | suc_eof | length=len | size=len
        DMA_TX_DESC.dw0 = (1 << 31) | (1 << 30) | (len << 12) | len;
        DMA_TX_DESC.buffer = src as u32;
        DMA_TX_DESC.next = 0;
        DMA_RX_DESC.dw0 = (1 << 31) | len; // owner=DMA, size=len
        DMA_RX_DESC.buffer = dst as u32;
        DMA_RX_DESC.next = 0;
        (
            src as u32,
            dst as u32,
            (&raw const DMA_TX_DESC) as u32,
            (&raw const DMA_RX_DESC) as u32,
            len,
        )
    };

    wr32(IN_INT_CLR, 0xFFFF_FFFF);
    wr32(OUT_INT_CLR, 0xFFFF_FFFF);
    wr32(IN_LINK, (rx_desc_addr & LINK_ADDR_MASK) | INLINK_START);
    wr32(OUT_LINK, (tx_desc_addr & LINK_ADDR_MASK) | OUTLINK_START);

    let mut eof = false;
    for _ in 0..100_000 {
        if rd32(IN_INT_RAW) & IN_SUC_EOF != 0 {
            eof = true;
            break;
        }
    }
    if !eof {
        return Err("gdma-eof-timeout");
    }
    if rd32(IN_INT_RAW) & IN_DONE == 0 {
        return Err("gdma-in-done");
    }
    if rd32(OUT_INT_RAW) & (OUT_TOTAL_EOF | OUT_DONE) != (OUT_TOTAL_EOF | OUT_DONE) {
        return Err("gdma-out-flags");
    }

    // EOF alone is not a transfer: the bytes must actually have moved. Read
    // through raw pointers so no optimizer can fold this to the initializer.
    let moved = (0..len).all(|i| unsafe {
        core::ptr::read_volatile((src_addr + i) as *const u8)
            == core::ptr::read_volatile((dst_addr + i) as *const u8)
    });
    if !moved {
        return Err("gdma-no-m2m-model");
    }

    // Both descriptors came back to the CPU (owner=0), and the RX length
    // field holds the received byte count.
    let (tx_owner, rx_owner, rx_len) = unsafe {
        (
            (*core::ptr::addr_of!(DMA_TX_DESC)).dw0 & (1 << 31),
            (*core::ptr::addr_of!(DMA_RX_DESC)).dw0 & (1 << 31),
            ((*core::ptr::addr_of!(DMA_RX_DESC)).dw0 >> 12) & 0xFFF,
        )
    };
    if tx_owner != 0 {
        return Err("gdma-tx-owner-writeback");
    }
    if rx_owner != 0 {
        return Err("gdma-rx-owner-writeback");
    }
    if rx_len != len {
        return Err("gdma-rx-length-writeback");
    }
    Ok(())
}

#[entry]
fn main() -> ! {
    // Order: clock first (exercises the PCR gate, reopening it before any
    // console output), then gpio, timer, dma, irq. Classes with no declared
    // peripheral (pwm/i2c/spi/adc/wdt/rtc) are left unreported, never faked.
    report("clock", check_clock());
    report("gpio", check_gpio());
    report("timer", check_timer());
    report("dma", check_dma());
    report("irq", check_irq());
    uart0_write_line("TIER1 done");

    loop {
        core::hint::spin_loop();
    }
}
