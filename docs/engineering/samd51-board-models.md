# SAMD51 board models

Opt-in models for booting a complete SAMD51 board (the no-ASF MakeCode Arcade
base in `CrispStrobe/codal-samd`). Each is selected only by its own type name in
a consumer's descriptor overlay; existing types, chip descriptors and qualified
fixtures are unchanged. Construction goes through one cold builder with fixed
instance addresses and no config overrides.

| Type | Register slice | Not modelled |
| --- | --- | --- |
| `samd51_oscctrl` | Reset-enabled DFLL48M; DPLL0/1 with `DPLLRATIO`, `DPLLCTRLB` (GCLK reference only), `DPLLSYNCBUSY` and `DPLLSTATUS`. A DPLL locks only once its reference channel (`GCLK PCHCTRL[1 + n]`) is enabled. | Crystals, DFLL closed loop, lock time, jitter, interrupts, events (refused). |
| `samd51_gclk` | The existing `SamGclk` model plus SAMD51 `GENCTRL[0..11]` at `0x20 + 4n` (GENCTRL0 resets to DFLL, enabled). Same type, so SERCOM `gclk_id` gating keeps working. | Generator output pins, `GCLK_IN`, chaining. |
| `samd51_tc` | TC0..TC5: 8/16-bit counters with NFRQ/MFRQ/NPWM/MPWM tops, compare-match and overflow flags/IRQs, CTRLB commands and READSYNC. The count rate is `f_TC / f_CPU` resolved from the live clock tree; with no bus clock, channel or running source the counter stops. | Waveform pins, capture, events, 32-bit pairing, DMA (refused or absent). |
| `samd51_cmcc_board` | The CMCC control slice plus `CFG`. `MAINT0.INVALL` is admitted while `CEN=1` only when `CFG` disables both caches. The control-only `samd51_cmcc_control` slice still refuses `CFG`. | Cache contents, tags, hit timing. |

`sam/clock_tree.rs` resolves nominal generator, peripheral-channel and CPU
frequencies from GCLK/OSCCTRL/MCLK registers on the bus. A source that is not
running resolves to `None`, never to a guessed frequency.

## USB `CTRLA.RUNSTDBY`

`samd51_usb_device_banks` admitted `CTRLA` bits `0x43`, placing RUN-IN-STANDBY at
bit 6. Microchip's SAMD51 device description (DFP 3.9.276 ATDF) places
`RUNSTDBY` at bit 2; bit 6 is reserved. The model now admits `0x07` and refuses
bit 6. The USB leaf's independently authored header carried the same error and is
corrected with it; a driver compiled against ATDF-generated headers exposed the
mismatch.

## Limits

Nominal frequencies only. These models make a natural boot possible; behaviour of
the peripherals beyond the slices above is not modelled, and none of this is a
physical or timing qualification.
