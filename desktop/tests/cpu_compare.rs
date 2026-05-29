use r68k_emu::cpu::ConfiguredCore;
use r68k_emu::cpu::Core;
use r68k_emu::interrupts::AutoInterruptController;
use r68k_emu::ram::{AddressBus, AddressSpace};
use std::fs;
use std::path::PathBuf;

struct FlatMem {
    data: Vec<u8>,
}
impl FlatMem {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0; size],
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
impl AddressBus for FlatMem {
    fn copy_from(&mut self, other: &Self) {
        self.data.copy_from_slice(&other.data);
    }
    fn read_byte(&self, _: AddressSpace, addr: u32) -> u32 {
        self.data
            .get((addr & 0x00FF_FFFF) as usize)
            .map_or(0xFF, |&b| u32::from(b))
    }
    fn read_word(&self, _: AddressSpace, addr: u32) -> u32 {
        let a = (addr & 0x00FF_FFFE) as usize;
        if a + 1 < self.data.len() {
            (u32::from(self.data[a]) << 8) | u32::from(self.data[a + 1])
        } else {
            0xFFFF
        }
    }
    fn read_long(&self, _: AddressSpace, addr: u32) -> u32 {
        let a = (addr & 0x00FF_FFFC) as usize;
        if a + 3 < self.data.len() {
            (u32::from(self.data[a]) << 24)
                | (u32::from(self.data[a + 1]) << 16)
                | (u32::from(self.data[a + 2]) << 8)
                | u32::from(self.data[a + 3])
        } else {
            0xFFFF_FFFF
        }
    }
    fn write_byte(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0x00FF_FFFF) as usize;
        if a < self.data.len() {
            self.data[a] = val as u8;
        }
    }
    fn write_word(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0x00FF_FFFE) as usize;
        if a + 1 < self.data.len() {
            self.data[a] = (val >> 8) as u8;
            self.data[a + 1] = val as u8;
        }
    }
    fn write_long(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0x00FF_FFFC) as usize;
        if a + 3 < self.data.len() {
            self.data[a] = (val >> 24) as u8;
            self.data[a + 1] = (val >> 16) as u8;
            self.data[a + 2] = (val >> 8) as u8;
            self.data[a + 3] = val as u8;
        }
    }
}

fn rom_path() -> PathBuf {
    PathBuf::from(env!("HOME")).join("Documents/retro/amiga_winuae/rom/kick.a500.34.005.rom")
}

#[test]
fn compare_first_1000() {
    use rumiga_core::emulator::Emulator;
    use rumiga_core::memory::MemoryConfig;

    let rom_file = rom_path();
    if !rom_file.exists() {
        return;
    }
    let rom = fs::read(&rom_file).unwrap();

    let mut mem = FlatMem::new(0x0100_0000);
    mem.data[0x00FC_0000..0x00FC_0000 + rom.len()].copy_from_slice(&rom);
    mem.data[0..rom.len()].copy_from_slice(&rom);

    let int_ctrl = AutoInterruptController::new();
    let mut r68k = ConfiguredCore::new_with(0, int_ctrl, mem);
    r68k.reset();

    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);

    for i in 0..5000 {
        let r_pc = *r68k.pc();
        let m_pc = emu.cpu.pc;

        if r_pc != m_pc {
            let r_dar = *r68k.dar();
            let m_dar = emu.cpu.dar;
            println!("DIVERGE at instruction {i}: r68k PC={r_pc:08X} m68000 PC={m_pc:08X}");
            println!(
                "  r68k: D0={:08X} D1={:08X} A0={:08X} A7={:08X}",
                r_dar[0], r_dar[1], r_dar[8], r_dar[15]
            );
            println!(
                "  m68k: D0={:08X} D1={:08X} A0={:08X} A7={:08X}",
                m_dar[0], m_dar[1], m_dar[8], m_dar[15]
            );
            break;
        }

        r68k.execute1();
        emu.step_instruction();
    }

    let r_pc = *r68k.pc();
    let m_pc = emu.cpu.pc;
    if r_pc == m_pc {
        println!("Both CPUs agree after 5000 instructions! PC={r_pc:08X}");
    }
}
