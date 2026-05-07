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
impl AddressBus for FlatMem {
    fn copy_from(&mut self, other: &Self) {
        self.data.copy_from_slice(&other.data);
    }
    fn read_byte(&self, _: AddressSpace, addr: u32) -> u32 {
        self.data
            .get((addr & 0xFFFFFF) as usize)
            .map_or(0xFF, |&b| b as u32)
    }
    fn read_word(&self, _: AddressSpace, addr: u32) -> u32 {
        let a = (addr & 0xFFFFFE) as usize;
        if a + 1 < self.data.len() {
            ((self.data[a] as u32) << 8) | self.data[a + 1] as u32
        } else {
            0xFFFF
        }
    }
    fn read_long(&self, _: AddressSpace, addr: u32) -> u32 {
        let a = (addr & 0xFFFFFC) as usize;
        if a + 3 < self.data.len() {
            ((self.data[a] as u32) << 24)
                | ((self.data[a + 1] as u32) << 16)
                | ((self.data[a + 2] as u32) << 8)
                | self.data[a + 3] as u32
        } else {
            0xFFFFFFFF
        }
    }
    fn write_byte(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0xFFFFFF) as usize;
        if a < self.data.len() {
            self.data[a] = val as u8;
        }
    }
    fn write_word(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0xFFFFFE) as usize;
        if a + 1 < self.data.len() {
            self.data[a] = (val >> 8) as u8;
            self.data[a + 1] = val as u8;
        }
    }
    fn write_long(&mut self, _: AddressSpace, addr: u32, val: u32) {
        let a = (addr & 0xFFFFFC) as usize;
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
    let rom_file = rom_path();
    if !rom_file.exists() {
        return;
    }
    let rom = fs::read(&rom_file).unwrap();

    let mut mem = FlatMem::new(0x100_0000);
    mem.data[0xFC0000..0xFC0000 + rom.len()].copy_from_slice(&rom);
    mem.data[0..rom.len()].copy_from_slice(&rom);

    let int_ctrl = AutoInterruptController::new();
    let mut r68k = ConfiguredCore::new_with(0, int_ctrl, mem);
    r68k.reset();

    // Run both CPUs instruction by instruction and compare
    use rumiga_core::emulator::Emulator;
    use rumiga_core::memory::MemoryConfig;

    let mut emu = Emulator::new(MemoryConfig::a500());
    emu.load_rom(&rom);
    // Our CPU needs a reset too
    emu.cpu.interpreter(&mut emu.memory); // process reset exception

    for i in 0..5000 {
        let r_pc = {
            let p = r68k.pc();
            *p
        };
        let m_pc = emu.cpu.regs.pc.0;

        if r_pc != m_pc {
            let r_dar = *r68k.dar();
            println!(
                "DIVERGE at instruction {}: r68k PC={:08X} m68000 PC={:08X}",
                i, r_pc, m_pc
            );
            println!(
                "  r68k: D0={:08X} D1={:08X} A0={:08X} A7={:08X}",
                r_dar[0], r_dar[1], r_dar[8], r_dar[15]
            );
            println!(
                "  m68k: D0={:08X} D1={:08X} A0={:08X} A7={:08X}",
                emu.cpu.regs.d[0].0, emu.cpu.regs.d[1].0, emu.cpu.regs.a[0].0, emu.cpu.regs.a[6].0
            );
            break;
        }

        r68k.execute1();
        emu.step_instruction();
    }

    let r_pc = {
        let p = r68k.pc();
        *p
    };
    let m_pc = emu.cpu.regs.pc.0;
    if r_pc == m_pc {
        println!("Both CPUs agree after 5000 instructions! PC={:08X}", r_pc);
    }
}
