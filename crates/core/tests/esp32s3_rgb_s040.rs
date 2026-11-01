//! S040 functional qualification. Positive and negative controls use actual
//! MMIO/GDMA and public artifacts; the final test executes the owned Xtensa
//! ELF, including GT911-driven guest redraw. No panel setters paint the image.
use labwired_config::SystemManifest;
use labwired_core::bus::SystemBus;
use labwired_core::cpu::XtensaLx7;
use labwired_core::inspect::{Artifact, InspectOpts};
use labwired_core::system::xtensa::{
    attach_esp32_external_devices, configure_xtensa_esp32s3, Esp32s3Opts,
};
use labwired_core::{Bus, Cpu, Machine};

const GPIO: u64 = 0x60004000;
const LCD: u64 = 0x60041000;
const DMA: u64 = 0x6003f000;
const DESC: u64 = 0x3fc88000;
const FRAME: u64 = 0x3fc8a000;
// The owned guest uses smaller repeated-row buffers; RESULT is its mailbox.
const RESULT: u64 = 0x3fc91800;
const USER: u32 = (1 << 23) | (1 << 24) | (1 << 27) | (1 << 13);

fn rig() -> Machine<XtensaLx7> {
    rig_with_opts(Esp32s3Opts::default())
}

fn rig_with_opts(opts: Esp32s3Opts) -> Machine<XtensaLx7> {
    let mut bus = SystemBus::new();
    let mut wiring = configure_xtensa_esp32s3(&mut bus, &opts);
    let system: SystemManifest =
        serde_yaml::from_str(include_str!("../../../configs/systems/esp32-4848s040.yaml")).unwrap();
    attach_esp32_external_devices(&mut bus, &system).unwrap();
    wiring.cpu.halted = true;
    Machine::new(wiring.cpu, bus)
}

fn artifact(m: &Machine<XtensaLx7>) -> Artifact {
    let mut artifacts = m.bus.display_artifacts_of_format(
        &[labwired_core::inspect::artifact_format::RGB565_BE],
        &InspectOpts {
            include_bytes: true,
            peripheral: None,
        },
    );
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].id, "display");
    assert_eq!(artifacts[0].meta["w"], 480);
    assert_eq!(artifacts[0].meta["h"], 480);
    artifacts.remove(0)
}

fn pin(m: &mut Machine<XtensaLx7>, number: u8, high: bool) {
    m.bus
        .write_u32(GPIO + if high { 0x14 } else { 0x18 }, 1 << (number - 32))
        .unwrap();
}

fn serial(m: &mut Machine<XtensaLx7>, word: u16) {
    pin(m, 39, false);
    for bit in (0..9).rev() {
        pin(m, 48, false);
        pin(m, 47, word & (1 << bit) != 0);
        pin(m, 48, true);
    }
    pin(m, 48, false);
    pin(m, 39, true);
}

fn init_panel(m: &mut Machine<XtensaLx7>) {
    m.bus
        .write_u32(GPIO + 0x30, (1 << 6) | (1 << 7) | (1 << 15) | (1 << 16))
        .unwrap();
    pin(m, 39, true);
    pin(m, 38, true);
    for word in [0x3a, 0x160, 0x11, 0x29] {
        serial(m, word);
    }
}

fn mount(m: &mut Machine<XtensaLx7>, word: u16, chunk: usize, circular: bool) {
    // Entire active region, including independently checked asymmetric
    // corners and row-boundary sentinels, reaches DMA through real RAM.
    for i in 0..480 * 480 {
        m.bus.write_u16(FRAME + i * 2, word).unwrap();
    }
    for (i, value) in [
        (0, 0xf800),
        (479, 0x07e0),
        (480, 0x1234),
        (480 * 479, 0x001f),
        (480 * 480 - 1, 0xffff),
    ] {
        m.bus.write_u16(FRAME + i * 2, value).unwrap();
    }
    for (i, start) in (0..480 * 480 * 2).step_by(chunk).enumerate() {
        let len = chunk.min(480 * 480 * 2 - start);
        let last = start + len == 480 * 480 * 2;
        let next = if last {
            if circular {
                DESC
            } else {
                0
            }
        } else {
            DESC + (i as u64 + 1) * 12
        };
        m.bus
            .write_u32(
                DESC + i as u64 * 12,
                (1 << 31) | (u32::from(last) << 30) | ((len as u32) << 12) | len as u32,
            )
            .unwrap();
        m.bus
            .write_u32(DESC + i as u64 * 12 + 4, FRAME as u32 + start as u32)
            .unwrap();
        m.bus
            .write_u32(DESC + i as u64 * 12 + 8, next as u32)
            .unwrap();
    }
    m.bus
        .write_u32(LCD + 0x1c, (1 << 31) | (479 << 11) | (517 << 21) | 57)
        .unwrap();
    m.bus
        .write_u32(LCD + 0x20, (547 << 20) | (479 << 8) | 27)
        .unwrap();
    m.bus.write_u32(LCD + 0x64, 1).unwrap();
    m.bus
        .write_u32(LCD + 0x18, if circular { 1 << 25 } else { 0 })
        .unwrap();
    m.bus.write_u32(DMA + 0xa8, 5).unwrap();
    m.bus
        .write_u32(DMA + 0x80, (DESC as u32 & 0xfffff) | (1 << 21))
        .unwrap();
}

fn await_vsync(m: &mut Machine<XtensaLx7>) {
    for _ in 0..600 {
        m.step().unwrap();
        if m.bus.read_u32(LCD + 0x6c).unwrap() & 1 != 0 {
            return;
        }
    }
    panic!("complete RGB frame never raised enabled VSYNC");
}

fn pixel(a: &Artifact, index: usize) -> u16 {
    let b = a.bytes.as_ref().unwrap();
    u16::from_be_bytes([b[index * 2], b[index * 2 + 1]])
}

#[test]
fn rgb565_geometry_stride_odd_descriptor_and_vsync_ack() {
    let mut m = rig();
    init_panel(&mut m);
    mount(&mut m, 0x2104, 4093, false);
    // DMA armed before LCD_START must stall without producing an event/frame.
    for _ in 0..5 {
        m.step().unwrap();
    }
    assert_eq!(artifact(&m).meta["frames_received"], 0);
    m.bus.write_u32(LCD + 0x14, USER).unwrap();
    await_vsync(&mut m);
    let a = artifact(&m);
    for (i, expected) in [
        (0, 0xf800),
        (479, 0x07e0),
        (480, 0x1234),
        (481, 0x2104),
        (480 * 479, 0x001f),
        (480 * 480 - 1, 0xffff),
    ] {
        assert_eq!(pixel(&a, i), expected, "pixel {i}");
    }
    assert_eq!(a.meta["frames_received"], 1);
    assert_eq!(m.bus.read_u32(LCD + 0x14).unwrap() & (1 << 27), 0);
    m.bus.write_u32(LCD + 0x70, 1).unwrap();
    assert_eq!(m.bus.read_u32(LCD + 0x6c).unwrap() & 1, 0);
    for _ in 0..5 {
        m.step().unwrap();
    }
    assert_eq!(artifact(&m).meta["frames_received"], 1);
}

#[test]
fn cyclic_frames_resample_memory_and_byte_order_is_applied_once() {
    let mut m = rig();
    init_panel(&mut m);
    mount(&mut m, 0x2104, 4092, true);
    m.bus.write_u32(LCD + 0x14, USER | (1 << 22)).unwrap();
    await_vsync(&mut m);
    assert_eq!(pixel(&artifact(&m), 480), 0x3412);
    assert_ne!(m.bus.read_u32(LCD + 0x14).unwrap() & (1 << 27), 0);
    m.bus.write_u16(FRAME, 0x5aa5).unwrap();
    m.bus.write_u32(LCD + 0x70, 1).unwrap();
    await_vsync(&mut m);
    let a = artifact(&m);
    assert_eq!(a.meta["frames_received"], 2);
    assert_eq!(pixel(&a, 0), 0xa55a);
    // Explicit stop prevents later ring data from making another frame.
    m.bus.write_u32(LCD + 0x14, USER & !(1 << 27)).unwrap();
    for _ in 0..150 {
        m.step().unwrap();
    }
    assert_eq!(artifact(&m).meta["frames_received"], 2);
}

#[test]
fn missing_short_cpu_owned_and_wrong_format_never_complete_a_frame() {
    for bad in ["short", "owner", "length", "8bit", "no_dma"] {
        let mut m = rig();
        init_panel(&mut m);
        mount(&mut m, 0x07e0, 4092, false);
        match bad {
            "short" => m.bus.write_u32(DESC + 8, 0).unwrap(),
            "owner" => {
                let v = m.bus.read_u32(DESC).unwrap();
                m.bus.write_u32(DESC, v & !(1 << 31)).unwrap();
            }
            "length" => m.bus.write_u32(DESC, (1 << 31) | (4092 << 12) | 4).unwrap(),
            "no_dma" => m.bus.write_u32(DMA + 0x80, 1 << 20).unwrap(),
            _ => {}
        }
        let user = if bad == "8bit" {
            USER & !(1 << 23)
        } else {
            USER
        };
        m.bus.write_u32(LCD + 0x14, user).unwrap();
        for _ in 0..160 {
            m.step().unwrap();
        }
        assert_eq!(artifact(&m).meta["frames_received"], 0, "{bad}");
        assert_eq!(m.bus.read_u32(LCD + 0x68).unwrap() & 1, 0, "{bad}");
        if bad == "owner" || bad == "length" {
            assert_ne!(m.bus.read_u32(DMA + 0x68).unwrap() & 4, 0);
        }
    }
}

#[test]
fn control_sleep_backlight_and_software_reset_gate_the_visible_frame() {
    let mut m = rig();
    init_panel(&mut m);
    mount(&mut m, 0x07e0, 4092, false);
    m.bus.write_u32(LCD + 0x14, USER).unwrap();
    await_vsync(&mut m);
    assert_eq!(pixel(&artifact(&m), 0), 0xf800);
    pin(&mut m, 38, false);
    assert_eq!(pixel(&artifact(&m), 0), 0);
    pin(&mut m, 38, true);
    assert_eq!(pixel(&artifact(&m), 0), 0xf800);
    serial(&mut m, 0x10);
    assert_eq!(pixel(&artifact(&m), 0), 0);
    serial(&mut m, 0x11);
    assert_eq!(pixel(&artifact(&m), 0), 0xf800);
    serial(&mut m, 0x28);
    assert_eq!(pixel(&artifact(&m), 0), 0);
    serial(&mut m, 0x29);
    assert_eq!(pixel(&artifact(&m), 0), 0xf800);
    serial(&mut m, 0x01);
    let a = artifact(&m);
    assert_eq!(pixel(&a, 0), 0);
    assert_eq!(a.meta["display_on"], false);
    assert_eq!(a.meta["frames_received"], 0);
}

#[test]
fn rgb_dma_reads_the_existing_mmu_mapped_octal_psram() {
    let mut m = rig_with_opts(Esp32s3Opts {
        real_reset_boot: true,
        ..Esp32s3Opts::default()
    });
    init_panel(&mut m);
    mount(&mut m, 0x2a55, 4092, false);
    // Eight real MMU pages map the frame to the existing 8MiB PSRAM array.
    for page in 0..8 {
        m.bus
            .write_u32(0x600c5000 + page * 4, (1 << 15) | page as u32)
            .unwrap();
    }
    for offset in (0..480 * 480 * 2).step_by(4) {
        let value = m.bus.read_u32(FRAME + offset).unwrap();
        m.bus.write_u32(0x3c000000 + offset, value).unwrap();
    }
    for (i, start) in (0..480 * 480 * 2).step_by(4092).enumerate() {
        m.bus
            .write_u32(DESC + i as u64 * 12 + 4, 0x3c000000 + start as u32)
            .unwrap();
    }
    m.bus.write_u32(LCD + 0x14, USER).unwrap();
    await_vsync(&mut m);
    let a = artifact(&m);
    assert_eq!(pixel(&a, 0), 0xf800);
    assert_eq!(pixel(&a, 480), 0x1234);
    assert_eq!(pixel(&a, 481), 0x2a55);
    assert_eq!(pixel(&a, 480 * 480 - 1), 0xffff);
}

#[test]
fn reset_discards_partial_frame_and_software_rotated_corners_keep_scan_order() {
    let mut m = rig();
    init_panel(&mut m);
    mount(&mut m, 0, 4092, false);
    m.bus.write_u32(LCD + 0x14, USER).unwrap();
    m.step().unwrap();
    m.bus.write_u32(LCD + 0x14, 1 << 28).unwrap();
    for _ in 0..150 {
        m.step().unwrap();
    }
    assert_eq!(artifact(&m).meta["frames_received"], 0);
    // A guest software rotation places corners in raster memory itself.
    // The RGB receiver must not invent MADCTL rotation or reverse row stride.
    for corners in [
        [0xf800, 0x07e0, 0x001f, 0xffff],
        [0x001f, 0xf800, 0xffff, 0x07e0],
        [0xffff, 0x001f, 0x07e0, 0xf800],
        [0x07e0, 0xffff, 0xf800, 0x001f],
    ] {
        for (i, value) in [0, 479, 480 * 479, 480 * 480 - 1].into_iter().zip(corners) {
            m.bus.write_u16(FRAME + i * 2, value).unwrap();
        }
        m.bus
            .write_u32(DMA + 0x80, (DESC as u32 & 0xfffff) | (1 << 21))
            .unwrap();
        m.bus.write_u32(LCD + 0x70, 3).unwrap();
        m.bus.write_u32(LCD + 0x14, USER).unwrap();
        await_vsync(&mut m);
        let a = artifact(&m);
        for (i, value) in [0, 479, 480 * 479, 480 * 480 - 1].into_iter().zip(corners) {
            assert_eq!(pixel(&a, i), value);
        }
    }
}

fn guest_until(m: &mut Machine<XtensaLx7>, count: u32) {
    for _ in 0..100_000 {
        m.step()
            .unwrap_or_else(|e| panic!("guest at {:#x}: {e}", m.cpu.get_pc()));
        if m.bus.read_u32(RESULT).unwrap() >= count {
            return;
        }
    }
    panic!(
        "guest did not produce report {count}; PC={:#x}, result={}, i2c raw={:#x}",
        m.cpu.get_pc(),
        m.bus.read_u32(RESULT).unwrap(),
        m.bus.read_u32(0x60013020).unwrap()
    );
}

#[test]
fn owned_xtensa_guest_reads_gt911_and_redraws_on_down_move_up() {
    let mut m = rig();
    let image = labwired_loader::load_elf_bytes(include_bytes!(
        "../../../tests/fixtures/esp32-4848s040/probe.elf"
    ))
    .unwrap();
    m.load_firmware(&image).unwrap();
    // This is an independently authored bare ELF entry, not a ROM-reset boot.
    m.cpu.set_pc(image.entry_point as u32);
    m.cpu.halted = false;
    guest_until(&mut m, 1);
    let a = artifact(&m);
    for (i, color) in [
        (0, 0xf800),
        (479, 0x07e0),
        (480 * 479, 0x001f),
        (480 * 480 - 1, 0xffff),
    ] {
        assert_eq!(pixel(&a, i), color);
    }
    let inputs = m.list_inputs();
    for key in ["x", "y", "pressed"] {
        assert!(inputs.iter().any(|(id, c)| id == "touch" && c.key == key));
    }
    m.set_inputs(&[
        (Some("touch"), "x", 123.0),
        (Some("touch"), "y", 456.0),
        (Some("touch"), "pressed", 1.0),
    ])
    .unwrap();
    guest_until(&mut m, 2);
    assert_eq!(pixel(&artifact(&m), 0), 123);
    assert_eq!(m.bus.read_u16(RESULT + 10).unwrap(), 123);
    assert_eq!(m.bus.read_u16(RESULT + 12).unwrap(), 456);
    m.set_inputs(&[(Some("touch"), "x", 321.0), (Some("touch"), "y", 47.0)])
        .unwrap();
    guest_until(&mut m, 3);
    assert_eq!(pixel(&artifact(&m), 0), 321);
    m.set_input_on("touch", "pressed", 0.0).unwrap();
    guest_until(&mut m, 4);
    assert_eq!(pixel(&artifact(&m), 0), 0xf800);
    assert_eq!(m.bus.read_u8(RESULT + 8).unwrap(), 0x80);
}

#[test]
fn h1_bank_one_pads_have_real_driver_input_release_and_capture_without_aliases() {
    use labwired_core::logic_capture::LogicSource;
    use labwired_core::pins::{External, Out, Pull};
    for pin in [40u8, 43, 44] {
        let mut m = rig();
        // FUNC1 selects GPIO on these H1 pads. Their FUNC0 routes are
        // MTDO/U0TXD/U0RXD; the shared model stores MCU_SEL but does not yet
        // enforce that arbitration, so use the actual GPIO configuration.
        m.bus
            .write_u32(0x60009004 + u64::from(pin) * 4, 0x1b00)
            .unwrap();
        let idx = m.bus.find_peripheral_index_by_name("gpio").unwrap();
        m.logic_watch(&[
            Some(LogicSource::pad(idx, pin)),
            Some(LogicSource::driver(idx, pin)),
        ]);
        let mask = 1u32 << (pin - 32);
        m.bus.write_u32(GPIO + 0x30, mask).unwrap();
        m.step().unwrap();
        m.bus.write_u32(GPIO + 0x14, mask).unwrap();
        m.step().unwrap();
        {
            let gpio = &m.bus.peripherals[idx].dev;
            assert_eq!(gpio.read_gpio_output(pin), Some(true));
            assert_eq!(
                gpio.read_gpio_output(pin - 32),
                Some(false),
                "no low-bank alias for {pin}"
            );
            assert_eq!(gpio.read_gpio_pad(pin), Some(true));
            assert_eq!(gpio.pins().unwrap().driver(pin).unwrap().out, Out::High);
        }
        assert_eq!(m.bus.read_u32(GPIO + 4).unwrap(), 0, "OUT0 unchanged");
        m.bus.write_u32(GPIO + 0x34, mask).unwrap();
        m.step().unwrap();
        assert_eq!(
            m.bus.peripherals[idx]
                .dev
                .pins()
                .unwrap()
                .driver(pin)
                .unwrap()
                .out,
            Out::Off
        );
        // Existing IO_MUX pullup semantics apply equally to IN1 inputs.
        m.bus.set_pad_external(idx, pin, External::Level(false));
        m.step().unwrap();
        assert_eq!(m.bus.peripherals[idx].dev.read_gpio_input(pin), Some(false));
        assert_eq!(m.bus.read_u32(GPIO + 0x40).unwrap() & mask, 0);
        m.bus.set_pad_external(idx, pin, External::Released);
        m.step().unwrap();
        assert_eq!(m.bus.peripherals[idx].dev.read_gpio_input(pin), Some(true));
        assert_eq!(
            m.bus.peripherals[idx]
                .dev
                .pins()
                .unwrap()
                .driver(pin)
                .unwrap()
                .pull,
            Pull::Up
        );
        // Open-drain high releases the line so an external low wins.
        m.bus
            .write_u32(GPIO + 0x74 + u64::from(pin) * 4, 1 << 2)
            .unwrap();
        m.bus.write_u32(GPIO + 0x30, mask).unwrap();
        m.bus.set_pad_external(idx, pin, External::Level(false));
        m.step().unwrap();
        assert_eq!(
            m.bus.peripherals[idx]
                .dev
                .pins()
                .unwrap()
                .driver(pin)
                .unwrap()
                .out,
            Out::Off
        );
        assert_eq!(m.bus.peripherals[idx].dev.read_gpio_pad(pin), Some(false));
        let edges = m.logic_read_edges(0).edges;
        assert!(edges.iter().any(|e| e.ch == 0 && e.value));
        assert!(edges.iter().any(|e| e.ch == 0 && !e.value));
        assert!(
            !m.logic_read_states(0).edges.is_empty(),
            "direction/release capture"
        );
        for absent in [22, 23, 24, 25, 49, 50, 51, 52, 53, 255] {
            let gpio = &mut m.bus.peripherals[idx].dev;
            assert_eq!(gpio.read_gpio_pad(absent), None);
            assert_eq!(gpio.pins().unwrap().driver(absent), None);
            assert_eq!(
                gpio.pins_mut()
                    .unwrap()
                    .set_external(absent, External::Level(true)),
                None
            );
        }
    }
}
