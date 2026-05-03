use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn rom_dir() -> PathBuf {
    PathBuf::from(env!("HOME")).join("Documents/retro/amiga_winuae/rom")
}

#[test]
fn dump_kickstart_13_frame_150_to_ppm() {
    let rom_file = rom_dir().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    for _ in 0..150 {
        emu.run_frame();
    }

    let fb = emu.framebuffer();
    let non_zero = fb.iter().filter(|&&p| p != 0).count();
    println!("Frame 150: {non_zero}/{} non-zero pixels", fb.len());

    // Count unique colors
    let mut colors: std::collections::BTreeSet<u16> = fb.iter().copied().collect();
    println!("Unique colors: {}", colors.len());
    colors.remove(&0);
    let top_colors: Vec<String> = colors.iter().take(10).map(|c| format!("{c:04X}")).collect();
    println!("Non-black colors: {}", top_colors.join(", "));

    // Write PPM
    let mut f = fs::File::create("/tmp/rumiga_frame.ppm").unwrap();
    writeln!(f, "P3\n320 256\n255").unwrap();
    for pixel in fb {
        let r = ((pixel >> 11) & 0x1F) * 255 / 31;
        let g = ((pixel >> 5) & 0x3F) * 255 / 63;
        let b = (pixel & 0x1F) * 255 / 31;
        writeln!(f, "{r} {g} {b}").unwrap();
    }
    println!("Wrote /tmp/rumiga_frame.ppm - open with: open /tmp/rumiga_frame.ppm");

    assert!(non_zero > 0, "Should have visible pixels");
}
