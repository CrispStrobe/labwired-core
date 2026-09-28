# Timed UART networks

A `uart_network` interconnect joins world nodes with serial links that take the
time a real wire takes. Use it to measure what a relay network does to a
message — throughput, buffer use, accumulated latency — and to test recovery:
*when this link slows down and one node restarts, does the network recover
without overflowing its buffers?*

The plain `uart_cross_link` hands a byte to the peer the instant firmware
writes it, into a queue with no bound. That proves two firmwares speak the same
protocol; it cannot show latency or overrun.

## What is modelled

| Part | Behaviour |
|------|-----------|
| Frame time | Start + data (7/8/9) + parity + stop (0.5/1/1.5/2) bits, at the baud rate each USART's `BRR` programs. Characters from one sender never overlap. |
| Receiver | Samples the line at **its own** baud rate and format, mid-bit. RXNE is set at the middle of the first stop bit (9.5 bit times after the start edge for 8N1). A baud or format mismatch reads garbage and framing errors (FE) — never the byte that was sent. |
| RX buffer | The USART data register only. A character that completes while RXNE is set is lost and sets ORE. No host-side queue. |
| TX | TDR + shift register: TXE, TC, and back-to-back characters exactly one frame apart. |
| Link | Extra propagation delay, optional jitter from a per-link generator seeded by `seed` (deterministic). |
| Script | `slow_link`, `reset_node`, `cut_link`, `restore_link` at a world time. |
| Timeline | One time axis (ps): TX start, delivery, RX interrupt, overrun, framing/parity error, drop, node reset, link changes, GPIO markers, tagged message delivery. |

Supported USART: the STM32 F1/F2/F4 register layout (`SR`/`DR`/`BRR`/`CR1-3`),
for example the STM32F401. A network link on any other UART model is refused
at build time rather than left untimed.

Not modelled, and reported: DMA reception (`CR3.DMAR` puts an "unmodelled" entry
on the timeline), IDLE/LIN break flags, hardware flow control, 9-bit transmit
data, the APB prescaler (`BRR` counts core clock cycles — true when PCLK = HCLK).
A node reset resets the core, NVIC, SysTick and the linked USARTs; other
peripherals keep their registers (firmware that re-initialises them is
unaffected). SRAM is kept, as on silicon.

## Manifest

```yaml
schema_version: "1.0"
name: relay-chain
nodes:
  - { id: n0, system: nucleo-f401re.yaml, firmware: source.elf }
  - { id: n1, system: nucleo-f401re.yaml, firmware: relay.elf }
  - { id: n2, system: nucleo-f401re.yaml, firmware: relay.elf }
interconnects:
  - type: uart_network
    nodes: [n0, n1, n2]
    config:
      topology: chain        # nodes[i].uart_out <-> nodes[i+1].uart_in
      uart_in: uart1
      uart_out: uart2
      delay_us: 0            # extra propagation delay, every link
      jitter_us: 0           # uniform [0, jitter_us] per character
      seed: 1
      messages:              # optional: per-message latency
        { sync: 0xA5, length: 5, id_offset: 1, id_bytes: 2, hop_offset: 3, checksum_xor: true }
      events:
        - { at_us: 5500, slow_link: 1, delay_us: 500 }
        - { at_us: 8600, reset_node: n1 }
        - { at_us: 9000, cut_link: 0 }
        - { at_us: 9500, restore_link: 0 }
      markers:               # GPIO edges on the timeline
        - { node: n2, peripheral: gpioa, pin: 5, name: app }
```

`topology: star` joins a `hub` (default: first node) to every other node:
`hub_uarts` lists the hub's UART per spoke, `spoke_uart` / `spoke_uarts` the
spokes' UARTs. Links are numbered in construction order (chain: link `i` is
`nodes[i] → nodes[i+1]`).

## Synchronisation, and why results are exact

Nodes run in conservative rounds. Each round is at most the network's
*lookahead*: the shortest time from a start bit leaving any sender to any
receiver acting on that character (link delay + 9.5 bit times). A character sent
in a round cannot be due at a peer before the round ends, so the peer learns of
it before its time and acts on it at the exact cycle. The result does not depend
on the round length or on node order — the tests run the same 10-node chain with
100 µs and 3 µs rounds and get identical timelines, and the same 3-node chain on
the per-cycle walk and on the event scheduler gives identical numbers.

Exception, documented: on a link whose two ends disagree on the format (a baud
mismatch), the receiver samples past the character into line time the sender
may not have decided yet, so the round shrinks to one 16× oversampling tick
past the delay and the result is exact to that tick.

## Reading the result

* CLI: the environment `result.json` carries `uart_network` — `links` (per
  direction: characters sent/delivered, throughput, utilization, max in flight,
  overruns, framing/parity errors, drops, per-character latency), `messages`
  (origin and per-node latency of every tagged message), `events` (the timeline),
  `node_resets`.
* Browser: `WasmWorld.uart_network_report(since)` returns the same report; the
  playground draws the timeline and the link table.
* Rust: `World::uart_network_report(since)`.

## Reference fixture and golden numbers

`crates/core/tests/fixtures/uart-chain/` is an interrupt-driven STM32F401 relay
(USART1 RXNE interrupt into a ring, main loop parses 5-byte messages and
forwards them with the hop count incremented, USART2 TXE interrupt drains a TX
ring; node 0 sends one message per millisecond). `tests/world_uart_network.rs`
checks it against numbers computed from first principles:

* 115200 baud from 84 MHz: `BRR` = 729 cycles per bit = 8.678571 µs.
* One hop of a 5-byte message = 4 frames + 9.5 bits = 49.5 bits = 429.589 µs.
* `k` links: `k × 429.589 µs + (k − 1) × p`, `p` = the relay's own
  store-and-forward time (measured, bounded 0 < p ≤ 10 µs).
* Slowing a link by `D` adds exactly `D` to every message that crosses it later.
* A mid-forward reset of a relay loses exactly the message it held; the chain
  then recovers with no overrun and ring buffers never above one message.
* A 150 µs RX interrupt on a 86.8 µs character stream shows ORE.

## Not the STM32U585 yet

Renode issue #948 asks for 100+ STM32U585 nodes. LabWired does not model the
STM32U585 (the U5 USART is the newer `ISR`/`RDR`/`TDR` layout with a FIFO), so
the network runs on an F401 today. Adding the U5 means a U585 chip model and a
timed mode for the v2 USART layout.
