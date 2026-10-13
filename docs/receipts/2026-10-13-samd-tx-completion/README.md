# Authored TX completion ordering — qualified and landed

[PR220](https://github.com/CrispStrobe/labwired-core/pull/220) merged as
`5809f42850eda119a6fe87597bfe29458be46112` after all21 enabled checks passed;
five workflow-conditional jobs were skipped. Reviewed source
`d673f49db07eee616b2d779f5e5c020ac66877ce`, actual checkout
`e87c324b90d3a1cf1777aa6c449250a6c2968d1e` and the landed merge share tree
`31430791165232061113167257cdee0e837b826b`. Its base/first parent is
`983eb2e0130c096c9cb1b3a9aebea558bc040df1`.

Original [Rust run38420879553](https://github.com/CrispStrobe/labwired-core/actions/runs/38420879553),
attempt1, passed both permanent affected SAM steps. The independently frozen
reader verified exact source/base/tree/job/step/actual-checkout bindings and
the original summary plus raw chronological boundary rows, without replay.

| Profile and official original job | Cases | Ordered / premature refused / no-success abort | Actual callback/return | DMA / SPI exception pairs | Accepted / wire bytes |
| --- | ---: | --- | ---: | --- | --- |
| [Feature-off115380401572](https://github.com/CrispStrobe/labwired-core/actions/runs/38420879553/job/115380401572) |22|16 /2 /4|18|16 /12|82 /72|
| [Feature-on115380401380](https://github.com/CrispStrobe/labwired-core/actions/runs/38420879553/job/115380401380) |44|32 /4 /8|36|32 /24|164 /144|

Original job logs: feature-off755976 bytes, SHA256
`13094e420b29c54b7f3242b6628c2e767acfd463df2c7b519eeccd8c750c5837`;
feature-on314251 bytes, SHA256
`cb8085ec3e163325ad96a751f053ad970485b938011829211e3cfa8a42233ef8`.
There were374/748 raw rows and no missing or duplicate case. All preserved
hardware-TX, software-DMA, bus endpoint and split-IRQ controls passed.

The successful ordering separates final accepted DATA, zero-count writeback/TC,
last slave byte, SPI TXC, actual CS store/release and actual guest callback
entry/caller return. The premature mutant must produce the exact
`cs-before-final-wire` refusal. Masked IRQ, paused MCLK/GCLK, reset and read-error
controls retain their separate required outcomes; reset/error cannot report
success. Feature-on includes event batches1/64/1024 plus forced walk.

Preserve the original failed candidates:
[compile-before-execution38398762963](https://github.com/CrispStrobe/labwired-core/actions/runs/38398762963),
[missing RAM-marker notification38407421347](https://github.com/CrispStrobe/labwired-core/actions/runs/38407421347)
and [largest-batch observation-window38414244825](https://github.com/CrispStrobe/labwired-core/actions/runs/38414244825).
The fixes respectively convert the image address type, read back actual
CPU-written pause markers, and select4096 nominal cycles/byte instead of1024.
The general debugger RAM-word-store observer gap remains unfixed. Frozen
ordering/count/packet rules and the20000-step bound were not relaxed.

The original reader's SHA256 is
`f761428ffe192f1c9834af3d4ddd365579061bf5d5c95d3260fdce92c91cecc5`;
14 synthetic positives and94 refusals passed before originals were acquired.
The final admission reader is
`68d0abaa8673cdacdbae9019448bf491c79698ba4f2d09e3a7ef8ccf2c0d9d7e`;
it passed before and after landing, requiring all21 enabled successes, the
five exact conditional skips and reviewed/tested/landed parents/tree equality.

This is an authored CPU-driver callback, not CODAL events/fibers or native
Arcade. Deliberately slow byte timing is not representative throughput, RTx or
physical timing. No engine implementation, app pin, drift acknowledgement,
capture, expiry or performance threshold changed.

## Subsequent real-service clock qualification — pending

The separate real CODAL service candidate has preserved two actual clock
failures, rather than treating baseline fiber success as runtime completion.
[Run38450843463](https://github.com/CrispStrobe/labwired-core/actions/runs/38450843463)
reproduced stale Event delivery from an IRQ already pending at disable.
[Run38453968112](https://github.com/CrispStrobe/labwired-core/actions/runs/38453968112)
retired that disabled IRQ without an Event, but failed after resume with three
IRQ entry/return pairs instead of two. Later clock cases did not execute.

The next candidate pins
[`c153f8dab17742588b1ad1a49ede26f3cf04ae14`](https://github.com/CrispStrobe/codal-samd/commit/c153f8dab17742588b1ad1a49ede26f3cf04ae14),
whose [standalone build38456953523](https://github.com/CrispStrobe/codal-samd/actions/runs/38456953523)
passed for all four images. It stops periodic SysTick at handler entry and
defers compare rearming until the actual CODAL callback finishes. The expected
eight guest cases, exact clock counts, masked-pending boundaries and two
scheduler profiles are unchanged. Build success is not guest qualification,
whole-image admission, native Arcade, app adoption or a performance result.
