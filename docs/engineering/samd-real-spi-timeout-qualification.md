# Prospective actual clock-gated SPI timeout

This fixture separately selects MCLK mode 10 and GCLK mode 11 from immutable
runtime `d7a5f5a4e2607d1b5c19a3a6168bb0d286efc92a`, with the same reviewed core,
explicit patches and source/toolchain provenance gates as the landed SPI
checkpoint. Positive mode 8, cancellation mode 9 and timeout modes 10/11 must
all execute under both scheduler profiles on this same corrected source.

The original candidate `6f71d879fa3add9bfb8013cb71754b5d597f7186` failed
[all four actual timeout cases](https://github.com/CrispStrobe/labwired-core/actions/runs/38554696298).
Each accepted two bytes with no wire bytes and writeback remaining 30. The
typed timeout entered exception 15 with PRIMASK zero, but restored-clock reads
found SPI still enabled (`0x2000e`); assertion `0xe0000cb5` stopped execution
before callback return or recovery. Those original failures remain evidence.

The repair disables DMA and releases GPIO CS before temporarily restoring the
exclusive SERCOM4 clocks to disable SPI and clear its flags. It then restores
the exact prior clock gates and entry interrupt mask before notification.
DMAC remains clocked in this profile. Source review is not guest qualification;
all eight affected cases and every enabled final-head check remain required.

The real worker gates an active transfer, then atomically snapshots writeback
and clock witnesses. DWT/SysTick, DMA and GPIO remain clocked. The actual Timer
must deliver one typed timeout in exception 15 with PRIMASK zero through its
immediate listener. Gated DATA/wire/writeback progress must freeze. The caller
restores the actual gate and captures SPI ENABLE/INTEN, DMA enable and CS before
recovery initialization. Reads returning zero while gated do not prove cleanup.

Preserve GPIO CS release separately from the modeled slave's clock-dependent CS
notification, all accepted/wire bytes and raw selected-clock writes. The slave
notification may be deferred until a clocked tick; it is not the GPIO write.
Then require actual callback return, Event/main resume, wait-begin/end counter
and DWT witnesses beyond the original one-shot deadline, no duplicate terminal
delivery and the unchanged 28-row positive async/blocking recovery contract.
Counter wait must be at least 1500 modeled microseconds; the 120-cycle/us
counter's quantization is explicit and is not physical timing calibration.

Raw startup/vector/data/BSS, image/map/source, IRQ and assembly-switch evidence
remain mandatory within the unchanged four-million-step bound. Freeze a new
independent reader and nonvacuous missing/premature/corrupt controls before
original acquisition. Preserve all four failed originals if any case fails;
the workflow reports failure, not a synthesized passing diagnostic. Any shared
driver repair needs affected positive, cancellation and timeout cases on the
same corrected source, plus every enabled exact-head check before normal merge.

See [runtime contract](https://github.com/CrispStrobe/codal-samd/blob/feat/simulation-codal-spi-timeout-20261014/docs/REAL-SPI-TIMEOUT-QUALIFICATION.md)
and [landed cancellation boundary](samd-real-spi-cancel-qualification.md).
Error, stale IRQ, blocking-result ownership, full board/native Arcade, installed
consumer, RTx and distribution/licence admission remain separate requirements.
No consumer pin, performance threshold, acknowledgement or physical capture
changes are authorized by this fixture.
