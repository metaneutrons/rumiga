// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Amiga OCS/ECS custom chip register definitions (SSOT).
//!
//! All registers are word-addressed at offsets from `$DFF000`.
//! Offsets range from `0x000` to `0x1FE`.

/// Blitter destination early read (dummy).
pub const BLTDDAT: u16 = 0x000;
/// DMA control and blitter status read.
pub const DMACONR: u16 = 0x002;
/// Vertical most-significant bits and frame flop read.
pub const VPOSR: u16 = 0x004;
/// Vertical and horizontal beam position read.
pub const VHPOSR: u16 = 0x006;
/// Disk data early read (dummy).
pub const DSKDATR: u16 = 0x008;
/// Joystick-mouse 0 data.
pub const JOY0DAT: u16 = 0x00A;
/// Joystick-mouse 1 data.
pub const JOY1DAT: u16 = 0x00C;
/// Collision data (read and clear).
pub const CLXDAT: u16 = 0x00E;
/// Audio/disk control read.
pub const ADKCONR: u16 = 0x010;
/// Pot counter pair 0 data.
pub const POT0DAT: u16 = 0x012;
/// Pot counter pair 1 data.
pub const POT1DAT: u16 = 0x014;
/// Pot pin data read.
pub const POTGOR: u16 = 0x016;
/// Serial port data and status read.
pub const SERDATR: u16 = 0x018;
/// Disk data byte and status read.
pub const DSKBYTR: u16 = 0x01A;
/// Interrupt enable bits read.
pub const INTENAR: u16 = 0x01C;
/// Interrupt request bits read.
pub const INTREQR: u16 = 0x01E;
/// Disk pointer high.
pub const DSKPTH: u16 = 0x020;
/// Disk pointer low.
pub const DSKPTL: u16 = 0x022;
/// Disk length.
pub const DSKLEN: u16 = 0x024;
/// Disk DMA data write.
pub const DSKDAT: u16 = 0x026;
/// Refresh pointer.
pub const REFPTR: u16 = 0x028;
/// Write vertical most-significant bits.
pub const VPOSW: u16 = 0x02A;
/// Write vertical and horizontal position.
pub const VHPOSW: u16 = 0x02C;
/// Coprocessor control (CDANG).
pub const COPCON: u16 = 0x02E;
/// Serial port data write.
pub const SERDAT: u16 = 0x030;
/// Serial port period and control.
pub const SERPER: u16 = 0x032;
/// Pot count start, pot pin drive enable.
pub const POTGO: u16 = 0x034;
/// Write to all joystick-mouse counters.
pub const JOYTEST: u16 = 0x036;
/// Strobe for hsync with VB and EQU.
pub const STREQU: u16 = 0x038;
/// Strobe for hsync with VB.
pub const STRVBL: u16 = 0x03A;
/// Strobe for hsync.
pub const STRHOR: u16 = 0x03C;
/// Strobe for long line identification.
pub const STRLONG: u16 = 0x03E;
/// Blitter control register 0.
pub const BLTCON0: u16 = 0x040;
/// Blitter control register 1.
pub const BLTCON1: u16 = 0x042;
/// Blitter first word mask for source A.
pub const BLTAFWM: u16 = 0x044;
/// Blitter last word mask for source A.
pub const BLTALWM: u16 = 0x046;
/// Blitter pointer to source C (high).
pub const BLTCPTH: u16 = 0x048;
/// Blitter pointer to source C (low).
pub const BLTCPTL: u16 = 0x04A;
/// Blitter pointer to source B (high).
pub const BLTBPTH: u16 = 0x04C;
/// Blitter pointer to source B (low).
pub const BLTBPTL: u16 = 0x04E;
/// Blitter pointer to source A (high).
pub const BLTAPTH: u16 = 0x050;
/// Blitter pointer to source A (low).
pub const BLTAPTL: u16 = 0x052;
/// Blitter pointer to destination D (high).
pub const BLTDPTH: u16 = 0x054;
/// Blitter pointer to destination D (low).
pub const BLTDPTL: u16 = 0x056;
/// Blitter start and size (width, height).
pub const BLTSIZE: u16 = 0x058;
/// Blitter control 0 lower 8 bits (ECS).
pub const BLTCON0L: u16 = 0x05A;
/// Blitter V size (ECS).
pub const BLTSIZV: u16 = 0x05C;
/// Blitter H size and start (ECS).
pub const BLTSIZH: u16 = 0x05E;
/// Blitter modulo for source C.
pub const BLTCMOD: u16 = 0x060;
/// Blitter modulo for source B.
pub const BLTBMOD: u16 = 0x062;
/// Blitter modulo for source A.
pub const BLTAMOD: u16 = 0x064;
/// Blitter modulo for destination D.
pub const BLTDMOD: u16 = 0x066;
/// Blitter source C data.
pub const BLTCDAT: u16 = 0x070;
/// Blitter source B data.
pub const BLTBDAT: u16 = 0x072;
/// Blitter source A data.
pub const BLTADAT: u16 = 0x074;
/// Disk sync pattern.
pub const DSKSYNC: u16 = 0x07E;
/// Copper first location (high).
pub const COP1LCH: u16 = 0x080;
/// Copper first location (low).
pub const COP1LCL: u16 = 0x082;
/// Copper second location (high).
pub const COP2LCH: u16 = 0x084;
/// Copper second location (low).
pub const COP2LCL: u16 = 0x086;
/// Copper restart at first location.
pub const COPJMP1: u16 = 0x088;
/// Copper restart at second location.
pub const COPJMP2: u16 = 0x08A;
/// Copper instruction fetch identify.
pub const COPINS: u16 = 0x08C;
/// Display window start (upper-left).
pub const DIWSTRT: u16 = 0x08E;
/// Display window stop (lower-right).
pub const DIWSTOP: u16 = 0x090;
/// Display data fetch start.
pub const DDFSTRT: u16 = 0x092;
/// Display data fetch stop.
pub const DDFSTOP: u16 = 0x094;
/// DMA control write (set/clear).
pub const DMACON: u16 = 0x096;
/// Collision control.
pub const CLXCON: u16 = 0x098;
/// Interrupt enable write (set/clear).
pub const INTENA: u16 = 0x09A;
/// Interrupt request write (set/clear).
pub const INTREQ: u16 = 0x09C;
/// Audio/disk/UART control.
pub const ADKCON: u16 = 0x09E;
/// Audio channel 0 location (high).
pub const AUD0LCH: u16 = 0x0A0;
/// Audio channel 0 location (low).
pub const AUD0LCL: u16 = 0x0A2;
/// Audio channel 0 length.
pub const AUD0LEN: u16 = 0x0A4;
/// Audio channel 0 period.
pub const AUD0PER: u16 = 0x0A6;
/// Audio channel 0 volume.
pub const AUD0VOL: u16 = 0x0A8;
/// Audio channel 0 data.
pub const AUD0DAT: u16 = 0x0AA;
/// Audio channel 1 location (high).
pub const AUD1LCH: u16 = 0x0B0;
/// Audio channel 1 location (low).
pub const AUD1LCL: u16 = 0x0B2;
/// Audio channel 1 length.
pub const AUD1LEN: u16 = 0x0B4;
/// Audio channel 1 period.
pub const AUD1PER: u16 = 0x0B6;
/// Audio channel 1 volume.
pub const AUD1VOL: u16 = 0x0B8;
/// Audio channel 1 data.
pub const AUD1DAT: u16 = 0x0BA;
/// Audio channel 2 location (high).
pub const AUD2LCH: u16 = 0x0C0;
/// Audio channel 2 location (low).
pub const AUD2LCL: u16 = 0x0C2;
/// Audio channel 2 length.
pub const AUD2LEN: u16 = 0x0C4;
/// Audio channel 2 period.
pub const AUD2PER: u16 = 0x0C6;
/// Audio channel 2 volume.
pub const AUD2VOL: u16 = 0x0C8;
/// Audio channel 2 data.
pub const AUD2DAT: u16 = 0x0CA;
/// Audio channel 3 location (high).
pub const AUD3LCH: u16 = 0x0D0;
/// Audio channel 3 location (low).
pub const AUD3LCL: u16 = 0x0D2;
/// Audio channel 3 length.
pub const AUD3LEN: u16 = 0x0D4;
/// Audio channel 3 period.
pub const AUD3PER: u16 = 0x0D6;
/// Audio channel 3 volume.
pub const AUD3VOL: u16 = 0x0D8;
/// Audio channel 3 data.
pub const AUD3DAT: u16 = 0x0DA;
/// Bitplane 1 pointer (high).
pub const BPL1PTH: u16 = 0x0E0;
/// Bitplane 1 pointer (low).
pub const BPL1PTL: u16 = 0x0E2;
/// Bitplane 2 pointer (high).
pub const BPL2PTH: u16 = 0x0E4;
/// Bitplane 2 pointer (low).
pub const BPL2PTL: u16 = 0x0E6;
/// Bitplane 3 pointer (high).
pub const BPL3PTH: u16 = 0x0E8;
/// Bitplane 3 pointer (low).
pub const BPL3PTL: u16 = 0x0EA;
/// Bitplane 4 pointer (high).
pub const BPL4PTH: u16 = 0x0EC;
/// Bitplane 4 pointer (low).
pub const BPL4PTL: u16 = 0x0EE;
/// Bitplane 5 pointer (high).
pub const BPL5PTH: u16 = 0x0F0;
/// Bitplane 5 pointer (low).
pub const BPL5PTL: u16 = 0x0F2;
/// Bitplane 6 pointer (high).
pub const BPL6PTH: u16 = 0x0F4;
/// Bitplane 6 pointer (low).
pub const BPL6PTL: u16 = 0x0F6;
/// Bitplane control register 0.
pub const BPLCON0: u16 = 0x100;
/// Bitplane control register 1 (scroll).
pub const BPLCON1: u16 = 0x102;
/// Bitplane control register 2 (priority).
pub const BPLCON2: u16 = 0x104;
/// Bitplane control register 3 (ECS).
pub const BPLCON3: u16 = 0x106;
/// Bitplane modulo (odd planes).
pub const BPL1MOD: u16 = 0x108;
/// Bitplane modulo (even planes).
pub const BPL2MOD: u16 = 0x10A;
/// Bitplane 1 data.
pub const BPL1DAT: u16 = 0x110;
/// Bitplane 2 data.
pub const BPL2DAT: u16 = 0x112;
/// Bitplane 3 data.
pub const BPL3DAT: u16 = 0x114;
/// Bitplane 4 data.
pub const BPL4DAT: u16 = 0x116;
/// Bitplane 5 data.
pub const BPL5DAT: u16 = 0x118;
/// Bitplane 6 data.
pub const BPL6DAT: u16 = 0x11A;
/// Sprite 0 pointer (high).
pub const SPR0PTH: u16 = 0x120;
/// Sprite 0 pointer (low).
pub const SPR0PTL: u16 = 0x122;
/// Sprite 1 pointer (high).
pub const SPR1PTH: u16 = 0x124;
/// Sprite 1 pointer (low).
pub const SPR1PTL: u16 = 0x126;
/// Sprite 2 pointer (high).
pub const SPR2PTH: u16 = 0x128;
/// Sprite 2 pointer (low).
pub const SPR2PTL: u16 = 0x12A;
/// Sprite 3 pointer (high).
pub const SPR3PTH: u16 = 0x12C;
/// Sprite 3 pointer (low).
pub const SPR3PTL: u16 = 0x12E;
/// Sprite 4 pointer (high).
pub const SPR4PTH: u16 = 0x130;
/// Sprite 4 pointer (low).
pub const SPR4PTL: u16 = 0x132;
/// Sprite 5 pointer (high).
pub const SPR5PTH: u16 = 0x134;
/// Sprite 5 pointer (low).
pub const SPR5PTL: u16 = 0x136;
/// Sprite 6 pointer (high).
pub const SPR6PTH: u16 = 0x138;
/// Sprite 6 pointer (low).
pub const SPR6PTL: u16 = 0x13A;
/// Sprite 7 pointer (high).
pub const SPR7PTH: u16 = 0x13C;
/// Sprite 7 pointer (low).
pub const SPR7PTL: u16 = 0x13E;
/// Sprite 0 vertical-horizontal start position.
pub const SPR0POS: u16 = 0x140;
/// Sprite 0 control.
pub const SPR0CTL: u16 = 0x142;
/// Sprite 0 image data A.
pub const SPR0DATA: u16 = 0x144;
/// Sprite 0 image data B.
pub const SPR0DATB: u16 = 0x146;
/// Sprite 1 vertical-horizontal start position.
pub const SPR1POS: u16 = 0x148;
/// Sprite 1 control.
pub const SPR1CTL: u16 = 0x14A;
/// Sprite 1 image data A.
pub const SPR1DATA: u16 = 0x14C;
/// Sprite 1 image data B.
pub const SPR1DATB: u16 = 0x14E;
/// Sprite 2 vertical-horizontal start position.
pub const SPR2POS: u16 = 0x150;
/// Sprite 2 control.
pub const SPR2CTL: u16 = 0x152;
/// Sprite 2 image data A.
pub const SPR2DATA: u16 = 0x154;
/// Sprite 2 image data B.
pub const SPR2DATB: u16 = 0x156;
/// Sprite 3 vertical-horizontal start position.
pub const SPR3POS: u16 = 0x158;
/// Sprite 3 control.
pub const SPR3CTL: u16 = 0x15A;
/// Sprite 3 image data A.
pub const SPR3DATA: u16 = 0x15C;
/// Sprite 3 image data B.
pub const SPR3DATB: u16 = 0x15E;
/// Sprite 4 vertical-horizontal start position.
pub const SPR4POS: u16 = 0x160;
/// Sprite 4 control.
pub const SPR4CTL: u16 = 0x162;
/// Sprite 4 image data A.
pub const SPR4DATA: u16 = 0x164;
/// Sprite 4 image data B.
pub const SPR4DATB: u16 = 0x166;
/// Sprite 5 vertical-horizontal start position.
pub const SPR5POS: u16 = 0x168;
/// Sprite 5 control.
pub const SPR5CTL: u16 = 0x16A;
/// Sprite 5 image data A.
pub const SPR5DATA: u16 = 0x16C;
/// Sprite 5 image data B.
pub const SPR5DATB: u16 = 0x16E;
/// Sprite 6 vertical-horizontal start position.
pub const SPR6POS: u16 = 0x170;
/// Sprite 6 control.
pub const SPR6CTL: u16 = 0x172;
/// Sprite 6 image data A.
pub const SPR6DATA: u16 = 0x174;
/// Sprite 6 image data B.
pub const SPR6DATB: u16 = 0x176;
/// Sprite 7 vertical-horizontal start position.
pub const SPR7POS: u16 = 0x178;
/// Sprite 7 control.
pub const SPR7CTL: u16 = 0x17A;
/// Sprite 7 image data A.
pub const SPR7DATA: u16 = 0x17C;
/// Sprite 7 image data B.
pub const SPR7DATB: u16 = 0x17E;
/// Color register 00.
pub const COLOR00: u16 = 0x180;
/// Color register 01.
pub const COLOR01: u16 = 0x182;
/// Color register 02.
pub const COLOR02: u16 = 0x184;
/// Color register 03.
pub const COLOR03: u16 = 0x186;
/// Color register 04.
pub const COLOR04: u16 = 0x188;
/// Color register 05.
pub const COLOR05: u16 = 0x18A;
/// Color register 06.
pub const COLOR06: u16 = 0x18C;
/// Color register 07.
pub const COLOR07: u16 = 0x18E;
/// Color register 08.
pub const COLOR08: u16 = 0x190;
/// Color register 09.
pub const COLOR09: u16 = 0x192;
/// Color register 10.
pub const COLOR10: u16 = 0x194;
/// Color register 11.
pub const COLOR11: u16 = 0x196;
/// Color register 12.
pub const COLOR12: u16 = 0x198;
/// Color register 13.
pub const COLOR13: u16 = 0x19A;
/// Color register 14.
pub const COLOR14: u16 = 0x19C;
/// Color register 15.
pub const COLOR15: u16 = 0x19E;
/// Color register 16.
pub const COLOR16: u16 = 0x1A0;
/// Color register 17.
pub const COLOR17: u16 = 0x1A2;
/// Color register 18.
pub const COLOR18: u16 = 0x1A4;
/// Color register 19.
pub const COLOR19: u16 = 0x1A6;
/// Color register 20.
pub const COLOR20: u16 = 0x1A8;
/// Color register 21.
pub const COLOR21: u16 = 0x1AA;
/// Color register 22.
pub const COLOR22: u16 = 0x1AC;
/// Color register 23.
pub const COLOR23: u16 = 0x1AE;
/// Color register 24.
pub const COLOR24: u16 = 0x1B0;
/// Color register 25.
pub const COLOR25: u16 = 0x1B2;
/// Color register 26.
pub const COLOR26: u16 = 0x1B4;
/// Color register 27.
pub const COLOR27: u16 = 0x1B6;
/// Color register 28.
pub const COLOR28: u16 = 0x1B8;
/// Color register 29.
pub const COLOR29: u16 = 0x1BA;
/// Color register 30.
pub const COLOR30: u16 = 0x1BC;
/// Color register 31.
pub const COLOR31: u16 = 0x1BE;
/// Horizontal total (ECS).
pub const HTOTAL: u16 = 0x1C0;
/// Horizontal sync stop (ECS).
pub const HSSTOP: u16 = 0x1C2;
/// Horizontal blank start (ECS).
pub const HBSTRT: u16 = 0x1C4;
/// Horizontal blank stop (ECS).
pub const HBSTOP: u16 = 0x1C6;
/// Vertical total (ECS).
pub const VTOTAL: u16 = 0x1C8;
/// Vertical sync stop (ECS).
pub const VSSTOP: u16 = 0x1CA;
/// Vertical blank start (ECS).
pub const VBSTRT: u16 = 0x1CC;
/// Vertical blank stop (ECS).
pub const VBSTOP: u16 = 0x1CE;
/// Beam counter control (ECS).
pub const BEAMCON0: u16 = 0x1DC;
/// Display window high bits (ECS).
pub const DIWHIGH: u16 = 0x1E4;
/// Fetch mode (AGA).
pub const FMODE: u16 = 0x1FC;

// --- DMA channel bit masks for DMACON ---

/// Audio channel 0 DMA.
pub const DMA_AUD0: u16 = 0x0001;
/// Audio channel 1 DMA.
pub const DMA_AUD1: u16 = 0x0002;
/// Audio channel 2 DMA.
pub const DMA_AUD2: u16 = 0x0004;
/// Audio channel 3 DMA.
pub const DMA_AUD3: u16 = 0x0008;
/// Disk DMA.
pub const DMA_DISK: u16 = 0x0010;
/// Sprite DMA.
pub const DMA_SPRITE: u16 = 0x0020;
/// Blitter DMA.
pub const DMA_BLITTER: u16 = 0x0040;
/// Copper DMA.
pub const DMA_COPPER: u16 = 0x0080;
/// Bitplane DMA.
pub const DMA_BITPLANE: u16 = 0x0100;
/// Master DMA enable (bit 9).
pub const DMA_MASTER: u16 = 0x0200;
/// Blitter priority (bit 10).
pub const DMA_BLITPRI: u16 = 0x0400;

// --- Interrupt bit masks ---

/// TBE — Serial transmit buffer empty.
pub const INT_TBE: u16 = 0x0001;
/// DSKBLK — Disk block finished.
pub const INT_DSKBLK: u16 = 0x0002;
/// SOFT — Software interrupt.
pub const INT_SOFT: u16 = 0x0004;
/// PORTS — I/O ports and timers (CIA-A).
pub const INT_PORTS: u16 = 0x0008;
/// COPER — Copper.
pub const INT_COPER: u16 = 0x0010;
/// VERTB — Vertical blank.
pub const INT_VERTB: u16 = 0x0020;
/// BLIT — Blitter finished.
pub const INT_BLIT: u16 = 0x0040;
/// AUD0 — Audio channel 0.
pub const INT_AUD0: u16 = 0x0080;
/// AUD1 — Audio channel 1.
pub const INT_AUD1: u16 = 0x0100;
/// AUD2 — Audio channel 2.
pub const INT_AUD2: u16 = 0x0200;
/// AUD3 — Audio channel 3.
pub const INT_AUD3: u16 = 0x0400;
/// RBF — Serial receive buffer full.
pub const INT_RBF: u16 = 0x0800;
/// DSKSYN — Disk sync value recognized.
pub const INT_DSKSYN: u16 = 0x1000;
/// EXTER — External interrupt (CIA-B).
pub const INT_EXTER: u16 = 0x2000;
/// Master interrupt enable (bit 14).
pub const INT_SETCLR: u16 = 0x4000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_offsets_match_hardware() {
        assert_eq!(DMACONR, 0x002);
        assert_eq!(VPOSR, 0x004);
        assert_eq!(VHPOSR, 0x006);
        assert_eq!(INTENAR, 0x01C);
        assert_eq!(INTREQR, 0x01E);
        assert_eq!(BLTCON0, 0x040);
        assert_eq!(BLTCON1, 0x042);
        assert_eq!(BLTSIZE, 0x058);
        assert_eq!(COP1LCH, 0x080);
        assert_eq!(COP2LCH, 0x084);
        assert_eq!(DIWSTRT, 0x08E);
        assert_eq!(DIWSTOP, 0x090);
        assert_eq!(DDFSTRT, 0x092);
        assert_eq!(DDFSTOP, 0x094);
        assert_eq!(DMACON, 0x096);
        assert_eq!(INTENA, 0x09A);
        assert_eq!(INTREQ, 0x09C);
        assert_eq!(BPLCON0, 0x100);
        assert_eq!(COLOR00, 0x180);
        assert_eq!(COLOR31, 0x1BE);
    }

    #[test]
    fn color_registers_are_contiguous() {
        assert_eq!(COLOR31 - COLOR00, 31 * 2);
    }
}
