# Architecture

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        Host / Device                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────────────────────────────┐   │
│  │  Web UI      │◄──►│  REST API (rumiga-api types)          │   │
│  │  (Next.js)   │    └──────────────┬───────────────────────┘   │
│  └──────────────┘                   │                           │
│                                     ▼                           │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Platform Backend                             │   │
│  │  ┌────────────────────┐  ┌────────────────────────────┐  │   │
│  │  │  Desktop (minifb)  │  │  ESP-IDF (MIPI-DSI, I2S)   │  │   │
│  │  └────────────────────┘  └────────────────────────────┘  │   │
│  └──────────────────────────────┬───────────────────────────┘   │
│                                 │ Platform traits               │
│                                 ▼                               │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                   rumiga-core                             │   │
│  │  ┌─────────┐  ┌────────┐  ┌────────┐  ┌─────────────┐  │   │
│  │  │ M68000  │  │Chipset │  │ Memory │  │   Events    │  │   │
│  │  │  CPU    │  │(Custom)│  │  Map   │  │  Scheduler  │  │   │
│  │  └────┬────┘  └───┬────┘  └───┬────┘  └──────┬──────┘  │   │
│  │       │            │           │              │          │   │
│  │       ▼            ▼           ▼              ▼          │   │
│  │  ┌─────────────────────────────────────────────────┐     │   │
│  │  │              Emulation Loop                     │     │   │
│  │  │  (frame-based: CPU + Copper + Blitter + DMA)   │     │   │
│  │  └─────────────────────────────────────────────────┘     │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Crate Dependency Graph

```
rumiga-desktop (bin)
  └── rumiga-platform-desktop
        ├── rumiga-platform (traits)
        ├── rumiga-core
        │     └── m68000
        └── minifb, cpal (external)

rumiga-firmware (bin, ESP-IDF)
  └── rumiga-platform-esp
        ├── rumiga-platform (traits)
        ├── rumiga-core
        │     └── m68000
        └── esp-idf-svc, esp-idf-hal (external)

rumiga-api (lib)
  └── (standalone — shared types for REST API)
```

## Emulation Loop

The emulator runs frame-by-frame, driven by the PAL timing of 312 scanlines per frame at 50 Hz:

1. **Frame start**: Reset beam position to (0, 0).
2. **Per scanline** (312 iterations):
   a. Execute M68000 CPU for 454 cycles (227 color clocks × 2).
   b. Advance the Copper coprocessor — execute MOVE/WAIT/SKIP.
   c. Run Blitter DMA if active (steals cycles from CPU).
   d. Advance CIA timers and check for interrupts.
   e. Process event scheduler — fire due events (audio, disk, VSYNC).
   f. If in visible area (lines 44–299): render playfield + sprites into framebuffer.
3. **Frame end**: Present framebuffer via `VideoOutput::present_frame()`, flush audio via `AudioOutput::queue_samples()`.

The event scheduler uses a priority queue sorted by cycle count, enabling sub-scanline precision for audio period changes, disk DMA, and interrupt timing.

## Platform Abstraction Design

All hardware interaction is behind traits defined in `rumiga-platform`:

| Trait          | Purpose                              | Desktop impl        | ESP impl           |
|----------------|--------------------------------------|---------------------|---------------------|
| `VideoOutput`  | Present RGB565 framebuffer           | minifb window       | MIPI-DSI + OSD     |
| `AudioOutput`  | Queue stereo PCM samples             | cpal stream         | I2S (ES8311)       |
| `InputSource`  | Poll keyboard, mouse, joystick      | minifb keys + mouse | USB HID + touch    |
| `Storage`      | File I/O for disk images             | std::fs             | SD/MMC FAT32       |

The binary (desktop or firmware) instantiates concrete implementations and passes them to the emulation loop. The core crate never imports platform-specific code.

## Memory Map

The Amiga memory map as implemented in `rumiga-core::memory`:

```
0x000000–0x1FFFFF  Chip RAM (512 KB – 2 MB, configurable)
0x200000–0x9FFFFF  (Reserved / AutoConfig Fast RAM)
0xA00000–0xBEFFFF  CIA space
  0xBFD000         CIA-A (keyboard, disk, parallel)
  0xBFE001         CIA-B (serial, timers)
0xBF0000–0xBFFFFF  (Reserved)
0xC00000–0xD7FFFF  Slow RAM (optional 512 KB at 0xC00000)
0xD80000–0xDBFFFF  (Reserved)
0xDC0000–0xDCFFFF  RTC (optional)
0xDD0000–0xDDFFFF  (Reserved)
0xDE0000–0xDEFFFF  (Reserved)
0xDF0000–0xDFFFFF  Custom chip registers (OCS/ECS/AGA)
  0xDFF000–0xDFF1FF  Custom registers (Denise, Agnus, Paula)
0xE00000–0xE7FFFF  (Reserved)
0xE80000–0xEFFFFF  AutoConfig space
0xF00000–0xF7FFFF  (Reserved / diagnostic ROM)
0xF80000–0xFFFFFF  Kickstart ROM (256 KB or 512 KB)
```

## How to Add a New Platform Backend

1. **Create the crate**: `crates/rumiga-platform-<name>/`
   - Add `Cargo.toml` with dependencies on `rumiga-platform` and `rumiga-core`.
   - Implement the four traits: `VideoOutput`, `AudioOutput`, `InputSource`, `Storage`.

2. **Implement traits**:
   ```rust
   pub struct MyVideo { /* ... */ }

   impl rumiga_platform::VideoOutput for MyVideo {
       fn present_frame(&mut self, framebuffer: &[u16], width: u32, height: u32) {
           // Blit framebuffer to your display
       }
   }
   // ... repeat for AudioOutput, InputSource, Storage
   ```

3. **Create a binary**: Add a binary crate (e.g., `my-target/`) that:
   - Instantiates your platform implementations.
   - Creates an `Emulator` from `rumiga-core`.
   - Runs the frame loop calling `emulator.run_frame()` and presenting output.

4. **Wire into workspace** (optional for out-of-tree backends):
   - Add the crate to `Cargo.toml` workspace members.
   - Add CI steps if needed.

5. **Test**: The core crate has its own test suite. Platform crates should add integration tests verifying trait contracts (e.g., framebuffer dimensions, sample rates).
