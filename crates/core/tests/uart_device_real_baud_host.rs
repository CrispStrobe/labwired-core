//! The hosting UART's side of `uart_device` real-baud pacing: device time
//! comes from engine cycles at the core clock, a disagreeing programmed baud is
//! reported, and a part that did not opt in keeps the historical pacing.

use labwired_core::peripherals::components::declarative_uart::DeclarativeUartKit;
use labwired_core::peripherals::uart::{Uart, UartRegisterLayout};
use labwired_core::{CycleClock, Peripheral};

const CPU_HZ: u64 = 8_000_000;

const TOY: &str = r#"
type: toy-reg-peer
behavior:
  primitive: uart_device
  uart:
    baud: 19200
    char_bits: 11
    frames: { framing: silence, gap_chars: 3.5, check: crc16_modbus }
    regs: [ { reg: 1, value: 7 }, { reg: 2, value: 258 } ]
    responses:
      - match: { bytes: "addr:u8 0x03 reg:u16be count:u16be" }
        respond_bytes: ["var(addr)", "0x03", "var(count) * 2", "regs(var(reg), var(count))", "crc16_modbus"]
"#;

fn request() -> Vec<u8> {
    let body = [1u8, 3, 0, 1, 0, 2];
    let mut v = body.to_vec();
    v.extend_from_slice(&labwired_config::uart_binary::crc16_modbus(&body).to_le_bytes());
    v
}

fn host(
    brr: u16,
    dev: Box<dyn labwired_core::peripherals::uart::UartStreamDevice>,
) -> (Uart, CycleClock) {
    let mut u = Uart::new_with_layout(UartRegisterLayout::Stm32F1);
    let clock = CycleClock::default();
    u.attach_cycle_clock(clock.clone());
    u.attach_cpu_hz(CPU_HZ);
    u.attach_stream(dev);
    u.write(0x08, (brr & 0xFF) as u8).unwrap();
    u.write(0x09, (brr >> 8) as u8).unwrap();
    (u, clock)
}

fn toy() -> Box<dyn labwired_core::peripherals::uart::UartStreamDevice> {
    Box::new(
        DeclarativeUartKit::from_yaml(TOY)
            .unwrap()
            .device("toy-reg-peer")
            .unwrap(),
    )
}

/// Drive the UART with `step_us` of engine time per tick and report the
/// elapsed microseconds at which each RX byte appeared.
fn run(u: &mut Uart, clock: &CycleClock, total_us: u64, step_us: u64) -> Vec<u64> {
    let rx = u.rx_buffer();
    let mut seen = 0usize;
    let mut at = Vec::new();
    let mut t = 0;
    while t < total_us {
        t += step_us;
        clock.publish(t * CPU_HZ / 1_000_000);
        u.tick();
        let n = rx.lock().unwrap().len();
        for _ in seen..n {
            at.push(t);
        }
        seen = n;
    }
    at
}

#[test]
fn a_device_on_its_own_baud_answers_in_real_time() {
    // 8 MHz / 19200 baud = 417.
    let (mut u, clock) = host(417, toy());
    for b in request() {
        u.write(0x04, b).unwrap();
    }
    let at = run(&mut u, &clock, 60_000, 50);
    assert_eq!(at.len(), 9, "answer length");
    // Frame ends after 3.5 chars (2.006 ms) of silence; one 50 us step of slack
    // each side, and one more for the first byte going out at once.
    assert!(
        (2000..=2300).contains(&at[0]),
        "first byte at {} us, expected just after the 2.006 ms gap",
        at[0]
    );
    let span = at[8] - at[0];
    assert!(
        (4400..=4800).contains(&span),
        "nine bytes spanned {span} us, expected about 4583 (8 x 572.9)"
    );
}

#[test]
fn a_programmed_baud_that_disagrees_is_reported() {
    labwired_core::fidelity::reset();
    // 8 MHz / 69 = 115942 baud against a part at 19200.
    let (mut u, clock) = host(69, toy());
    run(&mut u, &clock, 1_000, 100);
    let report = labwired_core::fidelity::take();
    let note = report
        .approximations
        .get(labwired_core::fidelity::UART_BAUD_MISMATCH)
        .expect("mismatch is reported");
    assert!(note.detail.contains("19200"), "{}", note.detail);
    assert!(note.detail.contains("115"), "{}", note.detail);
}

#[test]
fn a_matching_baud_reports_nothing() {
    labwired_core::fidelity::reset();
    let (mut u, clock) = host(417, toy());
    run(&mut u, &clock, 1_000, 100);
    assert!(!labwired_core::fidelity::take()
        .approximations
        .contains_key(labwired_core::fidelity::UART_BAUD_MISMATCH));
}

#[test]
fn an_apb_prescaled_divisor_is_not_a_false_alarm() {
    labwired_core::fidelity::reset();
    // 8 MHz / 208 = 38461: exactly what a UART on a /2 APB clock programmed
    // for 19200 baud looks like when the divisor is read against the core clock.
    let (mut u, clock) = host(208, toy());
    run(&mut u, &clock, 1_000, 100);
    assert!(!labwired_core::fidelity::take()
        .approximations
        .contains_key(labwired_core::fidelity::UART_BAUD_MISMATCH));
}

#[test]
fn a_text_part_keeps_the_host_pacing_whatever_the_clock_says() {
    labwired_core::fidelity::reset();
    let yaml = labwired_config::embedded_device_yaml("hc-05").unwrap();
    let hc05 = Box::new(
        DeclarativeUartKit::from_yaml(yaml)
            .unwrap()
            .device("hc-05")
            .unwrap(),
    );
    // Programmed baud far from the part's 38400, and a clock that never moves:
    // neither may change anything for a part that did not opt in.
    let (mut u, _clock) = host(69, hc05);
    for b in b"AT\r\n" {
        u.write(0x04, *b).unwrap();
    }
    let rx = u.rx_buffer();
    for tick in 1..=4 {
        u.tick();
        assert_eq!(rx.lock().unwrap().len(), tick, "one byte per bus tick");
    }
    let got: Vec<u8> = rx.lock().unwrap().iter().copied().collect();
    assert_eq!(got, b"OK\r\n");
    assert!(!labwired_core::fidelity::take()
        .approximations
        .contains_key(labwired_core::fidelity::UART_BAUD_MISMATCH));
}
