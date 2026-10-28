# SAMD51 board models

Opt-in models for running a complete SAMD51 board (the no-ASF MakeCode Arcade
base in `CrispStrobe/codal-samd`). Each is selected only by its own type name in
a consumer's descriptor overlay; existing types, chip descriptors and qualified
fixtures are unchanged. Construction goes through one cold builder with fixed
instance addresses and no config overrides.

| Type | Register slice | Not modelled |
| --- | --- | --- |
| `samd51_oscctrl` | Reset-enabled DFLL48M; DPLL0/1 with `DPLLRATIO`, `DPLLCTRLB` (GCLK reference only), `DPLLSYNCBUSY` and `DPLLSTATUS`. A DPLL locks only once its reference channel (`GCLK PCHCTRL[1 + n]`) is enabled. DFLL `DFLLCTRLB`, `DFLLVAL`, `DFLLMUL` and `DFLLSYNC`: USB clock recovery (`MODE` with `USBCRM`, `MUL` = 48000 periods per start of frame, DS60001507 28.6.4). | Crystals, DFLL closed loop on a GCLK reference, lock time, jitter, interrupts, events (refused). |
| `samd51_gclk` | The existing `SamGclk` model plus SAMD51 `GENCTRL[0..11]` at `0x20 + 4n` (GENCTRL0 resets to DFLL, enabled). Same type, so SERCOM `gclk_id` gating keeps working. | Generator output pins, `GCLK_IN`, chaining. |
| `samd51_tc` | TC0..TC5: 8/16-bit counters with NFRQ/MFRQ/NPWM/MPWM tops, compare-match and overflow flags/IRQs, CTRLB commands, READSYNC, CCBUF/PERBUF applied at the UPDATE (wrap). The count rate is `f_TC / f_CPU` resolved from the live clock tree. Waveform outputs WO0/WO1 for up-counting NPWM and MPWM WO1 are published edge-exact onto pads routed to PMUX function E (`DRVCTRL.INVEN` inverts). Each wrap raises one overflow DMA request on its `TCn_DMAC_ID_OVF` source. | NFRQ/MFRQ and MPWM WO0 outputs, down-counting outputs (drive low), capture, events, 32-bit pairing. |
| `sam_sercom_spi` (extended) | Optional `sck_port_base`/`mosi_port_base` (pads on different PORT groups), a MISO pad (`miso_port_base`, `miso_bit`, `dipo`): received bytes come from the devices only while it is muxed and selected by DIPO, otherwise the input reads the idle line; `dma_tx_trigger`/`dma_rx_trigger` declare the SERCOM's DMAC sources. The existing `port_base` form is unchanged. With `gclk_id` resolving on the live clock tree, a byte takes 16 * (BAUD + 1) core clocks at `f_CPU / f_GCLK`, the fraction carried across back-to-back bytes; otherwise nominal core-clock cycles. | Pad and edge timing, slave mode. |
| `samd51_cmcc_board` | The CMCC control slice plus `CFG`. `MAINT0.INVALL` is admitted while `CEN=1` only when `CFG` disables both caches. | Cache contents, tags, hit timing. |
| `samd51_eic` | Sixteen channels, rise/fall/both/high/low sensing, flags and IRQ lines 12..27. A pad edge reaches channel n only while the pad is muxed to the EIC (PMUXEN, function A) and the EIC is bus- and generic-clocked and enabled. PINSTATE follows the delivered level. | Filtering, debouncing, asynchronous timing, NMI, events (refused). |
| `samd51_sercom_i2cm` | SERCOM I2C master: 7-bit addressing, writes, smart-mode reads, repeated start, STOP, MB/SB/RXNACK against attached `I2cDevice` slaves. An operation completes on the next bus tick only while the SERCOM's bus and core clocks run and a PAD0 (SDA) and PAD1 (SCL) pad are muxed to it (ATDF pad table). With the core clock resolving, an operation lasts whole SCL periods of `10 + BAUD + BAUDLOW + f_core * t_rise` core clocks, with the rise time from the optional `scl_rise_ns` (default 0): START plus nine bits per address, nine more for a read's first byte, nine per byte, one per STOP. A device's `I2cDevice::clock_stretch_ns` is added once per byte it acknowledges or sends (twice for a read address, which carries the first byte). With `CTRLA.LOWTOUTEN`, a hold beyond the SCL low time-out (25 ms, the earliest of the datasheet's 25-35 ms) ends the operation there: MB or SB as normal, STOP sent, `STATUS.LOWTOUT` and `BUSERR` with `INTFLAG.ERROR`; a START written during a STOP follows it. An open read stays on the bus tick (a smart-mode DATA read issues the next byte). `busy()` and `completed()` expose operation progress to hosts. | Rise-time waveform, arbitration, the other bus timeouts (MEXTTOEN, SEXTTOEN, INACTOUT), 10-bit/high-speed, DMA, slave mode (refused). |
| `samd51_sercom_usart` | `sam_sercom_usart` with the SAMD51 interrupt layout: DRE, TXC, RXC and the remaining flags on four consecutive NVIC lines, reported as level sources (a DATA read can deassert RXC). Optional `tx_port_base`/`tx_bit`/`rx_port_base`/`rx_bit`/`pad_mux`/`txpo`/`rxpo`: a byte reaches the TX sink, and host bytes reach the receiver, only while that pad is muxed and selected by TXPO/RXPO (latched on the bus tick). With the pads and `gclk_id` declared, bytes are paced at the baud from BAUD and SAMPR (arithmetic and fractional) on the live clock tree: frames of start, data, parity and stop bits, in arithmetic mode a whole number of core clocks per frame (floor + 1, DS60001507 33.6.2.3.1), a DATA holding register before the shift register (DRE, TXC), host bytes a frame apart into a two-byte receive buffer (BUFOVF beyond). On an `event-scheduler` build the SERCOM wakes itself at each frame end while a byte is in flight. | As `sam_sercom_usart`; external-clock and reserved SAMPR settings move no byte when paced. |
| `samd51_dmac_board` | `sam_dmac_*` board profile: per channel, SERCOM TX or RX byte beats against the SERCOM DATA register whose chip description declares the trigger (`dma_tx_trigger`/`dma_rx_trigger`; an endpoint that declares none admits the historical SERCOM4 TX only), or TC3 overflow half-word SRCINC beats into DAC DATA0. Descriptor `EVOSEL`, and `STEPSEL` with `STEPSIZE = X1`, are admitted (no effect). Optional `request_latency_cycles` (default 0): a SERCOM request is serviced that many CPU cycles after the DMAC first sees it, a declared board input for arbitration and the descriptor fetch. | Other triggers, beat sizes, endpoints, linked descriptors (refused). |
| `samd51_dac` | Channel 0 conversions from DATA0 writes while `CTRLA.ENABLE`, `DACCTRL0.ENABLE` and the generic clock (`PCHCTRL[42]`) hold; `STATUS.READY0`. Each conversion is recorded with its engine cycle. With the board's `vrefa_mv` and `vddana_mv` declared, `output_millivolts(code)` is the nominal level DATA / 4095 x VREF for the selected REFSEL. | Gain, offset and linearity error, settling, load, the internal reference, refresh, dithering, channel 1, differential mode, events, interrupts, DATABUF. |
| `samd51_pac` | The access-error flags drivers clear before use (no access errors are modelled: flags read 0). | Write protection, events, the PAC interrupt (refused). |
| `samd51_tcc` | TCC0..TCC4 counters (24-bit TCC0/1, 16-bit TCC2..4) at the prescaled generic clock, PER top, CC0..CCn matches, CTRLB commands, and the SAMD51 lines: line 0 for OVF/TRG/CNT/ERR/faults, line 1 + x for MCx. | Waveforms, CCBUF/PERBUF, dithering, faults, capture, patterns, events, DMA. |
| `samd51_adc` | ADC0/ADC1 single-ended software-triggered conversions of AINn against GND with the VDDANA references (INTVCC1, INTVCC0), 8/10/12-bit, `floor(V * 2^n / Vref)`. A conversion completes on the next bus tick only while the ADC is enabled and clocked and the input's pad is on PMUX B. Input voltages are set by the host (`set_input_millivolts`). | Analog front end, averaging, correction, windowing, differential mode, events, DMA. |

`sam/clock_tree.rs` resolves nominal generator, peripheral-channel and CPU
frequencies from GCLK/OSCCTRL/MCLK registers on the bus. A source that is not
running resolves to `None`, never to a guessed frequency.

## Engine changes these models rely on

- **SAM pads get a `PortId`.** `PinPort::port_name_prefix` lets a port family
  name its ids; SAM PORT groups are `porta`/`portb`. External pad edges now
  reach edge sinks on SAM boards (`gpioX` naming is unchanged).
- **Edge sinks are bus-ticked.** After an external edge reaches a sink, the bus
  refreshes that sink's bus-tick membership, so a sink that defers the edge to
  `tick_with_bus` (the EIC) is serviced.
- **SAM PORT AF routing.** `selected_function` decodes `PINCFG.PMUXEN`/`PMUX`,
  and `bind_timer_output_pad` binds SAM pads on function E; only pads with a
  bound route consult it.
- **`Bus::take_dma_trigger`** consumes one pending request of a DMAC trigger
  source id (a TC answers for its own `TCn_DMAC_ID_OVF`; default: refused).
- **`Peripheral::timer_output_family`** names a timer's output family, so the
  SAMD51 TC pad wiring needs no downcast.
- **`Peripheral::on_system_reset`**, called for every peripheral when the core
  drains `AIRCR.SYSRESETREQ`: registers return to power-on values, as on
  silicon; wiring, attached devices and flash contents survive. Implemented by
  the SAMD51 models, the SAM PORT family of `gpio`, `sam_sercom_spi`,
  `sam_mclk`, `sam_gclk` and `sam_pm` (SAMD21 too), `sam_sercom_usart` (both
  layouts) and the byte DMAC; the PAC holds no state. Shared models (generic
  UART, SysTick beyond CSR, declarative registers) keep their previous
  behaviour (default no-op): changing them drifts hardware-captured boards.
  `Samd51Nvmctrl::committed()` counts committed erase and program operations
  across resets, so a host can cut power between them; `pending_operation()`
  shows the erase or program waiting to commit, from which a host can derive
  its own fault inside one operation. A USB device reset drops the host's
  queued transactions (the device leaves the bus).
- **`Bus::dma_rx_request` / `Bus::dma_triggers_at`** (RX readiness of a DMA
  endpoint; the trigger sources it declares), both refusing by default.

## DMAC `TCMPL` under `BLOCKACT=NOACT`

The byte DMAC slices set `TCMPL` at the end of a transfer only for
`BLOCKACT=INT`. CODAL's `DmaInstance` and Adafruit_ZeroDMA both run a single
unlinked descriptor with `BLOCKACT=NOACT` and complete on the `TCMPL`
interrupt on SAMD21/SAMD51 hardware, so the last block of a transaction sets
`TCMPL` whatever `BLOCKACT` says. Links remain refused, so every block is the
last; the `noact` test modes use CODAL's exact descriptor.

## USB `CTRLA.RUNSTDBY`

`samd51_usb_device_banks` admitted `CTRLA` bits `0x43`, placing RUN-IN-STANDBY at
bit 6. Microchip's SAMD51 device description (DFP 3.9.276 ATDF) places
`RUNSTDBY` at bit 2; bit 6 is reserved. The model admits `0x07` and refuses
bit 6.

## Limits

Nominal frequencies only; bus timing follows registers and nominal clocks. These models make a natural boot and the qualified
peripheral paths possible; behaviour beyond the slices above is not modelled,
and none of this is a physical or timing qualification.
