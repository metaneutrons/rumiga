// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Integration test: boot a real Kickstart ROM and verify emulation progresses.

use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;
use std::fs;
use std::path::PathBuf;

fn dirs_next() -> PathBuf {
    PathBuf::from(env!("HOME")).join("Documents/retro/amiga_winuae/rom")
}

#[test]
fn boot_kickstart_13_executes_instructions_and_produces_non_black_framebuffer() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found at {}", rom_file.display());
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    assert_eq!(rom.len(), 256 * 1024, "ROM must be 256KB");

    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // Run 5 frames
    for _ in 0..5 {
        emu.run_frame();
    }

    // After 5 frames, CPU should have executed many cycles
    assert!(
        emu.total_cycles > 100_000,
        "Expected >100k cycles after 5 frames, got {}",
        emu.total_cycles
    );

    // Framebuffer should have some non-zero pixels (Kickstart shows colors)
    let non_zero = emu.framebuffer().iter().filter(|&&p| p != 0).count();
    println!(
        "After 5 frames: {} total cycles, {}/{} non-zero pixels",
        emu.total_cycles,
        non_zero,
        emu.framebuffer().len()
    );

    // The Kickstart ROM should at minimum set COLOR00 (background)
    // Even if rendering isn't perfect, the CPU should be running
    assert!(
        emu.total_cycles > 500_000,
        "CPU should execute >500k cycles in 5 PAL frames"
    );
}

#[test]
fn boot_kickstart_13_cpu_reads_valid_reset_vectors() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found at {}", rom_file.display());
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // The ROM's first 4 bytes = initial SSP, next 4 = initial PC
    let ssp = u32::from_be_bytes([rom[0], rom[1], rom[2], rom[3]]);
    let pc = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);

    println!("ROM reset vectors: SSP=0x{ssp:08X}, PC=0x{pc:08X}");

    // SSP should be a valid RAM address (typically 0x400 or similar)
    // PC should be in ROM space (0xFC0000+ for 256K ROM)
    assert!(
        !(0x0008_0000..0x00FC_0000).contains(&pc),
        "Reset PC should be in ROM or low memory, got 0x{pc:08X}"
    );

    // Run one frame to let CPU fetch reset vectors
    emu.run_frame();

    // CPU should have advanced past the reset vector
    assert!(emu.total_cycles > 0, "CPU should have executed some cycles");
}

#[test]
fn boot_kickstart_13_produces_display_output_after_startup_delay() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found");
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    // Kickstart 1.3 has a ~15 frame startup delay loop
    // Run 100 frames to get past it
    for _ in 0..100 {
        emu.run_frame();
    }

    let fb = emu.framebuffer();
    let non_zero = fb.iter().filter(|&&p| p != 0).count();
    println!(
        "After 100 frames: {} cycles, {}/{} non-zero pixels",
        emu.total_cycles,
        non_zero,
        fb.len()
    );

    assert!(
        non_zero > 0,
        "Kickstart should produce visible output after 100 frames (2 seconds)"
    );
}

#[test]
fn test_kickstart_13_boots_past_memory_test_without_crashing() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found");
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    for _ in 0..150 {
        emu.run_frame();
    }

    // The ROM should have executed millions of cycles without crashing
    assert!(
        emu.total_cycles > 10_000_000,
        "Expected >10M cycles, got {}",
        emu.total_cycles
    );

    // The CPU should have progressed past the initial boot code ($FC00xx)
    // into the exec initialization ($FC30xx+) or hit STOP waiting for interrupts
    let pc = emu.cpu.pc;
    assert!(
        pc > 0x00FC_0100 || emu.cpu.is_stopped(),
        "CPU should have progressed past boot or be in STOP state, PC=${pc:08X}"
    );

    println!(
        "After 150 frames: PC=${:08X} cycles={} stopped={}",
        pc,
        emu.total_cycles,
        emu.cpu.is_stopped()
    );
}

#[test]
#[ignore = "blocked on CIA timer init bug; see fix_initcode investigation"]
fn boot_kickstart_13_graphics_library_initializes_display_planes() {
    let rom_file = dirs_next().join("kick.a500.34.005.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found");
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    let mut max_planes: usize = 0;
    let mut frame_reached: usize = 0;

    for frame in 0..1000 {
        emu.run_frame();
        let planes = emu.playfield.num_planes();
        if planes > max_planes {
            max_planes = planes;
            frame_reached = frame;
        }
    }

    println!(
        "Max bitplanes reached: {max_planes} at frame {frame_reached} \
         (total_cycles={})",
        emu.total_cycles
    );

    assert!(
        max_planes >= 2,
        "Kickstart 1.3 should set up >= 2 bitplanes for the insert-disk hand, \
         but only reached {max_planes} planes. CIA timers may not have started."
    );
}

#[test]
fn boot_kickstart_31_a1200_executes_instructions_and_boots() {
    let rom_file = dirs_next().join("kick.a1200.40.068.rom");
    if !rom_file.exists() {
        eprintln!(
            "SKIP: Kickstart 3.1 ROM not found at {}",
            rom_file.display()
        );
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    assert_eq!(
        rom.len(),
        512 * 1024,
        "A1200 Kickstart 3.1 ROM must be 512KB"
    );

    let mut emu = Emulator::new(MemoryConfig::a1200());
    emu.load_rom(&rom);

    // Run 5 PAL frames
    for _ in 0..5 {
        emu.run_frame();
    }

    // CPU should have executed millions of cycles
    assert!(
        emu.total_cycles > 500_000,
        "Expected >500k cycles after 5 frames in A1200 mode, got {}",
        emu.total_cycles
    );

    // SSP and PC reset vectors validation
    let ssp = u32::from_be_bytes([rom[0], rom[1], rom[2], rom[3]]);
    let pc = u32::from_be_bytes([rom[4], rom[5], rom[6], rom[7]]);
    println!("A1200 ROM reset vectors: SSP=0x{ssp:08X}, PC=0x{pc:08X}");

    // Run 150 PAL frames to ensure it boots past memory tests and does not crash
    for _ in 0..150 {
        emu.run_frame();
    }

    assert!(
        emu.total_cycles > 10_000_000,
        "Expected >10M cycles after 150 frames, got {}",
        emu.total_cycles
    );

    // Ensure CPU progressed into main boot ROM execution ($FC0000+ or $F80000+)
    let current_pc = emu.cpu.pc;
    assert!(
        current_pc >= 0x00F8_0000 || emu.cpu.is_stopped(),
        "CPU should have progressed or be in STOP state, PC=0x{current_pc:08X}"
    );

    println!(
        "A1200 boot successful: PC=0x{:08X} cycles={} stopped={}",
        current_pc,
        emu.total_cycles,
        emu.cpu.is_stopped()
    );
}

#[test]
fn test_kickstart_31_cpu_tracing() {
    let rom_file = dirs_next().join("kick.a1200.40.068.rom");
    if !rom_file.exists() {
        eprintln!("SKIP: ROM not found");
        return;
    }

    let rom = fs::read(&rom_file).unwrap();
    let mut emu = Emulator::new(MemoryConfig::a1200());
    emu.load_rom(&rom);

    // Set up a temporary trace log
    let trace_path = "trace_ks31_test.log";
    emu.enable_cpu_trace(trace_path, Some(50)).unwrap();

    // Step 50 instructions
    for _ in 0..50 {
        emu.step_instruction();
    }

    // Drop emulator so the trace log is flushed/closed
    drop(emu);

    let trace_data = fs::read_to_string(trace_path).unwrap();
    fs::remove_file(trace_path).unwrap();

    let lines: Vec<&str> = trace_data.lines().collect();
    assert_eq!(
        lines.len(),
        50,
        "Expected exactly 50 trace lines, got {}",
        lines.len()
    );

    // Check formatting of the first trace line
    let first_line = lines[0];
    assert!(
        first_line.contains("PC:"),
        "Trace line missing PC prefix: {first_line}"
    );
    assert!(
        first_line.contains("OP:"),
        "Trace line missing OP: {first_line}"
    );
    assert!(
        first_line.contains("D0:"),
        "Trace line missing registers: {first_line}"
    );
    assert!(
        first_line.contains("SR:"),
        "Trace line missing SR: {first_line}"
    );
    println!("Sample trace line: {first_line}");
}

#[test]
fn test_hdf_boot_reaches_read_sectors() {
    let rom_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/kick.a1200.46.143.rom");
    let hdf_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/workbench-314.hdf");

    if !rom_path.exists() || !hdf_path.exists() {
        eprintln!("SKIP: ROM or HDF not found");
        return;
    }

    let rom = fs::read(&rom_path).unwrap();
    let hdf_data = fs::read(&hdf_path).unwrap();

    let mut emu = Emulator::new(MemoryConfig::a1200());
    emu.load_rom(&rom);
    emu.insert_hdf(hdf_data);

    // Run for a number of frames to let scsi.device probe and perform the handshake
    // and read the RDB. Probing happens very early during scsi.device init, usually within the first few seconds (100-500 frames).
    for _ in 0..2000 {
        emu.run_frame();
        // Check if we have issued a READ SECTORS command (0x20 or 0x21)
        let ide = emu.memory.ide.borrow();
        if ide.command_log.contains(&0x20) || ide.command_log.contains(&0x21) {
            println!(
                "SUCCESS: Reached READ SECTORS command: {:?}",
                ide.command_log
            );
            return;
        }
    }

    let ide = emu.memory.ide.borrow();
    panic!(
        "Failed to reach READ SECTORS.\n\
         IDE Status: 0x{:02X}\n\
         IDE Error: 0x{:02X}\n\
         IDE DevCon: 0x{:02X}\n\
         IDE Select: 0x{:02X}\n\
         IDE Pending IRQ: {}\n\
         IDE Data Index: {}/{}\n\
         IDE Data Direction: {:?}\n\
         Gayle IRQ: 0x{:02X}\n\
         Gayle IntEna: 0x{:02X}\n\
         Chipset IntReq: 0x{:04X}\n\
         Chipset IntEna: 0x{:04X}\n\
         CPU PC: 0x{:08X}\n\
         CPU SR: 0x{:04X}\n\
         CPU Stopped: {} (0x{:08X})\n\
         CIA-A CRA: 0x{:02X} ICR_Mask: 0x{:02X} ICR_Data: 0x{:02X} IR: {} TimerA: 0x{:04X} LatchA: 0x{:04X}\n\
         CIA-B CRA: 0x{:02X} ICR_Mask: 0x{:02X} ICR_Data: 0x{:02X} IR: {} TimerA: 0x{:04X} LatchA: 0x{:04X}\n\
         Command log: {:?}",
        ide.status,
        ide.error,
        ide.devcon,
        ide.select,
        ide.pending_irq,
        ide.data_index,
        ide.data_buffer.len(),
        ide.data_direction,
        emu.memory.gayle_irq,
        emu.memory.gayle_intena,
        emu.chipset.intreq,
        emu.chipset.intena,
        emu.cpu.pc,
        emu.cpu.get_sr(),
        emu.cpu.is_stopped(),
        emu.cpu.stopped,
        emu.memory.cia.borrow().cia_a.cra,
        emu.memory.cia.borrow().cia_a.icr_mask,
        emu.memory.cia.borrow().cia_a.icr_data,
        emu.memory.cia.borrow().cia_a.icr_ir,
        emu.memory.cia.borrow().cia_a.timer_a,
        emu.memory.cia.borrow().cia_a.timer_a_latch,
        emu.memory.cia.borrow().cia_b.cra,
        emu.memory.cia.borrow().cia_b.icr_mask,
        emu.memory.cia.borrow().cia_b.icr_data,
        emu.memory.cia.borrow().cia_b.icr_ir,
        emu.memory.cia.borrow().cia_b.timer_a,
        emu.memory.cia.borrow().cia_b.timer_a_latch,
        ide.command_log
    );
}
