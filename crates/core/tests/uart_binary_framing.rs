//! `uart_device` binary frames, line-silence framing and real-baud pacing.
//!
//! The toy peer below speaks a Modbus-shaped read protocol (slave address,
//! function, register, count, CRC-16/MODBUS) but is NOT a Modbus slave: it only
//! exercises the primitive's keys.

use labwired_core::peripherals::components::declarative_uart::{
    DeclarativeUartDevice, DeclarativeUartKit,
};
use labwired_core::peripherals::device::UartStreamDevice;
use labwired_core::sim_input::SimInput;

const TOY: &str = r#"
type: toy-reg-peer
behavior:
  primitive: uart_device
  uart:
    baud: 19200
    char_bits: 11
    frames: { framing: silence, gap_chars: 3.5, check: crc16_modbus, max_bytes: 64 }
    regs:
      - { reg: 0, input: temperature }
      - { reg: 1, value: 7 }
      - { reg: 2, value: 258 }
    responses:
      # Address 9 is not ours; a guard on a shared pattern skips this entry.
      - match: { bytes: "addr:u8 0x03 reg:u16be count:u16be" }
        when: "var(addr) == 1 && var(count) > 0"
        respond_bytes: ["var(addr)", "0x03", "var(count) * 2", "regs(var(reg), var(count))", "crc16_modbus"]
      - match: { bytes: "addr:u8 0x06 reg:u16be value:u16be" }
        when: "var(addr) == 1"
        respond_bytes: ["var(addr)", "0x06", "var(reg):u16be", "var(value):u16be", "crc16_modbus"]
      - match: { bytes: "0xFF tail:rest" }
        respond_bytes: ["0xFF", "var(tail)"]
metadata:
  inputs:
    - { key: temperature, label: "Temperature", unit: "C", min: -40, max: 125, default: 21, expr_scale: 10 }
"#;

fn toy() -> DeclarativeUartDevice {
    DeclarativeUartKit::from_yaml(TOY)
        .expect("toy peer validates")
        .device("toy-reg-peer")
        .expect("toy peer builds")
}

fn with_crc(body: &[u8]) -> Vec<u8> {
    let mut v = body.to_vec();
    v.extend_from_slice(&labwired_config::uart_binary::crc16_modbus(body).to_le_bytes());
    v
}

/// Send `frame` as the firmware would, then let the line go quiet and collect
/// what the part answers. `step_us` is the real time credited per poll.
fn exchange(dev: &mut DeclarativeUartDevice, frame: &[u8], step_us: u32) -> Vec<u8> {
    for &b in frame {
        dev.on_tx_byte(b);
    }
    let mut out = Vec::new();
    // 200 ms of device time is far more than any answer here needs.
    for _ in 0..(200_000 / step_us) {
        if let Some(b) = dev.poll(step_us) {
            out.push(b);
        }
    }
    out
}

#[test]
fn a_frame_ends_on_silence_and_is_answered_from_the_register_table() {
    let mut dev = toy();
    let resp = exchange(&mut dev, &with_crc(&[1, 3, 0, 1, 0, 2]), 250);
    // addr, func, byte count, regs 1 and 2, CRC.
    assert_eq!(
        resp,
        with_crc(&[1, 3, 4, 0x00, 0x07, 0x01, 0x02]),
        "answer: {resp:02X?}"
    );
}

#[test]
fn the_register_table_follows_the_input_channel() {
    let mut dev = toy();
    dev.set_input("temperature", 25.0).unwrap();
    let resp = exchange(&mut dev, &with_crc(&[1, 3, 0, 0, 0, 1]), 250);
    // expr_scale 10: 25.0 C is 250 = 0x00FA.
    assert_eq!(resp, with_crc(&[1, 3, 2, 0x00, 0xFA]));
}

#[test]
fn a_frame_does_not_end_while_bytes_keep_arriving() {
    // 3.5 chars at 19200 baud/11 bits is 2.006 ms. Credit time between bytes
    // in steps shorter than that: the frame must stay whole.
    let mut dev = toy();
    let frame = with_crc(&[1, 3, 0, 1, 0, 2]);
    let mut out = Vec::new();
    for &b in &frame {
        dev.on_tx_byte(b);
        for _ in 0..3 {
            // 0.9 ms of quiet after each byte is under the 2.006 ms gap
            // only if the count restarts per byte, which is the rule.
            if let Some(x) = dev.poll(300) {
                out.push(x);
            }
        }
    }
    assert!(out.is_empty(), "frame was cut short: {out:02X?}");
    for _ in 0..400 {
        if let Some(x) = dev.poll(300) {
            out.push(x);
        }
    }
    assert_eq!(out, with_crc(&[1, 3, 4, 0, 7, 1, 2]));
}

#[test]
fn a_bad_checksum_is_dropped_without_an_answer() {
    let mut dev = toy();
    let mut frame = with_crc(&[1, 3, 0, 1, 0, 2]);
    *frame.last_mut().unwrap() ^= 0xFF;
    assert!(exchange(&mut dev, &frame, 250).is_empty());
    assert_eq!(dev.check_errors(), 1);
}

#[test]
fn a_guard_that_fails_leaves_the_part_silent_and_captures_reach_rules() {
    let mut dev = toy();
    // Address 9 matches the pattern but not the guard.
    assert!(exchange(&mut dev, &with_crc(&[9, 3, 0, 1, 0, 2]), 250).is_empty());
    // A write echoes its captures back as 16-bit fields.
    let resp = exchange(&mut dev, &with_crc(&[1, 6, 0x12, 0x34, 0xAB, 0xCD]), 250);
    assert_eq!(resp, with_crc(&[1, 6, 0x12, 0x34, 0xAB, 0xCD]));
    assert_eq!(dev.rule_machine().var("reg"), 0x1234);
}

#[test]
fn rest_of_frame_capture_reports_its_length() {
    let mut dev = toy();
    let resp = exchange(&mut dev, &with_crc(&[0xFF, 1, 2, 3]), 250);
    // 0xFF, then the tail length: three bytes plus two checksum bytes were
    // stripped first, so the tail is the 3 payload bytes.
    assert_eq!(resp, vec![0xFF, 3]);
}

#[test]
fn output_runs_at_the_parts_own_baud() {
    // 19200 baud, 11-bit characters: 572.9 us per byte. A 9-byte answer needs
    // about 5.2 ms on the wire; with 100 us credits it must NOT arrive faster.
    let mut dev = toy();
    for &b in &with_crc(&[1, 3, 0, 1, 0, 2]) {
        dev.on_tx_byte(b);
    }
    let mut first = None;
    let mut last = 0u32;
    let mut count = 0usize;
    let mut t = 0u32;
    while t < 60_000 && count < 9 {
        t += 100;
        if dev.poll(100).is_some() {
            first.get_or_insert(t);
            last = t;
            count += 1;
        }
    }
    assert_eq!(count, 9);
    let span = last - first.unwrap();
    // Eight inter-byte gaps of 572.9 us = 4583 us; allow one 100 us step.
    assert!(
        (4400..=4800).contains(&span),
        "9 bytes spanned {span} us, expected about 4583"
    );
}

#[test]
fn shipped_text_parts_do_not_opt_in() {
    for ty in ["hc-05", "sim800l", "neo6m-gps"] {
        let yaml = labwired_config::embedded_device_yaml(ty).unwrap();
        let dev = DeclarativeUartKit::from_yaml(yaml)
            .unwrap()
            .device(ty)
            .unwrap();
        assert!(!dev.paced_by_device(), "{ty} must keep the host pacing");
        assert_eq!(dev.max_bytes_per_tick(), 1, "{ty}");
        assert_eq!(dev.declared_baud(), None, "{ty} must not raise mismatches");
    }
}
