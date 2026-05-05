// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Golden vector tests for the custom chip register dispatch system.
//!
//! Each test verifies that writing a custom register via `dispatch_register_write`
//! correctly updates the target subsystem, matching `WinUAE` behavior.

use rumiga_core::custom;
use rumiga_core::emulator::Emulator;
use rumiga_core::memory::MemoryConfig;

/// Helper: create a fresh A500 emulator for register dispatch testing.
fn make_emulator() -> Emulator {
    Emulator::new(MemoryConfig::a500())
}

mod register_dispatch_golden_vectors {
    use super::*;

    // ─── Color palette dispatch ─────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_color00_updates_playfield_palette_index_0() {
        // WinUAE: writing $DFF180 (COLOR00) stores the 12-bit color value
        // in both the chipset color array and the playfield palette.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::COLOR00, 0x0ABC);
        assert_eq!(emu.playfield.color[0], 0x0ABC);
        assert_eq!(emu.chipset.color[0], 0x0ABC);
    }

    #[test]
    fn test_cpu_write_to_color31_updates_playfield_palette_index_31() {
        // WinUAE: writing $DFF1BE (COLOR31) stores the 12-bit color value
        // at palette index 31.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::COLOR31, 0x0FFF);
        assert_eq!(emu.playfield.color[31], 0x0FFF);
        assert_eq!(emu.chipset.color[31], 0x0FFF);
    }

    #[test]
    fn test_color_write_masks_to_12_bits() {
        // WinUAE: OCS color registers are 12-bit; upper nibble is discarded.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::COLOR00, 0xFFFF);
        assert_eq!(emu.playfield.color[0], 0x0FFF);
        assert_eq!(emu.chipset.color[0], 0x0FFF);
    }

    // ─── Playfield control dispatch ─────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_bplcon0_updates_playfield_plane_count() {
        // WinUAE: BPLCON0 bits 14-12 encode the number of active bitplanes.
        // Writing 0x4200 sets 4 planes (bits 14-12 = 0b100) + color burst.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::BPLCON0, 0x4200);
        assert_eq!(emu.playfield.bplcon0, 0x4200);
        // Verify plane count extraction: (bplcon0 >> 12) & 0x7
        assert_eq!((emu.playfield.bplcon0 >> 12) & 0x7, 4);
    }

    #[test]
    fn test_cpu_write_to_diwstrt_updates_playfield_display_window() {
        // WinUAE: DIWSTRT defines the upper-left corner of the display window.
        // Standard PAL value: $2C81 (VSTRT=0x2C, HSTRT=0x81).
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::DIWSTRT, 0x2C81);
        assert_eq!(emu.playfield.diwstrt, 0x2C81);
    }

    #[test]
    fn test_cpu_write_to_ddfstrt_updates_playfield_data_fetch_start() {
        // WinUAE: DDFSTRT defines where bitplane DMA fetching begins.
        // Standard low-res value: $0038.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::DDFSTRT, 0x0038);
        assert_eq!(emu.playfield.ddfstrt, 0x0038);
    }

    // ─── Bitplane pointer dispatch ──────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_bpl1pth_and_bpl1ptl_updates_playfield_pointer() {
        // WinUAE: BPL1PTH/BPL1PTL form a 32-bit pointer (only 20 bits used on OCS).
        // Writing high then low assembles the full address.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::BPL1PTH, 0x0007);
        emu.dispatch_register_write(custom::BPL1PTL, 0xC000);
        assert_eq!(emu.playfield.bplpt[0], 0x0007_C000);
    }

    // ─── DMACON dispatch ────────────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_dmacon_set_bits_enables_channels() {
        // WinUAE: writing DMACON with bit 15 set ORs the channel bits into the
        // active DMACON register.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::DMACON, 0x8000 | custom::DMA_BITPLANE);
        assert_ne!(emu.chipset.dmacon & custom::DMA_BITPLANE, 0);
    }

    #[test]
    fn test_cpu_write_to_dmacon_clear_bits_disables_channels() {
        // WinUAE: writing DMACON with bit 15 clear ANDs NOT the channel bits,
        // disabling those channels.
        let mut emu = make_emulator();
        // First enable bitplane + master
        emu.dispatch_register_write(
            custom::DMACON,
            0x8000 | custom::DMA_MASTER | custom::DMA_BITPLANE,
        );
        assert!(emu.chipset.dmaen(custom::DMA_BITPLANE));
        // Now clear bitplane
        emu.dispatch_register_write(custom::DMACON, custom::DMA_BITPLANE);
        assert!(!emu.chipset.dmaen(custom::DMA_BITPLANE));
    }

    #[test]
    fn test_dmacon_copper_bit_enables_copper() {
        // WinUAE: enabling DMA_COPPER + DMA_MASTER activates the Copper.
        let mut emu = make_emulator();
        assert!(!emu.copper.enabled);
        emu.dispatch_register_write(
            custom::DMACON,
            0x8000 | custom::DMA_MASTER | custom::DMA_COPPER,
        );
        assert!(emu.copper.enabled);
    }

    #[test]
    fn test_dmacon_master_enable_required_for_dma() {
        // WinUAE: individual DMA channel bits have no effect without DMA_MASTER.
        let mut emu = make_emulator();
        // Enable copper channel but NOT master
        emu.dispatch_register_write(custom::DMACON, 0x8000 | custom::DMA_COPPER);
        assert!(!emu.chipset.dmaen(custom::DMA_COPPER));
        assert!(!emu.copper.enabled);
    }

    // ─── Interrupt dispatch ─────────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_intena_enables_interrupt_mask() {
        // WinUAE: writing INTENA with bit 15 set enables the specified interrupt bits.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::INTENA, 0x8000 | custom::INT_VERTB);
        assert_ne!(emu.chipset.intena & custom::INT_VERTB, 0);
    }

    #[test]
    fn test_cpu_write_to_intreq_sets_pending_interrupt() {
        // WinUAE: writing INTREQ with bit 15 set marks the interrupt as pending.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::INTREQ, 0x8000 | custom::INT_VERTB);
        assert_ne!(emu.chipset.intreq & custom::INT_VERTB, 0);
    }

    #[test]
    fn test_intreqr_read_returns_current_pending_interrupts() {
        // WinUAE: INTREQR ($DFF01E) reads back the current interrupt request state.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::INTREQ, 0x8000 | custom::INT_BLIT);
        assert_eq!(emu.chipset.read_register(custom::INTREQR), custom::INT_BLIT);
    }

    // ─── Copper pointer dispatch ────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_cop1lch_and_cop1lcl_updates_copper_pointer() {
        // WinUAE: COP1LCH/COP1LCL form the 32-bit Copper list 1 start address.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::COP1LCH, 0x0002);
        emu.dispatch_register_write(custom::COP1LCL, 0x0000);
        assert_eq!(emu.copper.cop1lc, 0x0002_0000);
    }

    #[test]
    fn test_copjmp1_strobe_restarts_copper_from_cop1lc() {
        // WinUAE: writing any value to COPJMP1 ($DFF088) restarts the Copper
        // from the address in COP1LC.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::COP1LCH, 0x0000);
        emu.dispatch_register_write(custom::COP1LCL, 0x4000);
        emu.dispatch_register_write(custom::COPJMP1, 0x0000);
        assert_eq!(emu.copper.pc, 0x0000_4000);
    }

    // ─── Beam position readback ─────────────────────────────────────────────

    #[test]
    fn test_vposr_read_returns_current_vertical_position() {
        // WinUAE: VPOSR ($DFF004) returns the MSB of the vertical beam position
        // (bit 0 = V8 for long-frame identification on PAL).
        let mut emu = make_emulator();
        emu.chipset.vpos = 0x0100; // V8 set (line 256)
        assert_eq!(emu.chipset.read_register(custom::VPOSR), 0x0001);
    }

    #[test]
    fn test_vhposr_read_returns_combined_beam_position() {
        // WinUAE: VHPOSR ($DFF006) returns V[7:0] in the high byte and H[7:0]
        // in the low byte.
        let mut emu = make_emulator();
        emu.chipset.vpos = 44; // line 44
        emu.chipset.hpos = 100; // hpos 100
        let expected = (44 << 8) | 100;
        assert_eq!(emu.chipset.read_register(custom::VHPOSR), expected);
    }

    // ─── Blitter dispatch ───────────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_bltcon0_updates_blitter_control() {
        // WinUAE: BLTCON0 ($DFF040) configures shift, channel enables, and minterm.
        let mut emu = make_emulator();
        emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
        assert_eq!(emu.blitter.bltcon0, 0x09F0);
    }

    #[test]
    fn test_cpu_write_to_bltsize_triggers_blit_execution() {
        // WinUAE: writing BLTSIZE ($DFF058) starts the blitter immediately.
        // Height in bits 15-6, width in bits 5-0 (in words).
        let mut emu = make_emulator();
        // 1 row × 1 word = minimal blit
        emu.dispatch_register_write(custom::BLTSIZE, (1 << 6) | 1);
        assert!(emu.blitter.done);
    }

    // ─── Floppy dispatch ────────────────────────────────────────────────────

    #[test]
    fn test_cpu_write_to_dsklen_twice_activates_disk_dma() {
        // WinUAE: the Amiga requires DSKLEN to be written twice with bit 15 set
        // to activate disk DMA (hardware safety interlock).
        let mut emu = make_emulator();
        let dsklen_value = 0x8000 | 0x0100; // enable + 256 words
        emu.dispatch_register_write(custom::DSKLEN, dsklen_value);
        // First write: DMA not yet active (need double-write)
        emu.dispatch_register_write(custom::DSKLEN, dsklen_value);
        // Second write: DMA now in read mode
        assert_eq!(emu.floppy.dsk_length, 0x0100);
    }
}
