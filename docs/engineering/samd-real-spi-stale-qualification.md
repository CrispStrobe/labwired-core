# Prospective software-pended idle SPI IRQ qualification

This is a separate mode13 caller at immutable runtime
`35386bee113d4dc1531aa3c8ab90e6f9078191fe`, with unchanged driver/model.
It is not a naturally arising stale hardware interrupt or stale timeout test.
The [landed DMA-error checkpoint](samd-real-spi-error-qualification.md)
and its original failures remain preserved.

A real prime transfer must finish through hardware DMA and SPI TXC, callback
entry/return, actual Event delivery and main-fiber resume. Preserve that entire
prefix before the real guest masks interrupts and stores bit31 to both NVIC
ISPR0/ISPR1, pending IRQ31/IRQ63. Observe every raw store byte, actual exception47
and79 entry/return, idle DMA flags, retained sticky SPI TXC with INTEN0, released
CS and unchanged control registers. No extra callback, failure, DATA/wire byte
or CS transition may occur while the pending IRQs settle. Observe the dispatch
helper's actual PC/LR return before admitting the unchanged28-row async/blocking
recovery. Never replace full observed counts with recovery-only counts.

The expected complete DMA/SPI entry and return census is4/4: one prime hardware
completion, one explicitly software-pended idle dispatch, then two recovery
hardware completions each. Each software entry and return is separately bound
to the actual guest stores and helper epoch. A counter or guest flag alone is
insufficient. Complete startup/vectors/data/BSS/heap/provenance, real context
swaps and the unchanged four-million-step budget remain mandatory.

Build mode13 and positive mode8 with separate clean pinned core checkouts,
run both profiles and retain all four originals even on failure. Nonvacuous
deletion, bad-byte, extra-effect and ordering controls must refuse. Freeze an
independent exact-source/image-bound reader before acquiring the sole original;
do not import producer helpers or replay guests to read evidence. Every enabled
final-head check must pass before matched-head merge and reviewed/tested/landed
tree/parent/ref verification. A shared driver/model repair needs fresh affected
prior caller qualification rather than a silent adoption.

Blocking terminal-result ownership, lifetime, full board/API/image/native
Arcade, installed application, RTx and physical behavior remain separate. No
consumer pin, threshold, acknowledgement or hardware capture changes.
