# Prospective actual SPI writeback AHB fault

The fixture separately selects caller mode 12 from immutable runtime
`709ce0fbfe0c6165c4a22365008b91ea0238f198`. It adds no driver/model repair.
This is a controller-corruption/writeback fault, not an ordinary valid-buffer
request or physical fault. The real guest worker globally disables DMAC before
retargeting enable-protected WBADDR to aligned unmapped `0x60000000`; it keeps
the original valid buffer and live channel descriptor, then resumes global DMA.

The raw prefix must include partial accepted/wire progress, the worker, global
disable, all four WBADDR bytes, global re-enable, one further accepted DATA beat,
actual TERR at IRQ47 entry, CS release, typed failure callback/return, worker
completion and Event/main resume. Preserve every byte and the original RAM
writeback snapshot. Do not treat that old RAM count as post-fault writeback:
the controller now targets the unmapped address. No host flag/IRQ/Event injection
or callback substitution is allowed.

Require exactly one `dma_error` callback in IRQ47/PRIMASK0 with CS released,
strict pre-recovery disabled MASTER `0x2000c`, INTEN0 and DMA TX `0x200d00`, busy
reentry and actual callback return/main resume. Then require the unchanged
28-row positive asynchronous/blocking recovery, two later DMA-completion IRQs
and two SPI-TXC IRQs, with matched total DMA/SPI entry/return counts `[3,2]`.
Only the error IRQ contributes TERR; it must not masquerade as TCMPL or TXC.

The workflow builds fault mode12 and positive mode8 from two separate clean
pinned core trees, runs both scheduler profiles and retains all four originals
even if a case fails. Synthetic deletion/corrupt-byte controls are mandatory;
an independent source/image-bound reader must be frozen before original
acquisition. Source/compiler success is not guest admission. Preserve the first
actual failure before repair, keep the four-million-step bound and all enabled
exact-head checks, and normally merge only after reviewed/tested/landed tree and
parent/ref verification. A shared driver fix needs fresh affected eight-case
success/cancellation/timeout coverage as well as the new error caller.

See the [source caller/acceptance contract](https://github.com/CrispStrobe/codal-samd/blob/test/simulation-codal-spi-error-20261014/docs/REAL-SPI-DMA-ERROR-QUALIFICATION.md)
and [landed timeout boundary](samd-real-spi-timeout-qualification.md).
Stale IRQ, blocking-result lease, lifetime, native Arcade, installed app, RTx,
physical behavior and distribution/licence qualification remain separate.
No consumer pin, threshold, acknowledgement or physical capture changes.
