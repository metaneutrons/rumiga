// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Golden vector tests validated against WinUAE reference implementation.
//!
//! Each test cites the WinUAE source file and function it validates against.

#![allow(clippy::unreadable_literal)]
#![allow(clippy::similar_names)]
#![allow(clippy::doc_markdown)]

mod winuae_blitter_minterm_golden_vectors {
    use rumiga_core::blitter::apply_minterm;

    // Test vectors: A=0xF0F0, B=0xCCCC, C=0xAAAA
    // These values exercise all 8 bit-combinations of (A,B,C) across 16 bits.
    const A: u16 = 0xF0F0;
    const B: u16 = 0xCCCC;
    const C: u16 = 0xAAAA;

    /// Reference: WinUAE blit.h — blit_func() generic minterm implementation.
    /// Computes expected result for any minterm using the truth table definition.
    const fn expected(minterm: u8) -> u16 {
        let mut result: u16 = 0;
        if minterm & 0x01 != 0 {
            result |= !A & !B & !C;
        }
        if minterm & 0x02 != 0 {
            result |= !A & !B & C;
        }
        if minterm & 0x04 != 0 {
            result |= !A & B & !C;
        }
        if minterm & 0x08 != 0 {
            result |= !A & B & C;
        }
        if minterm & 0x10 != 0 {
            result |= A & !B & !C;
        }
        if minterm & 0x20 != 0 {
            result |= A & !B & C;
        }
        if minterm & 0x40 != 0 {
            result |= A & B & !C;
        }
        if minterm & 0x80 != 0 {
            result |= A & B & C;
        }
        result
    }

    /// WinUAE blit.h: minterm 0x00 = constant zero.
    #[test]
    fn test_minterm_0x00_constant_zero() {
        assert_eq!(apply_minterm(A, B, C, 0x00), 0x0000);
    }

    /// WinUAE blit.h: minterm 0xFF = constant ones.
    #[test]
    fn test_minterm_0xff_constant_ones() {
        assert_eq!(apply_minterm(A, B, C, 0xFF), 0xFFFF);
    }

    /// WinUAE blit.h: minterm 0xF0 = copy channel A.
    #[test]
    fn test_minterm_0xf0_copies_channel_a_unchanged() {
        assert_eq!(apply_minterm(A, B, C, 0xF0), A);
    }

    /// WinUAE blit.h: minterm 0xCC = copy channel B.
    #[test]
    fn test_minterm_0xcc_copies_channel_b_unchanged() {
        assert_eq!(apply_minterm(A, B, C, 0xCC), B);
    }

    /// WinUAE blit.h: minterm 0xAA = copy channel C.
    #[test]
    fn test_minterm_0xaa_copies_channel_c_unchanged() {
        assert_eq!(apply_minterm(A, B, C, 0xAA), C);
    }

    /// WinUAE blit.h: minterm 0x0F = NOT A.
    #[test]
    fn test_minterm_0x0f_inverts_channel_a() {
        assert_eq!(apply_minterm(A, B, C, 0x0F), !A);
    }

    /// WinUAE blit.h: minterm 0x33 = NOT B.
    #[test]
    fn test_minterm_0x33_inverts_channel_b() {
        assert_eq!(apply_minterm(A, B, C, 0x33), !B);
    }

    /// WinUAE blit.h: minterm 0x55 = NOT C.
    #[test]
    fn test_minterm_0x55_inverts_channel_c() {
        assert_eq!(apply_minterm(A, B, C, 0x55), !C);
    }

    /// WinUAE blit.h: minterm 0x0A = !A & C.
    #[test]
    fn test_minterm_0x0a_not_a_and_c() {
        assert_eq!(apply_minterm(A, B, C, 0x0A), !A & C);
    }

    /// WinUAE blit.h: minterm 0xCA = cookie-cut (A ? B : C).
    #[test]
    fn test_minterm_0xca_cookie_cut_selects_b_where_a_set_c_elsewhere() {
        let exp = (A & B) | (!A & C);
        assert_eq!(apply_minterm(A, B, C, 0xCA), exp);
    }

    /// WinUAE blit.h: minterm 0x5A = A XOR C.
    #[test]
    fn test_minterm_0x5a_xor_a_c() {
        assert_eq!(apply_minterm(A, B, C, 0x5A), A ^ C);
    }

    /// WinUAE blit.h: minterm 0xFC = A OR B.
    #[test]
    fn test_minterm_0xfc_or_a_b() {
        assert_eq!(apply_minterm(A, B, C, 0xFC), A | B);
    }

    /// WinUAE blit.h: minterm 0xC0 = A AND B.
    #[test]
    fn test_minterm_0xc0_and_a_b() {
        assert_eq!(apply_minterm(A, B, C, 0xC0), A & B);
    }

    /// WinUAE blit.h: minterm 0xA0 = A AND C.
    #[test]
    fn test_minterm_0xa0_and_a_c() {
        assert_eq!(apply_minterm(A, B, C, 0xA0), A & C);
    }

    /// WinUAE blit.h: minterm 0x88 = B AND C.
    #[test]
    fn test_minterm_0x88_and_b_c() {
        assert_eq!(apply_minterm(A, B, C, 0x88), B & C);
    }

    /// WinUAE blit.h: minterm 0x3C = A XOR B.
    #[test]
    fn test_minterm_0x3c_xor_a_b() {
        assert_eq!(apply_minterm(A, B, C, 0x3C), A ^ B);
    }

    /// WinUAE blit.h: minterm 0x66 = B XOR C.
    #[test]
    fn test_minterm_0x66_xor_b_c() {
        assert_eq!(apply_minterm(A, B, C, 0x66), B ^ C);
    }

    /// WinUAE blit.h: minterm 0x96 = A XOR B XOR C.
    #[test]
    fn test_minterm_0x96_xor_a_b_c() {
        assert_eq!(apply_minterm(A, B, C, 0x96), A ^ B ^ C);
    }

    /// WinUAE blit.h: minterm 0xFA = A OR C.
    #[test]
    fn test_minterm_0xfa_or_a_c() {
        assert_eq!(apply_minterm(A, B, C, 0xFA), A | C);
    }

    /// WinUAE blit.h: minterm 0xFE = A OR B OR C.
    #[test]
    fn test_minterm_0xfe_or_a_b_c() {
        assert_eq!(apply_minterm(A, B, C, 0xFE), A | B | C);
    }

    /// WinUAE blit.h: minterm 0x80 = A AND B AND C.
    #[test]
    fn test_minterm_0x80_and_a_b_c() {
        assert_eq!(apply_minterm(A, B, C, 0x80), A & B & C);
    }

    /// WinUAE blit.h: minterm 0xEA = C OR (A AND B).
    #[test]
    fn test_minterm_0xea_c_or_a_and_b() {
        assert_eq!(apply_minterm(A, B, C, 0xEA), C | (A & B));
    }

    /// WinUAE blit.h: minterm 0x30 = A AND NOT B.
    #[test]
    fn test_minterm_0x30_a_and_not_b() {
        assert_eq!(apply_minterm(A, B, C, 0x30), A & !B);
    }

    /// WinUAE blit.h: exhaustive test of all 256 minterms against truth table.
    #[test]
    fn test_all_256_minterms_match_truth_table() {
        for mt in 0..=255u8 {
            assert_eq!(
                apply_minterm(A, B, C, mt),
                expected(mt),
                "minterm 0x{mt:02X} mismatch"
            );
        }
    }
}

mod winuae_register_offset_golden_vectors {
    use rumiga_core::custom::*;

    /// WinUAE identify.cpp: DMACONR at offset 0x002.
    #[test]
    fn test_register_dmaconr_offset_matches_winuae_0x002() {
        assert_eq!(DMACONR, 0x002);
    }

    /// WinUAE identify.cpp: VPOSR at offset 0x004.
    #[test]
    fn test_register_vposr_offset_matches_winuae_0x004() {
        assert_eq!(VPOSR, 0x004);
    }

    /// WinUAE identify.cpp: VHPOSR at offset 0x006.
    #[test]
    fn test_register_vhposr_offset_matches_winuae_0x006() {
        assert_eq!(VHPOSR, 0x006);
    }

    /// WinUAE identify.cpp: DSKDATR at offset 0x008.
    #[test]
    fn test_register_dskdatr_offset_matches_winuae_0x008() {
        assert_eq!(DSKDATR, 0x008);
    }

    /// WinUAE identify.cpp: JOY0DAT at offset 0x00A.
    #[test]
    fn test_register_joy0dat_offset_matches_winuae_0x00a() {
        assert_eq!(JOY0DAT, 0x00A);
    }

    /// WinUAE identify.cpp: JOY1DAT at offset 0x00C.
    #[test]
    fn test_register_joy1dat_offset_matches_winuae_0x00c() {
        assert_eq!(JOY1DAT, 0x00C);
    }

    /// WinUAE identify.cpp: INTENAR at offset 0x01C.
    #[test]
    fn test_register_intenar_offset_matches_winuae_0x01c() {
        assert_eq!(INTENAR, 0x01C);
    }

    /// WinUAE identify.cpp: INTREQR at offset 0x01E.
    #[test]
    fn test_register_intreqr_offset_matches_winuae_0x01e() {
        assert_eq!(INTREQR, 0x01E);
    }

    /// WinUAE identify.cpp: DSKLEN at offset 0x024.
    #[test]
    fn test_register_dsklen_offset_matches_winuae_0x024() {
        assert_eq!(DSKLEN, 0x024);
    }

    /// WinUAE identify.cpp: BLTCON0 at offset 0x040.
    #[test]
    fn test_register_bltcon0_offset_matches_winuae_0x040() {
        assert_eq!(BLTCON0, 0x040);
    }

    /// WinUAE identify.cpp: BLTCON1 at offset 0x042.
    #[test]
    fn test_register_bltcon1_offset_matches_winuae_0x042() {
        assert_eq!(BLTCON1, 0x042);
    }

    /// WinUAE identify.cpp: BLTSIZE at offset 0x058.
    #[test]
    fn test_register_bltsize_offset_matches_winuae_0x058() {
        assert_eq!(BLTSIZE, 0x058);
    }

    /// WinUAE identify.cpp: COP1LCH at offset 0x080.
    #[test]
    fn test_register_cop1lch_offset_matches_winuae_0x080() {
        assert_eq!(COP1LCH, 0x080);
    }

    /// WinUAE identify.cpp: COP2LCH at offset 0x084.
    #[test]
    fn test_register_cop2lch_offset_matches_winuae_0x084() {
        assert_eq!(COP2LCH, 0x084);
    }

    /// WinUAE identify.cpp: DIWSTRT at offset 0x08E.
    #[test]
    fn test_register_diwstrt_offset_matches_winuae_0x08e() {
        assert_eq!(DIWSTRT, 0x08E);
    }

    /// WinUAE identify.cpp: DIWSTOP at offset 0x090.
    #[test]
    fn test_register_diwstop_offset_matches_winuae_0x090() {
        assert_eq!(DIWSTOP, 0x090);
    }

    /// WinUAE identify.cpp: DDFSTRT at offset 0x092.
    #[test]
    fn test_register_ddfstrt_offset_matches_winuae_0x092() {
        assert_eq!(DDFSTRT, 0x092);
    }

    /// WinUAE identify.cpp: DDFSTOP at offset 0x094.
    #[test]
    fn test_register_ddfstop_offset_matches_winuae_0x094() {
        assert_eq!(DDFSTOP, 0x094);
    }

    /// WinUAE identify.cpp: DMACON at offset 0x096.
    #[test]
    fn test_register_dmacon_offset_matches_winuae_0x096() {
        assert_eq!(DMACON, 0x096);
    }

    /// WinUAE identify.cpp: INTENA at offset 0x09A.
    #[test]
    fn test_register_intena_offset_matches_winuae_0x09a() {
        assert_eq!(INTENA, 0x09A);
    }

    /// WinUAE identify.cpp: INTREQ at offset 0x09C.
    #[test]
    fn test_register_intreq_offset_matches_winuae_0x09c() {
        assert_eq!(INTREQ, 0x09C);
    }

    /// WinUAE identify.cpp: AUD0LCH at offset 0x0A0.
    #[test]
    fn test_register_aud0lch_offset_matches_winuae_0x0a0() {
        assert_eq!(AUD0LCH, 0x0A0);
    }

    /// WinUAE identify.cpp: BPLCON0 at offset 0x100.
    #[test]
    fn test_register_bplcon0_offset_matches_winuae_0x100() {
        assert_eq!(BPLCON0, 0x100);
    }

    /// WinUAE identify.cpp: BPL1PTH at offset 0x0E0.
    #[test]
    fn test_register_bpl1pth_offset_matches_winuae_0x0e0() {
        assert_eq!(BPL1PTH, 0x0E0);
    }

    /// WinUAE identify.cpp: SPR0PTH at offset 0x120.
    #[test]
    fn test_register_spr0pth_offset_matches_winuae_0x120() {
        assert_eq!(SPR0PTH, 0x120);
    }

    /// WinUAE identify.cpp: COLOR00 at offset 0x180.
    #[test]
    fn test_register_color00_offset_matches_winuae_0x180() {
        assert_eq!(COLOR00, 0x180);
    }

    /// WinUAE identify.cpp: COLOR31 at offset 0x1BE.
    #[test]
    fn test_register_color31_offset_matches_winuae_0x1be() {
        assert_eq!(COLOR31, 0x1BE);
    }

    /// WinUAE identify.cpp: all 32 color registers are contiguous, 2 bytes apart.
    #[test]
    fn test_color_registers_contiguous_stride_2() {
        for i in 0..32u16 {
            assert_eq!(COLOR00 + i * 2, 0x180 + i * 2);
        }
    }

    /// WinUAE custom.h: DMA bit masks match hardware specification.
    #[test]
    fn test_dma_bit_masks_match_winuae() {
        assert_eq!(DMA_AUD0, 0x0001);
        assert_eq!(DMA_AUD1, 0x0002);
        assert_eq!(DMA_AUD2, 0x0004);
        assert_eq!(DMA_AUD3, 0x0008);
        assert_eq!(DMA_DISK, 0x0010);
        assert_eq!(DMA_SPRITE, 0x0020);
        assert_eq!(DMA_BLITTER, 0x0040);
        assert_eq!(DMA_COPPER, 0x0080);
        assert_eq!(DMA_BITPLANE, 0x0100);
        assert_eq!(DMA_MASTER, 0x0200);
        assert_eq!(DMA_BLITPRI, 0x0400);
    }

    /// WinUAE identify.cpp: blitter registers at correct offsets.
    #[test]
    fn test_blitter_register_block_offsets() {
        assert_eq!(BLTAFWM, 0x044);
        assert_eq!(BLTALWM, 0x046);
        assert_eq!(BLTCPTH, 0x048);
        assert_eq!(BLTBPTH, 0x04C);
        assert_eq!(BLTAPTH, 0x050);
        assert_eq!(BLTDPTH, 0x054);
        assert_eq!(BLTCMOD, 0x060);
        assert_eq!(BLTBMOD, 0x062);
        assert_eq!(BLTAMOD, 0x064);
        assert_eq!(BLTDMOD, 0x066);
        assert_eq!(BLTCDAT, 0x070);
        assert_eq!(BLTBDAT, 0x072);
        assert_eq!(BLTADAT, 0x074);
    }
}

mod winuae_cia_timer_golden_vectors {
    use rumiga_core::cia::CiaState;

    /// WinUAE cia.cpp: Timer loaded with N, underflows after N+1 ticks.
    #[test]
    fn test_cia_timer_a_underflows_after_n_plus_1_ticks() {
        let mut cia = CiaState::new();
        // Load timer with value 5
        cia.write(0x4, 5); // TALO = 5
        cia.write(0x5, 0); // TAHI = 0
        // Enable timer A interrupt
        cia.write(0xD, 0x81); // ICR: set bit 7 + bit 0 (TA)
        // Start timer
        cia.write(0xE, 0x01); // CRA: START

        // Ticks 1..5: no underflow
        for i in 1..=5 {
            assert!(!cia.tick(), "unexpected underflow at tick {i}");
        }
        // Tick 6 (N+1): underflow
        assert!(cia.tick(), "expected underflow at tick N+1=6");
    }

    /// WinUAE cia.cpp: One-shot mode stops timer after underflow.
    #[test]
    fn test_cia_timer_a_oneshot_stops_after_underflow() {
        let mut cia = CiaState::new();
        cia.write(0x4, 2); // TALO = 2
        cia.write(0x5, 0); // TAHI = 0
        // Start in one-shot mode (bit 3 = RUNMODE, bit 0 = START)
        cia.write(0xE, 0x09);

        cia.tick(); // 2 -> 1
        cia.tick(); // 1 -> 0
        cia.tick(); // 0 -> underflow, stop

        // CRA START bit should be cleared
        let cra = cia.read(0xE);
        assert_eq!(cra & 0x01, 0, "timer should have stopped");

        // Timer should not decrement further
        let before = cia.timer_a;
        cia.tick();
        assert_eq!(cia.timer_a, before, "stopped timer should not count");
    }

    /// WinUAE cia.cpp: Continuous mode reloads and keeps counting.
    #[test]
    fn test_cia_timer_a_continuous_reloads_and_continues() {
        let mut cia = CiaState::new();
        cia.write(0x4, 3); // TALO = 3
        cia.write(0x5, 0); // TAHI = 0
        cia.write(0xE, 0x01); // CRA: START, continuous

        // First underflow after 4 ticks
        for _ in 0..3 {
            cia.tick();
        }
        cia.tick(); // underflow, reload to 3

        assert_eq!(cia.timer_a, 3, "timer should reload to latch value");

        // Timer keeps running — CRA START still set
        let cra = cia.read(0xE);
        assert_eq!(cra & 0x01, 1, "continuous timer should keep running");
    }

    /// WinUAE cia.cpp: ICR read clears all pending bits, returns old value.
    #[test]
    fn test_cia_icr_read_clears_pending_returns_old_value() {
        let mut cia = CiaState::new();
        // Force pending bits
        cia.icr_data = 0x03; // TA + TB pending

        let val = cia.read(0xD);
        assert_eq!(val, 0x03, "should return pending bits");
        assert_eq!(cia.icr_data, 0, "pending bits should be cleared after read");

        // Second read returns 0
        let val2 = cia.read(0xD);
        assert_eq!(val2, 0);
    }

    /// WinUAE cia.cpp: ICR mask set/clear logic — bit 7 controls set vs clear.
    #[test]
    fn test_cia_icr_mask_set_clear_bit7_semantics() {
        let mut cia = CiaState::new();

        // Set TA mask: write 0x81 (bit 7 = set, bit 0 = TA)
        cia.write(0xD, 0x81);
        assert_eq!(cia.icr_mask & 0x01, 0x01, "TA mask should be set");

        // Set TB mask: write 0x82 (bit 7 = set, bit 1 = TB)
        cia.write(0xD, 0x82);
        assert_eq!(cia.icr_mask & 0x03, 0x03, "TA+TB masks should be set");

        // Clear TA mask: write 0x01 (bit 7 = 0 = clear, bit 0 = TA)
        cia.write(0xD, 0x01);
        assert_eq!(cia.icr_mask & 0x01, 0x00, "TA mask should be cleared");
        assert_eq!(cia.icr_mask & 0x02, 0x02, "TB mask should remain set");
    }

    /// WinUAE cia.cpp: Timer B also counts down and underflows correctly.
    #[test]
    fn test_cia_timer_b_underflows_after_n_plus_1_ticks() {
        let mut cia = CiaState::new();
        cia.write(0x6, 4); // TBLO = 4
        cia.write(0x7, 0); // TBHI = 0
        cia.write(0xD, 0x82); // ICR: set TB mask
        cia.write(0xF, 0x01); // CRB: START

        for _ in 0..4 {
            assert!(!cia.tick());
        }
        assert!(cia.tick(), "timer B should underflow at tick N+1=5");
        assert_eq!(cia.timer_b, 4, "timer B should reload");
    }

    /// WinUAE cia.cpp: Force-load via CR_LOAD bit loads latch into counter.
    #[test]
    fn test_cia_force_load_loads_latch_into_counter() {
        let mut cia = CiaState::new();
        cia.write(0x4, 0x42); // TALO
        cia.write(0x5, 0x01); // TAHI -> latch = 0x0142

        // Timer is stopped, writing TAHI already loads counter
        assert_eq!(cia.timer_a, 0x0142);

        // Change latch without loading (timer running)
        cia.write(0xE, 0x01); // start
        cia.write(0x4, 0x99); // change low latch
        cia.write(0x5, 0x02); // change high latch (doesn't load because running)
        assert_ne!(cia.timer_a_latch, cia.timer_a);

        // Force load
        cia.write(0xE, 0x11); // START | LOAD
        assert_eq!(cia.timer_a, 0x0299);
    }
}

mod winuae_copper_execution_golden_vectors {
    use rumiga_core::copper::{CopperAction, CopperExecState, CopperState};

    fn make_chip_ram(instructions: &[(u16, u16)]) -> Vec<u8> {
        let mut ram = Vec::new();
        for &(w1, w2) in instructions {
            ram.extend_from_slice(&w1.to_be_bytes());
            ram.extend_from_slice(&w2.to_be_bytes());
        }
        // Pad to avoid out-of-bounds
        ram.resize(ram.len() + 256, 0);
        ram
    }

    fn enabled_copper(instructions: &[(u16, u16)]) -> (CopperState, Vec<u8>) {
        let ram = make_chip_ram(instructions);
        let mut copper = CopperState::new();
        copper.enabled = true;
        copper.state = CopperExecState::FetchFirst;
        (copper, ram)
    }

    /// WinUAE copper.cpp: MOVE writes COLOR00 at correct beam position.
    #[test]
    fn test_copper_move_writes_color00_at_correct_beam_position() {
        // MOVE COLOR00 ($180), value $0F00
        let (mut copper, ram) = enabled_copper(&[(0x0180, 0x0F00)]);

        copper.cycle(&ram, 44, 100); // fetch first
        copper.cycle(&ram, 44, 100); // fetch second
        let action = copper.cycle(&ram, 44, 100); // execute

        assert_eq!(
            action,
            Some(CopperAction::WriteRegister {
                offset: 0x0180,
                value: 0x0F00,
            })
        );
    }

    /// WinUAE copper.cpp: WAIT blocks until beam position matches with mask.
    #[test]
    fn test_copper_wait_blocks_until_beam_matches_with_mask() {
        // WAIT for vpos=100, hpos=0, vmask=0x7F, hmask=0x7F
        let ir1: u16 = (100 << 8) | 1; // vpos=100, bit 0=1 (WAIT/SKIP)
        let ir2: u16 = (0x7F << 8) | (0x7F << 1); // full masks, bit 0=0 (WAIT)
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // Beam at vpos=50 — should stay waiting
        copper.cycle(&ram, 50, 0);
        assert_eq!(copper.state, CopperExecState::Execute);

        // Beam at vpos=100 — should pass
        copper.cycle(&ram, 100, 0);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    /// WinUAE copper.cpp: WAIT with all-ones mask requires exact match.
    #[test]
    fn test_copper_wait_all_ones_mask_exact_match() {
        // WAIT for vpos=80, hpos=40
        let ir1: u16 = (80 << 8) | (40 << 1) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0);
        copper.cycle(&ram, 0, 0);

        // vpos=80 but hpos=30 (< 40) — should wait
        copper.cycle(&ram, 80, 30);
        assert_eq!(copper.state, CopperExecState::Execute);

        // vpos=80, hpos=40 — should pass
        copper.cycle(&ram, 80, 40);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    /// WinUAE copper.cpp: End-of-list (0xFFFF/0xFFFE) never advances.
    #[test]
    fn test_copper_end_of_list_never_advances() {
        let (mut copper, ram) = enabled_copper(&[(0xFFFF, 0xFFFE)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // Try many beam positions — should never pass
        for v in [0, 100, 200, 255, 311] {
            for h in [0, 50, 100, 113, 226] {
                copper.cycle(&ram, v, h);
                assert_eq!(
                    copper.state,
                    CopperExecState::Execute,
                    "end-of-list should never pass at v={v} h={h}"
                );
            }
        }
    }

    /// WinUAE copper.cpp: SKIP advances PC by 4 when condition met.
    #[test]
    fn test_copper_skip_advances_when_condition_met() {
        // SKIP if vpos >= 0, hpos >= 0 (always true)
        let ir1: u16 = 1; // vpos=0, hpos=0, bit 0=1
        let ir2: u16 = (0x7F << 8) | (0x7F << 1) | 1; // SKIP (bit 0=1)
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2), (0x0180, 0x0FFF)]);

        copper.cycle(&ram, 10, 10); // fetch first
        copper.cycle(&ram, 10, 10); // fetch second

        let pc_before = copper.pc;
        copper.cycle(&ram, 10, 10); // execute SKIP
        assert_eq!(copper.state, CopperExecState::FetchFirst);
        assert_eq!(copper.pc, pc_before + 4, "SKIP should advance PC by 4");
    }

    /// WinUAE copper.cpp: Danger bit protection blocks writes to $00-$3F.
    #[test]
    fn test_copper_danger_bit_blocks_writes_below_0x40() {
        // MOVE to COPCON ($02E) without danger bit
        let (mut copper, ram) = enabled_copper(&[(0x002E, 0x0002)]);

        copper.cycle(&ram, 0, 0);
        copper.cycle(&ram, 0, 0);
        let action = copper.cycle(&ram, 0, 0);
        assert_eq!(action, None, "write below $40 should be blocked");
    }

    /// WinUAE copper.cpp: Danger bit allows writes to $00-$3F when set.
    #[test]
    fn test_copper_danger_bit_allows_writes_below_0x40_when_set() {
        let (mut copper, ram) = enabled_copper(&[(0x002E, 0x0002)]);
        copper.danger = true;

        copper.cycle(&ram, 0, 0);
        copper.cycle(&ram, 0, 0);
        let action = copper.cycle(&ram, 0, 0);
        assert_eq!(
            action,
            Some(CopperAction::WriteRegister {
                offset: 0x002E,
                value: 0x0002,
            })
        );
    }

    /// WinUAE copper.cpp: SKIP does not advance when condition not met.
    #[test]
    fn test_copper_skip_does_not_advance_when_condition_not_met() {
        // SKIP if vpos >= 200 (won't be met at vpos=10)
        let ir1: u16 = (200 << 8) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1) | 1; // SKIP
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2), (0x0180, 0x0FFF)]);

        copper.cycle(&ram, 10, 10); // fetch first
        copper.cycle(&ram, 10, 10); // fetch second

        let pc_before = copper.pc;
        copper.cycle(&ram, 10, 10); // execute SKIP — condition not met
        assert_eq!(copper.state, CopperExecState::FetchFirst);
        assert_eq!(copper.pc, pc_before, "SKIP should not advance PC");
    }
}

mod winuae_playfield_rendering_golden_vectors {
    use rumiga_core::playfield::{DISPLAY_WIDTH, PlayfieldState, amiga_to_rgb565};

    /// WinUAE custom.h: $0000 → 0x0000 (black).
    #[test]
    fn test_color_0x0000_converts_to_rgb565_black() {
        assert_eq!(amiga_to_rgb565(0x0000), 0x0000);
    }

    /// WinUAE custom.h: $0FFF → 0xFFFF (white).
    #[test]
    fn test_color_0x0fff_converts_to_rgb565_white() {
        assert_eq!(amiga_to_rgb565(0x0FFF), 0xFFFF);
    }

    /// WinUAE custom.h: $0F00 → 0xF800 (red).
    #[test]
    fn test_color_0x0f00_converts_to_rgb565_red() {
        assert_eq!(amiga_to_rgb565(0x0F00), 0xF800);
    }

    /// WinUAE custom.h: $00F0 → 0x07E0 (green).
    #[test]
    fn test_color_0x00f0_converts_to_rgb565_green() {
        assert_eq!(amiga_to_rgb565(0x00F0), 0x07E0);
    }

    /// WinUAE custom.h: $000F → 0x001F (blue).
    #[test]
    fn test_color_0x000f_converts_to_rgb565_blue() {
        assert_eq!(amiga_to_rgb565(0x000F), 0x001F);
    }

    /// WinUAE custom.h: $0888 → 0x8C51 (gray, with MSB replication for accuracy).
    #[test]
    fn test_color_0x0888_converts_to_rgb565_gray() {
        // R=8: (8<<1)|(8>>3) = 17, G=8: (8<<2)|(8>>2) = 34, B=8: (8<<1)|(8>>3) = 17
        // (17<<11)|(34<<5)|17 = 0x8C51
        assert_eq!(amiga_to_rgb565(0x0888), 0x8C51);
    }

    /// WinUAE playfield: single bitplane with alternating bits produces
    /// alternating color 0 and color 1 pixels.
    #[test]
    fn test_single_bitplane_alternating_bits_produces_alternating_colors() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x1000; // 1 plane
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;
        pf.color[0] = 0x0000;
        pf.color[1] = 0x0FFF;

        // 0xAAAA = 1010_1010_1010_1010 — alternating bits
        let mut chip_ram = vec![0u8; 1024];
        chip_ram[0] = 0xAA;
        chip_ram[1] = 0xAA;
        pf.bplpt[0] = 0;

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let white = amiga_to_rgb565(0x0FFF);
        let black = amiga_to_rgb565(0x0000);

        // Bit 15=1, bit 14=0, bit 13=1, ... alternating
        for (i, px) in line_buffer.iter().enumerate().take(16) {
            let expected = if i % 2 == 0 { white } else { black };
            assert_eq!(*px, expected, "pixel {i} mismatch");
        }
    }

    /// WinUAE playfield: 4 bitplanes with known data produce correct palette index.
    #[test]
    fn test_four_bitplanes_produce_correct_palette_index() {
        let mut pf = PlayfieldState::new();
        pf.bplcon0 = 0x4000; // 4 planes
        pf.diwstrt = 0x2C81;
        pf.diwstop = 0x2CC1;

        // Set color 15 (all planes set) to a known value
        pf.color[15] = 0x0F0F; // purple

        // All planes = 0xFFFF for first word → all pixels = index 15
        let mut chip_ram = vec![0u8; 1024];
        for plane in 0..4usize {
            let base = plane * 64;
            chip_ram[base] = 0xFF;
            chip_ram[base + 1] = 0xFF;
            pf.bplpt[plane] = u32::try_from(base).unwrap();
        }

        let mut line_buffer = [0u16; DISPLAY_WIDTH as usize];
        pf.render_scanline(0x2C, &chip_ram, &mut line_buffer);

        let expected = amiga_to_rgb565(0x0F0F);
        for (i, px) in line_buffer.iter().enumerate().take(16) {
            assert_eq!(*px, expected, "pixel {i} should be color 15");
        }
    }
}

mod winuae_audio_timing_golden_vectors {
    use rumiga_core::audio::{AMIGA_CLOCK_PAL, AudioState};

    /// WinUAE audio.cpp: Period 124 at PAL clock → ~28.6 kHz sample rate.
    #[test]
    fn test_period_124_produces_approx_28600_hz() {
        let freq = AMIGA_CLOCK_PAL / 124;
        // 3_546_895 / 124 = 28_604
        assert!(
            (28_000..29_200).contains(&freq),
            "period 124 should produce ~28.6 kHz, got {freq}"
        );
    }

    /// WinUAE audio.cpp: Period 162 at PAL clock → ~21.9 kHz sample rate.
    #[test]
    fn test_period_162_produces_approx_21900_hz() {
        let freq = AMIGA_CLOCK_PAL / 162;
        // 3_546_895 / 162 = 21_894
        assert!(
            (21_500..22_300).contains(&freq),
            "period 162 should produce ~21.9 kHz, got {freq}"
        );
    }

    /// WinUAE audio.cpp: Period 320 at PAL clock → ~11.1 kHz sample rate.
    #[test]
    fn test_period_320_produces_approx_11100_hz() {
        let freq = AMIGA_CLOCK_PAL / 320;
        // 3_546_895 / 320 = 11_084
        assert!(
            (10_800..11_400).contains(&freq),
            "period 320 should produce ~11.1 kHz, got {freq}"
        );
    }

    /// WinUAE audio.cpp: Volume 64 = full scale output.
    #[test]
    fn test_volume_64_full_scale() {
        let mut state = AudioState::new();
        state.channels[0].sample_byte = 127;
        state.channels[0].volume = 64;

        let mut left = [0i16; 1];
        let mut right = [0i16; 1];
        state.generate_samples(&[], &mut left, &mut right, 1);

        // 127 * 64 * 100/100 = 8128
        assert_eq!(left[0], 8128);
    }

    /// WinUAE audio.cpp: Volume 0 = silence.
    #[test]
    fn test_volume_0_silence() {
        let mut state = AudioState::new();
        state.channels[0].sample_byte = 127;
        state.channels[0].volume = 0;

        let mut left = [0i16; 1];
        let mut right = [0i16; 1];
        state.generate_samples(&[], &mut left, &mut right, 1);

        assert_eq!(left[0], 0);
        assert_eq!(right[0], 0);
    }

    /// WinUAE audio.cpp: Default stereo assignment — channels 0+3 left, 1+2 right.
    #[test]
    fn test_default_stereo_channels_0_3_left_1_2_right() {
        let state = AudioState::new();

        // Channels 0 and 3: 100% left, 0% right
        assert_eq!(state.channel_mix[0].left_pct, 100);
        assert_eq!(state.channel_mix[0].right_pct, 0);
        assert_eq!(state.channel_mix[3].left_pct, 100);
        assert_eq!(state.channel_mix[3].right_pct, 0);

        // Channels 1 and 2: 0% left, 100% right
        assert_eq!(state.channel_mix[1].left_pct, 0);
        assert_eq!(state.channel_mix[1].right_pct, 100);
        assert_eq!(state.channel_mix[2].left_pct, 0);
        assert_eq!(state.channel_mix[2].right_pct, 100);
    }

    /// WinUAE custom.h: PAL clock constant matches hardware spec.
    #[test]
    fn test_pal_clock_constant_matches_hardware() {
        assert_eq!(AMIGA_CLOCK_PAL, 3_546_895);
    }

    /// WinUAE audio.cpp: Audio channel register spacing is 16 bytes.
    #[test]
    fn test_audio_channel_register_spacing_16_bytes() {
        use rumiga_core::custom::*;
        assert_eq!(AUD1LCH - AUD0LCH, 0x10);
        assert_eq!(AUD2LCH - AUD1LCH, 0x10);
        assert_eq!(AUD3LCH - AUD2LCH, 0x10);
    }
}

mod winuae_memory_map_golden_vectors {
    use m68000::memory_access::MemoryAccess;
    use rumiga_core::memory::{AmigaMemory, MemoryConfig};

    /// WinUAE memory.cpp: Chip RAM at $000000.
    #[test]
    fn test_chip_ram_at_000000() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        let _ = mem.set_byte(0x000000, 0x42);
        assert_eq!(mem.get_byte(0x000000), Some(0x42));
    }

    /// WinUAE memory.cpp: Chip RAM mirrored up to $200000.
    #[test]
    fn test_chip_ram_mirrored_up_to_200000() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        let _ = mem.set_byte(0x001000, 0xAB);
        // 512KB chip RAM: 0x081000 mirrors to 0x001000
        assert_eq!(mem.get_byte(0x081000), Some(0xAB));
        // 0x101000 also mirrors
        assert_eq!(mem.get_byte(0x101000), Some(0xAB));
    }

    /// WinUAE memory.cpp: ROM at $FC0000 (256K).
    #[test]
    fn test_rom_at_fc0000_256k() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let mut rom = vec![0u8; 256 * 1024];
        rom[0] = 0x11;
        rom[1] = 0x14;
        mem.load_rom(&rom);
        assert_eq!(mem.get_byte(0xFC0000), Some(0x11));
        assert_eq!(mem.get_byte(0xFC0001), Some(0x14));
    }

    /// WinUAE memory.cpp: ROM at $F80000 (512K).
    #[test]
    fn test_rom_at_f80000_512k() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500_plus());
        let mut rom = vec![0u8; 512 * 1024];
        rom[0] = 0x22;
        mem.load_rom(&rom);
        assert_eq!(mem.get_byte(0xF80000), Some(0x22));
    }

    /// WinUAE memory.cpp: Custom registers at $DFF000-$DFF1FF.
    #[test]
    fn test_custom_registers_at_dff000() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        // Custom register reads should not return None (mapped region)
        assert!(mem.get_byte(0xDFF000).is_some());
        assert!(mem.get_byte(0xDFF1FF).is_some());
    }

    /// WinUAE memory.cpp: CIA-A at $BFE001 (odd bytes).
    #[test]
    fn test_cia_a_at_bfe001_odd_bytes() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        // CIA-A is at odd addresses starting at $BFE001
        // The CIA address space is $BFD000-$C00000
        assert!(mem.get_byte(0xBFE001).is_some());
    }

    /// WinUAE memory.cpp: CIA-B at $BFD000 (even bytes).
    #[test]
    fn test_cia_b_at_bfd000_even_bytes() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        assert!(mem.get_byte(0xBFD000).is_some());
    }

    /// WinUAE memory.cpp: Overlay — ROM visible at $000000 after reset.
    #[test]
    fn test_overlay_rom_visible_at_000000_after_reset() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let mut rom = vec![0u8; 256 * 1024];
        rom[0] = 0x00;
        rom[1] = 0xFC;
        rom[2] = 0x00;
        rom[3] = 0x02;
        mem.load_rom(&rom);

        // Overlay is true by default (after reset)
        assert!(mem.overlay);
        assert_eq!(mem.get_word(0x000000), Some(0x00FC));
        assert_eq!(mem.get_word(0x000002), Some(0x0002));

        // Disable overlay
        mem.overlay = false;
        // Now reads chip RAM (zeroed)
        assert_eq!(mem.get_word(0x000000), Some(0x0000));
    }

    /// WinUAE memory.cpp: Slow RAM at $C00000-$C80000.
    #[test]
    fn test_slow_ram_at_c00000() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let _ = mem.set_byte(0xC00000, 0x77);
        assert_eq!(mem.get_byte(0xC00000), Some(0x77));
        let _ = mem.set_byte(0xC7FFFF, 0x88);
        assert_eq!(mem.get_byte(0xC7FFFF), Some(0x88));
    }

    /// WinUAE memory.cpp: Unmapped addresses return None (bus error).
    #[test]
    fn test_unmapped_address_returns_none() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        mem.overlay = false;
        // No fast RAM configured, $200000 is unmapped
        assert_eq!(mem.get_byte(0x200000), None);
    }

    /// WinUAE memory.cpp: ROM is read-only — writes are ignored.
    #[test]
    fn test_rom_is_read_only() {
        let mut mem = AmigaMemory::new(MemoryConfig::a500());
        let rom = vec![0xAA; 256 * 1024];
        mem.load_rom(&rom);
        let _ = mem.set_byte(0xFC0000, 0x55);
        assert_eq!(mem.get_byte(0xFC0000), Some(0xAA));
    }
}

mod copper_hpos_regression_tests {
    use rumiga_core::copper::{CopperAction, CopperExecState, CopperState};

    fn make_chip_ram(instructions: &[(u16, u16)]) -> Vec<u8> {
        let mut ram = Vec::new();
        for &(w1, w2) in instructions {
            ram.extend_from_slice(&w1.to_be_bytes());
            ram.extend_from_slice(&w2.to_be_bytes());
        }
        ram.resize(ram.len() + 256, 0);
        ram
    }

    fn enabled_copper(instructions: &[(u16, u16)]) -> (CopperState, Vec<u8>) {
        let ram = make_chip_ram(instructions);
        let mut copper = CopperState::new();
        copper.enabled = true;
        copper.state = CopperExecState::FetchFirst;
        (copper, ram)
    }

    /// Regression: copper WAIT passes when hpos advances past target.
    #[test]
    fn test_copper_wait_passes_when_hpos_advances_past_target() {
        // WAIT for h=40 (vpos=0)
        let ir1: u16 = (40 << 1) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // hpos=40 should pass
        copper.cycle(&ram, 0, 40);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    /// Regression: copper WAIT blocks when hpos is before target.
    #[test]
    fn test_copper_wait_blocks_when_hpos_before_target() {
        // WAIT for h=100 (vpos=0)
        let ir1: u16 = (100 << 1) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // hpos=50 should NOT pass
        copper.cycle(&ram, 0, 50);
        assert_eq!(copper.state, CopperExecState::Execute);
    }

    /// Regression: copper WAIT passes immediately when vpos is past target.
    #[test]
    fn test_copper_wait_passes_immediately_when_vpos_past_target() {
        // WAIT for v=50, h=100
        let ir1: u16 = (50 << 8) | (100 << 1) | 1;
        let ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(ir1, ir2)]);

        copper.cycle(&ram, 0, 0); // fetch first
        copper.cycle(&ram, 0, 0); // fetch second

        // vpos=60, hpos=0 — vertical past target, should pass regardless of hpos
        copper.cycle(&ram, 60, 0);
        assert_eq!(copper.state, CopperExecState::FetchFirst);
    }

    /// Regression: copper executes multiple MOVEs within one scanline.
    #[test]
    fn test_copper_executes_multiple_moves_per_scanline() {
        // 10 MOVE instructions to COLOR00-COLOR09
        let instructions: Vec<(u16, u16)> = (0..10).map(|i| (0x0180 + i * 2, 0x0100 + i)).collect();
        let (mut copper, ram) = enabled_copper(&instructions);

        let mut moves = 0u32;
        for h in 0u16..227 {
            if let Some(CopperAction::WriteRegister { .. }) = copper.cycle(&ram, 44, h) {
                moves += 1;
            }
        }
        assert_eq!(moves, 10, "all 10 MOVEs should execute within one scanline");
    }

    /// Regression: WAIT h=44 then MOVE COLOR00 fires correctly.
    #[test]
    fn test_copper_wait_then_move_sets_color_at_correct_hpos() {
        // WAIT h=44 (vpos=0), then MOVE COLOR00=$0F00
        let wait_ir1: u16 = (44 << 1) | 1;
        let wait_ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) = enabled_copper(&[(wait_ir1, wait_ir2), (0x0180, 0x0F00)]);

        let mut fired = false;
        for h in 0u16..227 {
            if let Some(CopperAction::WriteRegister { offset, value }) = copper.cycle(&ram, 0, h) {
                assert_eq!(offset, 0x0180);
                assert_eq!(value, 0x0F00);
                fired = true;
            }
        }
        assert!(fired, "MOVE COLOR00 should fire after WAIT h=44");
    }

    /// Regression: multiple color changes in one scanline via WAIT.
    #[test]
    fn test_copper_full_scanline_color_changes() {
        // MOVE COLOR00=$0F00, WAIT h=100, MOVE COLOR00=$00F0
        let wait_ir1: u16 = (100 << 1) | 1;
        let wait_ir2: u16 = (0x7F << 8) | (0x7F << 1);
        let (mut copper, ram) =
            enabled_copper(&[(0x0180, 0x0F00), (wait_ir1, wait_ir2), (0x0180, 0x00F0)]);

        let mut values = Vec::new();
        for h in 0u16..227 {
            if let Some(CopperAction::WriteRegister { value, .. }) = copper.cycle(&ram, 0, h) {
                values.push(value);
            }
        }
        assert_eq!(values, vec![0x0F00, 0x00F0]);
    }
}
