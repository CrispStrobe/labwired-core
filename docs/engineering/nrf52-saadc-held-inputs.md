# nRF52 SAADC held-input functional contract

This original MIT slice makes configured SAADC conversions respond to held
analog input levels. It does not qualify micro:bit microphone audio capture.

The register facts and conversion equation follow Nordic's
[SAADC product specification](https://docs.nordicsemi.com/r/bundle/ps_nrf52832/page/saadc.html)
for the shared nRF52 SAADC IP. The nRF52833 instance is SAADC at `0x40007000`,
IRQ7. The [micro:bit Foundation pinmap](https://tech.microbit.org/hardware/schematic/)
assigns `MIC_IN` to P0.05/AIN3 and `RUN_MIC` to P0.20. A microphone fixture must
drive AIN3 through SAADC, rather than the PDM peripheral.

## Input and code mapping

`Peripheral::set_adc_channel_input(ain, millivolts)` holds one physical AIN0..7.
`adc_channel_count()` returns 8. `clear_adc_channel_input(ain)` releases it.
Invalid physical input indices return false without altering an input. The
WASM `set_adc_channel_millivolts("saadc", 3, millivolts)` and
`clear_adc_channel("saadc", 3)` use these generic hooks. The existing STM32 ADC
downcast remains the compatibility fallback.

CH[n] is a converter configuration slot, not an AIN or GPIO index. PSELP/PSELN
values 1..8 select AIN0..7, so any CH[n].PSELP=4 selects the microphone pad.
PSELP=0 disables a slot; invalid selectors are skipped. PSELN is ignored in
single-ended mode. In differential mode NC is modeled as ground. Selector9
uses the explicit modeled 3300mV supply, also used by VDD/4 reference.

The configured gain choices are 1/6, 1/5, 1/4, 1/3, 1/2, 1, 2 and 4.
Reference is 600mV internal or 825mV modeled VDD/4. Integer rational arithmetic
computes `(VP-VN) * gain/reference * 2^(resolution-m)`, where m=0 for
single-ended mode and m=1 for differential mode. Division truncates toward
zero. Single-ended results saturate to 0..2^N-1; differential results saturate
to -2^(N-1)..2^(N-1)-1. DMA stores signed little-endian 16-bit samples.

When any physical input is held, un-driven AINs are explicitly modeled at
ground. Releasing the last input restores the legacy 3000mV fixture source;
selected channels still apply their configured gain/reference. The original
register-only tests configure no CH.PSELP at all: solely when no held input
and no positive selector exists, that old fixture converts its 3000mV source
with default gain/reference. Invalid selectors never enable that fallback.

## Bounded DMA approximation and remaining work

The pre-existing engine performs RESULT.MAXCNT conversions for one SAMPLE
task, visiting enabled CH[n] slots in ascending order and repeating them until
the buffer is filled. MAXCNT is bounded to 32767 samples. A delay-zero scheduler
event or the bare-bus tick drains the conversion. This SAMPLE-to-MAXCNT burst
is a compatibility approximation; actual silicon converts one sample per
enabled channel for each SAMPLE and appends it to the START-latched DMA buffer.
START/STOP enable gating is retained; STOP and disabling the ADC cancel a
pending burst. With live inputs and no enabled slot, no samples or completion
events are fabricated. Existing event latches remain until firmware clears them.

END, DONE and RESULTDONE latch on completed bursts; their enabled bits raise
the level IRQ. Nordic event bits use `(event offset-0x100)/4`, so STARTED=0,
END=1, DONE=2, RESULTDONE=3, CALIBRATEDONE=4 and STOPPED=5. Event clear and
INTENCLR deassert the line. Acquisition time, oversampling/BURST, local timed
sampling, calibration, limit events and resistor networks are not modeled.

Before qualifying microphone capture, implement and prove START-latched
PTR/MAXCNT, per-SAMPLE scan append, END only on buffer completion, sample
timing/PPI/oversampling, RUN_MIC gating and an explicitly bounded waveform
source. Microphone bias, sound-pressure calibration and analog noise require
separate board fixtures. No vendor firmware or analog hardware model is added
by this slice.
