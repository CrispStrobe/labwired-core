// LabWired - Firmware Simulation Platform
// SPDX-License-Identifier: MIT

//! Does `analogWrite` / `ledcWrite` duty reach the PAD on an ESP32, -C3, -S3?
//!
//! Before this, no: the LEDC was a register bank (duty readback only), the
//! boot ROM's `gpio_matrix_out` was a no-op thunk on the classic part and the
//! S3 fast-boot harness, so `GPIO_FUNCn_OUT_SEL_CFG` never named the channel,
//! and nothing bound the LEDC's matrix signals to a pad. A dimmed LED on an
//! ESP32 bench read the idle GPIO latch — flat.
//!
//! This runs REAL Arduino-ESP32 images (`crates/core/tests/fixtures/
//! esp32-ledc-pwm/<chip>`, one sketch built for each chip with PlatformIO
//! `espressif32@7.0.1` = arduino-esp32 2.0.17; that directory's README has the
//! rebuild and hashes) through `labwired test` — the runner every gate uses,
//! with real `delay()` timing — and measures each pad through the logic
//! analyzer (`--watch-gpio` → `read_gpio_pad` + push capture), the same read
//! bw-board's circuit consumes. The sketch steps two LEDC channels through five
//! duty phases and toggles a plain GPIO marker between phases:
//!
//! | phase | GPIO2 `analogWrite` (1 kHz, 8 bit, the last channel) | GPIO5 `ledcWrite` (5 kHz, 10 bit, channel 0) |
//! |---|---|---|
//! | 0 | 0 → 0 % | 1023 → full on (Arduino writes 2^10) |
//! | 1 | 64 → 25 % | 256 → 25 % |
//! | 2 | 128 → 50 % | 768 → 75 % |
//! | 3 | 191 → 74.609375 % | 0 → 0 % |
//! | 4 | 255 → full on | 512 → 50 % |
//!
//! Every whole period inside a phase must carry EXACTLY that duty (to the
//! cycle) at EXACTLY the programmed period in core cycles (1 kHz = 240 000 at
//! the classic/S3's 240 MHz, 160 000 at the C3's 160 MHz), and the rails must
//! be flat. On the classic part `analogWrite`'s channel is LS 7 and
//! `ledcWrite`'s HS 0, so both speed groups are covered.

use std::path::PathBuf;
use std::process::Command;

/// GPIO2 (analogWrite), GPIO5 (ledcWrite), GPIO4 (phase marker).
const PADS: [u8; 3] = [2, 5, 4];

/// Expected duty per phase as `(numerator, denominator)`.
const WANT_A: [(u64, u64); 5] = [(0, 1), (64, 256), (128, 256), (191, 256), (1, 1)];
const WANT_B: [(u64, u64); 5] = [(1, 1), (256, 1024), (768, 1024), (0, 1), (512, 1024)];

struct Trace {
    initial: [bool; 3],
    /// Per channel: (cycle, level) transitions, ascending.
    edges: [Vec<(u64, bool)>; 3],
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run_fixture(chip: &str) -> Trace {
    let root = workspace_root();
    let fixture = root.join(format!(
        "crates/core/tests/fixtures/esp32-ledc-pwm/{chip}/firmware.elf"
    ));
    let dir = std::env::temp_dir().join(format!(
        "labwired-esp32-ledc-pad-{chip}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    // A bare board: the committed chip descriptor and nothing else on any pad.
    let system = dir.join("system.yaml");
    std::fs::write(
        &system,
        format!(
            "name: \"esp32-ledc-pad-{chip}\"\nchip: \"{}\"\nboard_io: []\n",
            root.join(format!("configs/chips/{chip}.yaml")).display()
        ),
    )
    .expect("write system");
    let script = dir.join("test.yaml");
    std::fs::write(
        &script,
        format!(
            "schema_version: \"1.0\"\ninputs:\n  firmware: {}\n  system: {}\nlimits:\n  \
             max_steps: 50000000\n  max_uart_bytes: 65536\n  stop_when_assertions_pass: true\n  \
             stop_when_assertions_pass_settle_steps: 1000\n  stop_when_assertions_pass_min_steps: 0\n\
             assertions:\n  - uart_contains: \"LW_B9_DONE\"\n",
            fixture.display(),
            system.display()
        ),
    )
    .expect("write script");
    let out = dir.join("out");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_labwired"));
    cmd.arg("test")
        .arg("--script")
        .arg(&script)
        .arg("--output-dir")
        .arg(&out)
        .arg("--no-uart-stdout");
    for p in PADS {
        cmd.arg("--watch-gpio").arg(format!("gpio:{p}"));
    }
    if chip == "esp32c3" {
        // The C3 image calls into the mask ROM; boot it on the vendored one.
        let roms = root.join("crates/core/roms/esp32c3");
        cmd.env("LABWIRED_ESP32C3_ROM", roms.join("esp32c3_rom.bin"))
            .env("LABWIRED_ESP32C3_ROM_DATA", roms.join("esp32c3_drom.bin"));
    }
    let status = cmd
        .current_dir(&root)
        .status()
        .expect("spawn labwired test");
    let uart = std::fs::read_to_string(out.join("uart.log")).unwrap_or_default();
    assert!(status.success(), "labwired test failed; UART: {uart:?}");
    let result: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("result.json")).expect("result.json"),
    )
    .expect("parse result.json");
    let logic = &result["logic_edges"];
    assert_eq!(logic["dropped"].as_u64(), Some(0), "edge ring overflowed");
    let mut trace = Trace {
        initial: [false; 3],
        edges: Default::default(),
    };
    for ch in logic["channels"].as_array().expect("channels") {
        let pin = ch["pin"].as_u64().expect("pin") as u8;
        let k = PADS.iter().position(|&p| p == pin).expect("a watched pad");
        trace.initial[k] = ch["initial"].as_u64() == Some(1);
        for t in ch["transitions"].as_array().expect("transitions") {
            trace.edges[k].push((
                t["cycle"].as_u64().expect("cycle"),
                t["value"].as_u64() == Some(1),
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    trace
}

/// Level of channel `ch` just after cycle `at`.
fn level_at(t: &Trace, ch: usize, at: u64) -> bool {
    t.edges[ch]
        .iter()
        .take_while(|&&(c, _)| c <= at)
        .last()
        .map_or(t.initial[ch], |&(_, v)| v)
}

/// The duty of every WHOLE period of channel `ch` inside `[from, to)`:
/// `(period, high)` per rising-edge-to-rising-edge interval.
fn periods(t: &Trace, ch: usize, from: u64, to: u64) -> Vec<(u64, u64)> {
    let e: Vec<(u64, bool)> = t.edges[ch]
        .iter()
        .copied()
        .filter(|&(c, _)| c >= from && c < to)
        .collect();
    let rises: Vec<u64> = e.iter().filter(|x| x.1).map(|x| x.0).collect();
    rises
        .windows(2)
        .filter_map(|w| {
            let fall = e.iter().find(|&&(c, v)| !v && c > w[0] && c < w[1])?;
            Some((w[1] - w[0], fall.0 - w[0]))
        })
        .collect()
}

fn check_phase(
    t: &Trace,
    ch: usize,
    from: u64,
    to: u64,
    period: u64,
    want: (u64, u64),
    what: &str,
) {
    let (num, den) = want;
    if num == 0 || num == den {
        let rail = num == den;
        // Skip one period after the marker: the new duty lands at the boundary.
        let settle = from + 2 * period;
        let toggles = t.edges[ch]
            .iter()
            .filter(|&&(c, _)| c > settle && c < to)
            .count();
        assert_eq!(
            toggles,
            0,
            "{what}: a {}% rail must not toggle",
            if rail { 100 } else { 0 }
        );
        assert_eq!(
            level_at(t, ch, settle),
            rail,
            "{what}: rail level must be {}",
            if rail { "high" } else { "low" }
        );
        return;
    }
    // Drop the period straddling the change: the first whole period starts at
    // the first rising edge after the marker plus one period.
    let p = periods(t, ch, from + period, to);
    assert!(
        p.len() >= 3,
        "{what}: only {} whole periods measured",
        p.len()
    );
    for (len, high) in &p {
        // A timer restart (Arduino's analogWrite re-runs ledcSetup) can only
        // ever shorten the LAST interval of a phase; every other one is exact.
        if *len != period {
            continue;
        }
        // Exact to the cycle: an edge lands on the first whole CPU cycle of
        // its count, so a duty whose ideal width is fractional (191/256 of
        // 240 000 = 179 062.5) rounds by at most one cycle.
        assert!(
            (high * den).abs_diff(period * num) <= den,
            "{what}: high {high} of {period} cycles, want {num}/{den}"
        );
    }
    let exact = p.iter().filter(|(len, _)| *len == period).count();
    assert!(
        exact >= 3,
        "{what}: {exact} periods at the programmed {period} cycles"
    );
}

fn assert_duties(chip: &str, cpu_mhz: u64) {
    let t = run_fixture(chip);
    // Phase 0 starts at the marker's first RISING edge; anything before it is
    // boot (a C3 pad reads its pull-up until `pinMode` drives it low).
    let marks: Vec<u64> = t.edges[2]
        .iter()
        .skip_while(|&&(_, v)| !v)
        .map(|&(c, _)| c)
        .collect();
    assert_eq!(
        marks.len(),
        6,
        "{chip}: the phase marker must toggle into each of the five phases and out: {marks:?}"
    );
    let (period_a, period_b) = (cpu_mhz * 1_000, cpu_mhz * 200);
    for phase in 0..5 {
        let (from, to) = (marks[phase], marks[phase + 1]);
        check_phase(
            &t,
            0,
            from,
            to,
            period_a,
            WANT_A[phase],
            &format!("{chip} phase {phase} GPIO2 analogWrite"),
        );
        check_phase(
            &t,
            1,
            from,
            to,
            period_b,
            WANT_B[phase],
            &format!("{chip} phase {phase} GPIO5 ledcWrite"),
        );
    }
}

#[test]
fn esp32_classic_arduino_ledc_duty_reaches_the_pad_exactly() {
    assert_duties("esp32", 240);
}

#[test]
fn esp32c3_arduino_ledc_duty_reaches_the_pad_exactly() {
    assert_duties("esp32c3", 160);
}

#[test]
fn esp32s3_arduino_ledc_duty_reaches_the_pad_exactly() {
    assert_duties("esp32s3", 240);
}
