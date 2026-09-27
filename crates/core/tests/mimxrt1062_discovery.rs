// Discovery harness (temporary): boot the FB200 stock image with a logging
// register file behind every unmodelled peripheral window.
#![allow(dead_code)]

use labwired_config::{ChipDescriptor, SystemManifest};
use labwired_core::bus::SystemBus;
use labwired_core::memory::ProgramImage;
use labwired_core::system::cortex_m::configure_cortex_m;
use labwired_core::{Bus, Cpu, Machine, Peripheral, SimResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct Access {
    step: u64,
    pc: u32,
    addr: u64,
    write: bool,
    val: u32,
    size: u8,
}

#[derive(Debug, Default)]
struct Shared {
    step: u64,
    pc: u32,
    log: Vec<Access>,
    dropped: u64,
}

#[derive(Debug)]
struct Logger {
    base: u64,
    regs: Mutex<HashMap<u64, u32>>,
    sh: Arc<Mutex<Shared>>,
}

impl Logger {
    fn rec(&self, off: u64, write: bool, val: u32, size: u8) {
        let mut s = self.sh.lock().unwrap();
        let a = Access {
            step: s.step,
            pc: s.pc,
            addr: self.base + off,
            write,
            val,
            size,
        };
        if s.log.len() < 1_500_000 {
            s.log.push(a);
        } else {
            let i = 1_500_000 + (s.dropped % 200_000) as usize;
            if s.log.len() <= i {
                s.log.push(a);
            } else {
                s.log[i] = a;
            }
            s.dropped += 1;
        }
    }
    fn get(&self, off: u64) -> u32 {
        let a = (self.base + off) & !3;
        if let Some(v) = force().get(&a) {
            return (*self.regs.lock().unwrap().get(&(off & !3)).unwrap_or(&0) & !v.0) | v.1;
        }
        *self.regs.lock().unwrap().get(&(off & !3)).unwrap_or(&0)
    }
    fn set(&self, off: u64, v: u32) {
        self.regs.lock().unwrap().insert(off & !3, v);
    }
}

impl Peripheral for Logger {
    fn read(&self, offset: u64) -> SimResult<u8> {
        let w = self.get(offset);
        self.rec(offset, false, w, 1);
        Ok((w >> ((offset & 3) * 8)) as u8)
    }
    fn write(&mut self, offset: u64, value: u8) -> SimResult<()> {
        let sh = ((offset & 3) * 8) as u32;
        let w = (self.get(offset) & !(0xFF << sh)) | ((value as u32) << sh);
        self.set(offset, w);
        self.rec(offset, true, value as u32, 1);
        Ok(())
    }
    fn read_u16(&self, offset: u64) -> SimResult<u16> {
        let w = self.get(offset);
        self.rec(offset, false, w, 2);
        Ok((w >> ((offset & 3) * 8)) as u16)
    }
    fn write_u16(&mut self, offset: u64, value: u16) -> SimResult<()> {
        let sh = ((offset & 3) * 8) as u32;
        let w = (self.get(offset) & !(0xFFFF << sh)) | ((value as u32) << sh);
        self.set(offset, w);
        self.rec(offset, true, value as u32, 2);
        Ok(())
    }
    fn read_u32(&self, offset: u64) -> SimResult<u32> {
        let w = self.get(offset);
        self.rec(offset, false, w, 4);
        Ok(w)
    }
    fn write_u32(&mut self, offset: u64, value: u32) -> SimResult<()> {
        self.set(offset, value);
        self.rec(offset, true, value, 4);
        Ok(())
    }
    fn needs_legacy_walk(&self) -> bool {
        false
    }
}

/// FORCE="addr:mask:val,..." — discovery-only read overrides.
fn force() -> &'static HashMap<u64, (u32, u32)> {
    static F: std::sync::OnceLock<HashMap<u64, (u32, u32)>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let mut m = HashMap::new();
        for e in std::env::var("FORCE").unwrap_or_default().split(',') {
            let p: Vec<_> = e.split(':').collect();
            if p.len() == 3 {
                let h = |s: &str| u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
                m.insert(h(p[0]), (h(p[1]) as u32, h(p[2]) as u32));
            }
        }
        m
    })
}

fn parse_mr(bytes: &[u8]) -> Vec<Vec<u8>> {
    assert_eq!(&bytes[0..9], b"Mooer_TAG");
    let n = bytes[47] as usize;
    let mut off = 128;
    let mut out = Vec::new();
    for _ in 0..n {
        assert_eq!(&bytes[off..off + 9], b"Mooer_TAG");
        let size = u32::from_le_bytes(bytes[off + 17..off + 21].try_into().unwrap()) as usize;
        off += 512;
        out.push(bytes[off..off + size].to_vec());
        off += size;
    }
    out
}

#[test]
#[ignore]
fn discover() {
    let mr = std::env::var("MR").expect("MR");
    let max: u64 = std::env::var("MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000_000);
    let blocks = parse_mr(&std::fs::read(mr).unwrap());
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let chip_path = root.join("configs/chips/mimxrt1062.yaml");
    let chip = ChipDescriptor::from_file(&chip_path).unwrap();
    let manifest: SystemManifest =
        serde_yaml::from_str(&format!("name: disc\nchip: {}\n", chip_path.display())).unwrap();
    let mut bus = SystemBus::from_config(&chip, &manifest).unwrap();
    let sh = Arc::new(Mutex::new(Shared::default()));
    for (name, base, size) in [
        ("log_aips1", 0x4000_0000u64, 0x0040_0000u64),
        ("log_aips5", 0x4200_0000, 0x0010_0000),
        ("log_ppb_misc", 0xE004_0000, 0x000C_0000),
    ] {
        bus.add_peripheral(
            name,
            base,
            size,
            None,
            Box::new(Logger {
                base,
                regs: Mutex::new(HashMap::new()),
                sh: sh.clone(),
            }),
        );
    }
    if let Ok(mv) = std::env::var("ADC9") {
        let idx = bus.find_peripheral_index_by_name("adc1").unwrap();
        bus.peripherals[idx]
            .dev
            .set_adc_channel_input(9, mv.parse().unwrap());
    }
    let (mut cpu, _nvic) = configure_cortex_m(&mut bus);
    cpu.set_faults_enabled(false);
    let mut m = Machine::new(cpu, bus);
    let mut img = ProgramImage::new(0x6001_0000, labwired_core::Arch::Arm);
    img.add_segment(0x6001_0000, blocks[0].clone());
    if std::env::var("MODELS").is_ok() {
        img.add_segment(0x6004_1000, blocks[1].clone());
    }
    m.load_firmware(&img).unwrap();
    eprintln!("start pc={:#x} sp={:#x}", m.cpu.get_pc(), m.cpu.get_register(13));
    let mut pcs: HashMap<u32, u64> = HashMap::new();
    let mut last_pcs = std::collections::VecDeque::new();
    let mut err = None;
    let mut step = 0u64;
    let marks: Vec<u32> = std::env::var("MARKS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect();
    while step < max {
        let pc = m.cpu.get_pc();
        {
            let mut s = sh.lock().unwrap();
            s.step = step;
            s.pc = pc;
        }
        if marks.contains(&pc) && !pcs.contains_key(&pc) {
            eprintln!("MARK {pc:#x} at step {step}");
        }
        *pcs.entry(pc).or_default() += 1;
        last_pcs.push_back(pc);
        if last_pcs.len() > 64 {
            last_pcs.pop_front();
        }
        if let Err(e) = m.step() {
            err = Some(e);
            break;
        }
        step += 1;
    }
    eprintln!("steps={step} pc={:#x} err={err:?}", m.cpu.get_pc());
    for i in 0..16u8 {
        eprint!("r{i}={:#x} ", m.cpu.get_register(i));
    }
    eprintln!();
    eprintln!(
        "last pcs: {:x?}",
        last_pcs.iter().collect::<Vec<_>>()
    );
    let s = sh.lock().unwrap();
    // unique per register summary in order of first access
    let mut seen: HashMap<u64, (usize, u64, u32, u64, u64)> = HashMap::new();
    let mut order = Vec::new();
    for (i, a) in s.log.iter().enumerate() {
        let e = seen.entry(a.addr).or_insert_with(|| {
            order.push(a.addr);
            (i, 0, a.pc, a.step, 0)
        });
        e.1 += 1;
        e.4 = a.step;
    }
    let out = std::env::var("OUT").unwrap_or("/tmp/disc".into());
    let mut f = String::new();
    let mut logv = s.log.clone();
    logv.sort_by_key(|a| a.step);
    for a in &logv {
        f.push_str(&format!(
            "{} {:08x} {} {:08x} {:08x} {}\n",
            a.step,
            a.pc,
            if a.write { "W" } else { "R" },
            a.addr,
            a.val,
            a.size
        ));
    }
    std::fs::write(format!("{out}.log"), f).unwrap();
    let mut g = String::new();
    for addr in order {
        let e = seen[&addr];
        g.push_str(&format!(
            "{addr:08x} n={} first_pc={:08x} first_step={} last_step={}\n",
            e.1, e.2, e.3, e.4
        ));
    }
    std::fs::write(format!("{out}.regs"), g).unwrap();
    // hottest pcs
    let mut hot: Vec<_> = pcs.into_iter().collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1));
    let h: Vec<String> = hot.iter().take(40).map(|(p, n)| format!("{p:x}:{n}")).collect();
    eprintln!("hot: {}", h.join(" "));
    // Dump memory for disassembly
    if let Ok(d) = std::env::var("DUMP") {
        for (name, base, len) in [
            ("itcm", 0u64, 0x20000usize),
            ("dtcm", 0x2000_0000, 0x58000),
            ("ocram", 0x2020_0000, 0x10000),
        ] {
            let mut v = Vec::with_capacity(len);
            for i in 0..len as u64 {
                v.push(m.bus.read_u8(base + i).unwrap_or(0));
            }
            std::fs::write(format!("{d}/{name}.bin"), v).unwrap();
        }
    }
}
