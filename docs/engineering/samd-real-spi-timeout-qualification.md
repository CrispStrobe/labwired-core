# Prospective actual clock-gated SPI timeout

This fixture separately selects MCLK mode 10 and GCLK mode 11 from immutable
runtime `6f71d879fa3add9bfb8013cb71754b5d597f7186`, with the same reviewed core,
explicit patches and source/toolchain provenance gates as the landed SPI
checkpoint. Both modes must execute under both scheduler profiles. The first
candidate deliberately retains the existing driver: preserve any actual cleanup
failure before repair, and do not mistake compiler success for guest admission.

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
