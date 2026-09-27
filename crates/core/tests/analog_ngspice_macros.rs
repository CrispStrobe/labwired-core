// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

//! Differential oracle for the dependent sources, the op-amp and comparator
//! macros, and diode breakdown: the in-core engine against ngspice-47, at
//! 0.05 % of full scale on every sample.
//!
//! ## Two decks per case, written separately
//!
//! The in-core side of each case uses this engine's own vocabulary — an `X`
//! line naming the built-in `LM358`, `LM393`, `IDEAL_OPAMP`, `1N4733A` or
//! `SMAJ5.0A` card. ngspice has no such cards, so its side is written out by
//! hand as a `.subckt` built from the *documented* macro (the doc comments on
//! `OpAmpModel` and `ComparatorModel`) and the datasheet numbers, not from the
//! lowering code. A mistake in the lowering — a pole in the wrong place, a
//! clamp on the wrong rail, a sign — is therefore a disagreement here, not a
//! shared assumption. The dependent-source deck is one text both engines read.
//!
//! ## Where the ngspice half comes from
//!
//! CI has no ngspice binary: `pr-workspace-tests` installs none, and the older
//! `crates/cli/tests/analog_vs_ngspice_differential.rs` therefore SKIPS there,
//! which is a green that measured nothing. So the reference samples are
//! committed, one file per case, under `tests/fixtures/analog_ngspice/`, and
//! the comparison runs against them everywhere. Each file records a hash of the
//! exact ngspice deck that produced it; a deck edited without regenerating its
//! file fails [`golden_files_match_their_decks`]. Where ngspice IS on PATH the
//! committed samples are also re-derived and compared with a fresh run, so a
//! stale file cannot hide either.
//!
//! Regenerate after a deliberate change to a deck:
//!
//! ```text
//! LABWIRED_REGEN_NGSPICE_GOLDEN=1 cargo test -p labwired-core --test analog_ngspice_macros
//! ```
//!
//! ## The negative controls are tests
//!
//! A 0.05 % differential is only worth something if a small physical error
//! breaks it. [`negative_controls_fail_the_differential`] reruns the zener
//! regulator with `BV` 1 % high and the inverting amplifier with the op-amp's
//! gain 1 % high (`AOL` and `GBW` together — the gain stage's
//! transconductance), and asserts that each one FAILS against the same
//! committed ngspice samples. If a future change makes the tolerance too loose
//! to see a 1 % device error, that test goes red.

use labwired_core::analog::{parse_netlist, Integration, Solver};
use std::path::PathBuf;
use std::process::Command;

/// Worst allowed disagreement on any sample, as a fraction of the case's full
/// scale. The same number as the older device differential, for the same
/// reason: 2 % was measured to be vacuous there.
const TOLERANCE: f64 = 5e-4;

/// The ngspice options every reference deck runs with: the in-core engine's
/// temperature (300.00 K) and Newton tolerances, so neither engine is solving
/// a looser problem than the other.
const NGSPICE_OPTIONS: &str =
    ".options temp=26.85 tnom=26.85 reltol=1e-9 abstol=1e-15 vntol=1e-12 gmin=1e-12\n";

/// One circuit, solved by both engines and compared at every sample.
struct Case {
    /// File stem of the committed reference, and the failure message's name.
    name: &'static str,
    /// What the in-core engine parses.
    in_core: String,
    /// What ngspice runs (the circuit only: options and control are added).
    ngspice: String,
    /// Node compared.
    node: &'static str,
    /// Sample interval, seconds.
    sample: f64,
    /// In-core internal steps per sample; ngspice's maximum step is the same.
    substeps: u32,
    /// Samples compared.
    samples: usize,
    /// Denominator of the error, volts.
    full_scale: f64,
    /// The compared trace must swing at least this far, or agreement proves
    /// nothing.
    minimum_swing: f64,
    /// For a switching output: the level whose crossings are compared as
    /// TIMES (see [`EDGE_TOLERANCE`]), with the samples an edge falls in left
    /// out of the voltage comparison. `None` compares every sample.
    edge_level: Option<f64>,
}

// ---------------------------------------------------------------------------
// The macros, as ngspice subcircuits written from their documentation
// ---------------------------------------------------------------------------

/// The clamp diode both macros use: `IS=1e-12 N=0.02`.
const CLAMP_MODEL: &str = ".model DCLAMP D(IS=1e-12 N=0.02)\n";

/// Gain-node resistance of both macros, ohms.
const RG: f64 = 1e6;

/// The op-amp macro as documented on `OpAmpModel`, as an ngspice subcircuit
/// with pins `inp inn vcc vee out`.
fn opamp_subckt(name: &str, aol: f64, gbw: f64, rout: f64, drop_hi: f64, drop_lo: f64) -> String {
    let gm = aol / RG;
    let cg = aol / (2.0 * std::f64::consts::PI * gbw * RG);
    let mut text = format!(".subckt {name} inp inn vcc vee out\n");
    text += &format!("G1 0 p inp inn {gm:.17e}\n");
    text += &format!("Rg p 0 {RG:.17e}\n");
    text += &format!("Cg p 0 {cg:.17e}\n");
    let hi = if drop_hi == 0.0 {
        "vcc"
    } else {
        text += &format!("Vhi vcc hi {drop_hi:.17e}\n");
        "hi"
    };
    let lo = if drop_lo == 0.0 {
        "vee"
    } else {
        text += &format!("Vlo lo vee {drop_lo:.17e}\n");
        "lo"
    };
    text += &format!("Dhi p {hi} DCLAMP\nDlo {lo} p DCLAMP\n");
    text += &format!("Go 0 out p 0 {:.17e}\n", 1.0 / rout);
    text += &format!("Ro out 0 {rout:.17e}\n");
    text += CLAMP_MODEL;
    text += ".ends\n";
    text
}

/// The comparator macro as documented on `ComparatorModel`.
fn comparator_subckt(
    name: &str,
    aol: f64,
    gbw: f64,
    rout: f64,
    vhys: f64,
    push_pull: bool,
) -> String {
    let gm = aol / RG;
    let cq = aol / (2.0 * std::f64::consts::PI * gbw * RG);
    let mut text = format!(".subckt {name} inp inn vcc vee out\n");
    text += &format!("G1 vee q inn inp {gm:.17e}\n");
    text += &format!("Rq q vee {RG:.17e}\n");
    text += &format!("Cq q vee {cq:.17e}\n");
    if vhys != 0.0 {
        let gh = gm * vhys;
        text += &format!("Gh vee q q vee {gh:.17e}\n");
        text += &format!("Ih q vee {:.17e}\n", 0.5 * gh);
    }
    text += "Vqh qh vee 1\nDqh q qh DCLAMP\nDql vee q DCLAMP\n";
    if push_pull {
        let gp = 1e4 / RG;
        text += &format!("Gp p 0 q vee {gp:.17e}\n");
        text += &format!("Ip 0 p {:.17e}\n", 0.5 * gp);
        text += &format!("Rp p 0 {RG:.17e}\n");
        text += "Dph p vcc DCLAMP\nDpl vee p DCLAMP\n";
        text += &format!("Go 0 out p 0 {:.17e}\n", 1.0 / rout);
        text += &format!("Ro out 0 {rout:.17e}\n");
    } else {
        // Open collector: an N-channel switch with KP = 2/ROUT, W = L = 1.
        text += "Mo out q vee vee MOUT W=1 L=1\n";
        text += &format!(
            ".model MOUT NMOS(LEVEL=1 VTO=0.5 KP={:.17e} LAMBDA=0)\n",
            2.0 / rout
        );
    }
    text += CLAMP_MODEL;
    text += ".ends\n";
    text
}

// The built-in cards' values, restated from the datasheets they cite. If a
// card in `builtin_model` drifts from these, the differential says so.
const LM358: (f64, f64, f64, f64, f64) = (1e5, 0.7e6, 50.0, 1.5, 0.0);
const IDEAL: (f64, f64, f64, f64, f64) = (1e6, 10e6, 1.0, 0.0, 0.0);
const LM393: (f64, f64, f64) = (2e5, 12e6, 37.5);
const ZENER_1N4733A: &str = "D(IS=1e-11 N=1.2 RS=6 BV=4.8 IBV=49m)";
const TVS_SMAJ5_0A: &str = "D(IS=1e-12 N=1 RS=0.0525 BV=6.7 IBV=10m)";

// ---------------------------------------------------------------------------
// The cases
// ---------------------------------------------------------------------------

/// Inverting amplifier, gain −10, LM358 on ±12 V, driven at 20 kHz where the
/// 0.7 MHz GBW has a loop gain of only ~3 — so the pole is being measured, not
/// just the resistor ratio — and hard enough (1.2 V peak) to clip on the
/// DROP_HI rail at +10.5 V.
fn inverting(in_core_card: &str) -> Case {
    let circuit = "Vcc vcc 0 dc 12\n\
                   Vee vee 0 dc -12\n\
                   Vin in 0 SIN(0 1.2 20k)\n\
                   Rin in inn 10k\n\
                   Rf inn out 100k\n\
                   Rl out 0 10k\n";
    let in_core = format!("{circuit}XU1 0 inn vcc vee out {in_core_card}\n");
    let (aol, gbw, rout, hi, lo) = LM358;
    let ngspice = format!(
        "{circuit}XU1 0 inn vcc vee out LM358M\n{}",
        opamp_subckt("LM358M", aol, gbw, rout, hi, lo)
    );
    Case {
        name: "opamp_inverting_lm358",
        in_core,
        ngspice,
        node: "out",
        sample: 1e-6,
        substeps: 100,
        samples: 150,
        full_scale: 12.0,
        minimum_swing: 15.0,
        edge_level: None,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();

    cases.push(inverting("LM358"));

    // Non-inverting, gain 2, rail-to-rail IDEAL_OPAMP on a single 5 V supply,
    // driven past both rails so both clamps engage.
    {
        let circuit = "Vcc vcc 0 dc 5\n\
                       Vin in 0 SIN(1.25 1.5 5k)\n\
                       R1 inn 0 10k\n\
                       R2 out inn 10k\n\
                       Rl out 0 10k\n";
        let (aol, gbw, rout, hi, lo) = IDEAL;
        cases.push(Case {
            name: "opamp_noninverting_ideal",
            in_core: format!("{circuit}XU1 in inn vcc 0 out IDEAL_OPAMP\n"),
            ngspice: format!(
                "{circuit}XU1 in inn vcc 0 out IDEALM\n{}",
                opamp_subckt("IDEALM", aol, gbw, rout, hi, lo)
            ),
            node: "out",
            sample: 2e-6,
            substeps: 40,
            samples: 150,
            full_scale: 5.0,
            minimum_swing: 4.5,
            edge_level: None,
        });
    }

    // Schmitt trigger: LM393 (open collector, 10 k pull-up) with external
    // positive feedback, driven by a 0–5 V triangle. Thresholds ≈ 2.27 V and
    // 2.73 V.
    {
        let circuit = "Vcc vcc 0 dc 5\n\
                       Vref ref 0 dc 2.5\n\
                       Vin in 0 PULSE(0 5 0 10u 10u 0 20u)\n\
                       Rpu vcc out 10k\n\
                       R1 ref inp 10k\n\
                       R2 out inp 100k\n";
        let (aol, gbw, rout) = LM393;
        cases.push(Case {
            name: "comparator_schmitt_lm393",
            in_core: format!("{circuit}XU1 inp in vcc 0 out LM393\n"),
            ngspice: format!(
                "{circuit}XU1 inp in vcc 0 out LM393M\n{}",
                comparator_subckt("LM393M", aol, gbw, rout, 0.0, false)
            ),
            node: "out",
            sample: 0.1e-6,
            substeps: 200,
            samples: 400,
            full_scale: 5.0,
            minimum_swing: 4.0,
            edge_level: Some(2.5),
        });
    }

    // Push-pull comparator with 200 mV of INTERNAL hysteresis, no feedback
    // resistors: the window comes from VHYS alone.
    {
        let circuit = "Vcc vcc 0 dc 3.3\n\
                       Vref inn 0 dc 1.65\n\
                       Vin in 0 SIN(1.65 0.4 50k)\n\
                       Rl out 0 10k\n";
        cases.push(Case {
            name: "comparator_pushpull_vhys",
            in_core: format!(
                "{circuit}XU1 in inn vcc 0 out CPP\n\
                 .model CPP COMP(AOL=2e5 GBW=12meg ROUT=20 VHYS=0.2 OUTPUT=PP)\n"
            ),
            ngspice: format!(
                "{circuit}XU1 in inn vcc 0 out CPPM\n{}",
                comparator_subckt("CPPM", 2e5, 12e6, 20.0, 0.2, true)
            ),
            node: "out",
            sample: 0.1e-6,
            substeps: 200,
            samples: 400,
            full_scale: 3.3,
            minimum_swing: 3.0,
            edge_level: Some(1.65),
        });
    }

    // Zener shunt regulator: 100 Ω from a supply ramped 0 → 9 V over 1 ms,
    // held, and ramped back — the whole reverse I–V, knee included.
    cases.push(zener_regulator("1N4733A", None));

    // TVS clamp: a 24 V, 2 Ω pulse onto a 1 k load protected by SMAJ5.0A.
    {
        let circuit = "Vsurge src 0 PULSE(0 24 10u 1u 1u 20u 60u)\n\
                       Rs src prot 2\n\
                       Rl prot 0 1k\n\
                       D1 0 prot ";
        cases.push(Case {
            name: "tvs_clamp_smaj5v0a",
            in_core: format!("{circuit}SMAJ5.0A\n"),
            ngspice: format!("{circuit}DTVS\n.model DTVS {TVS_SMAJ5_0A}\n"),
            node: "prot",
            sample: 0.5e-6,
            substeps: 10,
            samples: 120,
            full_scale: 24.0,
            minimum_swing: 6.0,
            edge_level: None,
        });
    }

    // All four linear dependent sources in one deck, identical text to both
    // engines: a VCVS gain stage into an RC, a VCCS into a load, a CCCS
    // mirroring a sense source's current and a CCVS turning it back into a
    // voltage.
    {
        let deck = "Vin in 0 SIN(0 1 2k)\n\
                    E1 a 0 in 0 4\n\
                    R1 a b 1k\n\
                    C1 b 0 100n\n\
                    G1 0 c b 0 2m\n\
                    Rc c 0 1k\n\
                    Vsense c d dc 0\n\
                    Rd d 0 1k\n\
                    F1 0 e Vsense 3\n\
                    Re e 0 500\n\
                    H1 f 0 Vsense 2k\n\
                    Rf f 0 1k\n\
                    Esum out 0 e f 0.5\n\
                    Rout out 0 1k\n"
            .to_string();
        cases.push(Case {
            name: "dependent_sources_efgh",
            in_core: deck.clone(),
            ngspice: deck,
            node: "out",
            sample: 5e-6,
            substeps: 100,
            samples: 200,
            full_scale: 4.0,
            minimum_swing: 1.0,
            edge_level: None,
        });
    }

    cases
}

/// The zener regulator, with the in-core side on `card` (a built-in name, or
/// `ZS` defined by `card_line`).
fn zener_regulator(card: &str, card_line: Option<&str>) -> Case {
    let circuit = "Vin in 0 PULSE(0 9 0 1m 1m 1m 4m)\n\
                   R1 in k 100\n\
                   Rl k 0 10k\n\
                   D1 0 k ";
    let in_core = match card_line {
        Some(line) => format!("{circuit}{card}\n{line}\n"),
        None => format!("{circuit}{card}\n"),
    };
    Case {
        name: "zener_regulator_1n4733a",
        in_core,
        ngspice: format!("{circuit}DZ\n.model DZ {ZENER_1N4733A}\n"),
        node: "k",
        sample: 20e-6,
        substeps: 20,
        samples: 175,
        full_scale: 9.0,
        minimum_swing: 4.5,
        edge_level: None,
    }
}

// ---------------------------------------------------------------------------
// The two engines
// ---------------------------------------------------------------------------

/// Largest allowed disagreement between the two engines about WHEN a
/// switching output crosses its mid level, seconds.
///
/// A comparator's edge is regenerative: it completes in a few nanoseconds, so
/// comparing the one sample it lands in measures the sampling grid, not the
/// comparator (a 1 ns shift is a full-scale error on that sample). The
/// question that matters for a comparator is where its THRESHOLD is, and a
/// threshold error shows up as an edge-time error. The scale: 1 % of the
/// 200 mV `VHYS` window is 1 mV, which on the push-pull deck's 50 kHz sine
/// moves the edge by ~8 ns. 2 ns catches that with 4× margin.
const EDGE_TOLERANCE: f64 = 2e-9;

/// One engine's view of a case: the value at every sample boundary and, for a
/// switching case, the interpolated time of every mid-level crossing.
struct Trace {
    samples: Vec<f64>,
    edges: Vec<f64>,
}

/// Crossings of `level` between two consecutive points, linearly interpolated.
fn crossing(level: f64, t0: f64, v0: f64, t1: f64, v1: f64) -> Option<f64> {
    if (v0 < level) != (v1 < level) {
        Some(t0 + (level - v0) * (t1 - t0) / (v1 - v0))
    } else {
        None
    }
}

fn in_core_trace(case: &Case) -> Trace {
    let circuit =
        parse_netlist(&case.in_core).unwrap_or_else(|error| panic!("{}: {error}", case.name));
    let mut solver = Solver::new(circuit, Integration::Trapezoidal)
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));
    let node = solver
        .circuit()
        .node(case.node)
        .unwrap_or_else(|| panic!("{}: no node `{}`", case.name, case.node));
    let h = case.sample / f64::from(case.substeps);
    let mut samples = Vec::with_capacity(case.samples);
    let mut edges = Vec::new();
    let (mut t_prev, mut v_prev) = (0.0, solver.node_voltage(node));
    for sample in 0..case.samples {
        for substep in 0..case.substeps {
            solver.advance(h).unwrap_or_else(|error| {
                panic!("{}: sample {sample} substep {substep}: {error}", case.name)
            });
            let (t, v) = (solver.time(), solver.node_voltage(node));
            if let Some(level) = case.edge_level {
                edges.extend(crossing(level, t_prev, v_prev, t, v));
            }
            (t_prev, v_prev) = (t, v);
        }
        samples.push(solver.node_voltage(node));
    }
    Trace { samples, edges }
}

/// The full ngspice deck for `case`, exactly as run and as hashed. `raw` gets
/// ngspice's own adaptive time points (fine at an edge, which is where the
/// crossing times are read from); `data` gets them resampled onto the grid.
fn ngspice_deck(case: &Case, data: &str, raw: &str) -> String {
    let tmax = case.sample / f64::from(case.substeps);
    let tstop = case.sample * case.samples as f64;
    format!(
        "* {name}\n{circuit}{NGSPICE_OPTIONS}\
         .control\n\
         set numdgt=17\n\
         tran {sample:e} {tstop:e} 0 {tmax:e}\n\
         wrdata {raw} v({node})\n\
         linearize v({node})\n\
         wrdata {data} v({node})\n\
         .endc\n\
         .end\n",
        name = case.name,
        circuit = case.ngspice,
        sample = case.sample,
        node = case.node,
    )
}

/// FNV-1a of the deck with its output paths blanked, so the hash names the
/// circuit and the analysis, not the temp directory it ran in.
fn deck_hash(case: &Case) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in ngspice_deck(case, "OUT", "RAW").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn ngspice_available() -> bool {
    Command::new("ngspice")
        .arg("-v")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// `<time> <value>` rows of a `wrdata` file.
fn wrdata_rows(text: &str) -> Vec<(f64, f64)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let t = fields.next()?.parse::<f64>().ok()?;
            let v = fields.next()?.parse::<f64>().ok()?;
            Some((t, v))
        })
        .collect()
}

fn run_ngspice(case: &Case) -> Trace {
    let dir = std::env::temp_dir().join(format!(
        "labwired-ngspice-macros-{}-{}",
        std::process::id(),
        case.name
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let data = dir.join("out.data");
    let raw = dir.join("raw.data");
    let cir = dir.join("deck.cir");
    std::fs::write(
        &cir,
        ngspice_deck(
            case,
            &data.display().to_string(),
            &raw.display().to_string(),
        ),
    )
    .expect("write deck");
    let output = Command::new("ngspice")
        .arg("-b")
        .arg(&cir)
        .current_dir(&dir)
        .output()
        .expect("ngspice runs");
    assert!(
        output.status.success(),
        "{}: ngspice failed: {}{}",
        case.name,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let read = |path: &PathBuf| {
        std::fs::read_to_string(path).unwrap_or_else(|error| {
            panic!(
                "{}: ngspice wrote no {} ({error}); stdout was {}",
                case.name,
                path.display(),
                String::from_utf8_lossy(&output.stdout)
            )
        })
    };
    let resampled = wrdata_rows(&read(&data));
    let adaptive = wrdata_rows(&read(&raw));
    let _ = std::fs::remove_dir_all(&dir);
    // Row 0 is the t = 0 operating point, which the in-core series does not
    // report (it reports the END of each step).
    let samples: Vec<f64> = resampled
        .iter()
        .skip(1)
        .take(case.samples)
        .map(|(_, v)| *v)
        .collect();
    assert_eq!(
        samples.len(),
        case.samples,
        "{}: short ngspice run",
        case.name
    );
    let edges = match case.edge_level {
        None => Vec::new(),
        Some(level) => adaptive
            .windows(2)
            .filter_map(|pair| crossing(level, pair[0].0, pair[0].1, pair[1].0, pair[1].1))
            .collect(),
    };
    Trace { samples, edges }
}

fn golden_path(case: &Case) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/analog_ngspice")
        .join(format!("{}.txt", case.name))
}

/// The committed reference: `# deck <hash>`, `# edge <t>` for each crossing,
/// then one sample per line.
fn read_golden(case: &Case) -> (String, Trace) {
    let path = golden_path(case);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "{}: no committed ngspice reference at {} ({error}); generate it with \
             LABWIRED_REGEN_NGSPICE_GOLDEN=1 on a host with ngspice-47",
            case.name,
            path.display()
        )
    });
    let mut hash = String::new();
    let mut trace = Trace {
        samples: Vec::new(),
        edges: Vec::new(),
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# deck ") {
            hash = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("# edge ") {
            trace
                .edges
                .push(rest.trim().parse::<f64>().expect("golden edge"));
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            trace
                .samples
                .push(line.trim().parse::<f64>().expect("golden value"));
        }
    }
    (hash, trace)
}

fn write_golden(case: &Case, trace: &Trace) {
    let mut text = format!(
        "# ngspice-47 reference for `{}` (crates/core/tests/analog_ngspice_macros.rs)\n\
         # v({}) at every {:e} s, t = 0 excluded\n\
         # deck {}\n",
        case.name,
        case.node,
        case.sample,
        deck_hash(case)
    );
    for edge in &trace.edges {
        text += &format!("# edge {edge:.17e}\n");
    }
    for value in &trace.samples {
        text += &format!("{value:.17e}\n");
    }
    let path = golden_path(case);
    std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("fixture dir");
    std::fs::write(&path, text).expect("write golden");
}

/// The verdict of one comparison.
struct Comparison {
    /// Worst sample disagreement, fraction of full scale, and its index.
    worst: f64,
    at: usize,
    /// Samples left out because an edge falls inside them.
    skipped: usize,
    /// Worst edge-time disagreement, seconds (0 when the case has no edges).
    worst_edge: f64,
    /// Edge counts, ours and theirs.
    edges: (usize, usize),
}

fn compare(case: &Case, ours: &Trace, theirs: &Trace) -> Comparison {
    let near_edge = |index: usize| {
        let t1 = (index as f64 + 1.0) * case.sample;
        let t0 = t1 - 2.0 * case.sample;
        ours.edges
            .iter()
            .chain(theirs.edges.iter())
            .any(|edge| *edge > t0 && *edge <= t1)
    };
    let mut worst = 0.0_f64;
    let mut at = 0;
    let mut skipped = 0;
    for (index, (a, n)) in ours.samples.iter().zip(theirs.samples.iter()).enumerate() {
        if case.edge_level.is_some() && near_edge(index) {
            skipped += 1;
            continue;
        }
        let error = (a - n).abs() / case.full_scale;
        if error > worst || error.is_nan() {
            worst = error;
            at = index;
        }
    }
    let mut worst_edge = 0.0_f64;
    for (a, n) in ours.edges.iter().zip(theirs.edges.iter()) {
        worst_edge = worst_edge.max((a - n).abs());
    }
    Comparison {
        worst,
        at,
        skipped,
        worst_edge,
        edges: (ours.edges.len(), theirs.edges.len()),
    }
}

impl Comparison {
    fn passes(&self) -> bool {
        self.worst < TOLERANCE && self.worst_edge < EDGE_TOLERANCE && self.edges.0 == self.edges.1
    }

    fn describe(&self, case: &Case, ours: &Trace, theirs: &Trace) -> String {
        let mut text = format!(
            "{:<28} {:>4} samples  worst {:.3e} % of {} V (sample {}: in-core {} V vs ngspice {} V)",
            case.name,
            case.samples,
            self.worst * 100.0,
            case.full_scale,
            self.at + 1,
            ours.samples[self.at],
            theirs.samples[self.at]
        );
        if case.edge_level.is_some() {
            text += &format!(
                "; {} vs {} edges, worst edge time {:.3} ns, {} edge samples excluded",
                self.edges.0,
                self.edges.1,
                self.worst_edge * 1e9,
                self.skipped
            );
        }
        text
    }
}

fn swing(series: &[f64]) -> f64 {
    let max = series.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = series.iter().cloned().fold(f64::INFINITY, f64::min);
    max - min
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn golden_files_match_their_decks() {
    if std::env::var_os("LABWIRED_REGEN_NGSPICE_GOLDEN").is_some() {
        assert!(
            ngspice_available(),
            "LABWIRED_REGEN_NGSPICE_GOLDEN is set but `ngspice` is not on PATH"
        );
        for case in cases() {
            write_golden(&case, &run_ngspice(&case));
        }
    }
    for case in cases() {
        let (hash, trace) = read_golden(&case);
        assert_eq!(
            hash,
            deck_hash(&case),
            "{}: the committed ngspice reference was produced by a different deck; \
             regenerate with LABWIRED_REGEN_NGSPICE_GOLDEN=1",
            case.name
        );
        assert_eq!(
            trace.samples.len(),
            case.samples,
            "{}: sample count",
            case.name
        );
        if case.edge_level.is_some() {
            assert!(
                trace.edges.len() >= 2,
                "{}: a switching case must switch, and the reference has {} edges",
                case.name,
                trace.edges.len()
            );
        }
    }
}

#[test]
fn committed_references_are_what_ngspice_produces_today() {
    if !ngspice_available() {
        eprintln!(
            "SKIP (freshness only): `ngspice` is not on PATH, so the committed references \
             cannot be re-derived here. The differential itself still runs against them in \
             `macros_match_ngspice`."
        );
        return;
    }
    if std::env::var_os("LABWIRED_REGEN_NGSPICE_GOLDEN").is_some() {
        // Being rewritten by the sibling test in this same run.
        return;
    }
    for case in cases() {
        let fresh = run_ngspice(&case);
        let (_, committed) = read_golden(&case);
        let verdict = compare(&case, &fresh, &committed);
        assert!(
            verdict.worst < 1e-9
                && verdict.worst_edge < 1e-15
                && verdict.edges.0 == verdict.edges.1,
            "{}: ngspice today disagrees with the committed reference ({}); regenerate with \
             LABWIRED_REGEN_NGSPICE_GOLDEN=1",
            case.name,
            verdict.describe(&case, &fresh, &committed)
        );
    }
}

#[test]
fn macros_match_ngspice() {
    let mut report = String::new();
    for case in cases() {
        let ours = in_core_trace(&case);
        let (_, theirs) = read_golden(&case);
        let s = swing(&ours.samples);
        assert!(
            s >= case.minimum_swing,
            "{}: the trace only swings {s} V, so agreeing about it proves nothing; \
             expected at least {} V",
            case.name,
            case.minimum_swing
        );
        let verdict = compare(&case, &ours, &theirs);
        report += &verdict.describe(&case, &ours, &theirs);
        report += "\n";
        if std::env::var_os("LABWIRED_DIFF_DUMP").is_some() {
            for (i, (a, n)) in ours.samples.iter().zip(theirs.samples.iter()).enumerate() {
                eprintln!(
                    "{} {} {a} {n} {:e}",
                    case.name,
                    i + 1,
                    (a - n) / case.full_scale
                );
            }
            eprintln!("{} edges ours {:?}", case.name, ours.edges);
            eprintln!("{} edges theirs {:?}", case.name, theirs.edges);
            continue;
        }
        assert!(
            verdict.passes(),
            "{}: outside tolerance ({} % of full scale, {} ns on an edge):\n{report}",
            case.name,
            TOLERANCE * 100.0,
            EDGE_TOLERANCE * 1e9
        );
    }
    eprint!("{report}");
}

#[test]
fn negative_controls_fail_the_differential() {
    // Zener BV 1 % high: 4.8 V → 4.848 V, every other parameter as the card.
    let sabotaged_zener = zener_regulator(
        "ZS",
        Some(".model ZS D(IS=1e-11 N=1.2 RS=6 BV=4.848 IBV=49m)"),
    );
    // Op-amp gain 1 % high: AOL and GBW both +1 %, i.e. the gain stage's
    // transconductance, everything else as the LM358 card.
    let sabotaged_opamp =
        inverting("LMS\n.model LMS OPAMP(AOL=1.01e5 GBW=0.707meg ROUT=50 DROP_HI=1.5 DROP_LO=0)");

    // Comparator hysteresis 1 % wide: VHYS 200 mV → 202 mV. This one can only
    // fail on the EDGE times — the settled levels do not depend on VHYS — so
    // it is the negative control for `EDGE_TOLERANCE`.
    let mut sabotaged_comparator = cases()
        .into_iter()
        .find(|case| case.name == "comparator_pushpull_vhys")
        .expect("push-pull case");
    sabotaged_comparator.in_core = sabotaged_comparator
        .in_core
        .replace("VHYS=0.2 ", "VHYS=0.202 ");
    assert!(sabotaged_comparator.in_core.contains("VHYS=0.202 "));

    let mut report = String::new();
    for (label, case) in [
        ("zener BV +1 %", &sabotaged_zener),
        ("op-amp gain +1 %", &sabotaged_opamp),
        ("comparator VHYS +1 %", &sabotaged_comparator),
    ] {
        let ours = in_core_trace(case);
        let (_, theirs) = read_golden(case);
        let verdict = compare(case, &ours, &theirs);
        report += &format!(
            "negative control `{label}`: {} — {} the differential ({} % / {} ns)\n",
            verdict.describe(case, &ours, &theirs),
            if verdict.passes() { "PASSES" } else { "FAILS" },
            TOLERANCE * 100.0,
            EDGE_TOLERANCE * 1e9
        );
        assert!(
            !verdict.passes(),
            "a 1 % device error must fail the differential, and `{label}` did not:\n{report}"
        );
    }
    eprint!("{report}");
}
