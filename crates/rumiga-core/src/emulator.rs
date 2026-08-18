// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Main emulation loop tying CPU and chipset together.

#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use m68k::AddressBus;
use m68k::CpuCore;
use m68k::{NoOpHleHandler, StepResult};
use rumiga_platform::TraceSink;

use core::time::Duration;

use crate::digest::StateDigest;

use crate::audio::AudioState;
use crate::blitter::BlitterState;
use crate::chipset::CustomChipState;
use crate::copper::{CopperAction, CopperState};
use crate::custom;
use crate::events::{EventScheduler, EventType};
use crate::floppy::FloppyController;
use crate::memory::{AmigaMemory, MemoryConfig};
use crate::network::MacAddress;
use crate::playfield::{self, PlayfieldState};
use crate::sprites::SpriteEngine;
use crate::video::VideoStandard;
use rumiga_platform::{BoundedQueue, OverflowPolicy, QueueAdmission};

use crate::replay::{InputEvent, InputRecorder, InputRecording, ReplayError};

/// CPU cycles per scanline (227 color clocks × 2).
const CYCLES_PER_LINE: usize = 227 * 2;

/// Display width in pixels.
const DISPLAY_WIDTH: usize = playfield::DISPLAY_WIDTH as usize;

/// Framebuffer size in pixels.
const FRAMEBUFFER_SIZE: usize = DISPLAY_WIDTH * playfield::DISPLAY_HEIGHT as usize;

/// Capacity of the guest keyboard queue.
///
/// The queue drains one event every three frames, so about 17 events per second under
/// PAL. A burst longer than this capacity loses events, which is what the queue's
/// counters exist to make visible.
const MAX_KEY_EVENTS: usize = 16;

/// Overflow policy for the guest keyboard queue.
///
/// Refusing the newest event preserves the order of what is already typed, which is what
/// a full keyboard buffer does on real hardware: the keystrokes are lost at the source
/// rather than reordered. This matches the behaviour the previous unnamed bound had, so
/// the policy makes an existing decision explicit rather than changing it.
const KEY_EVENT_OVERFLOW: OverflowPolicy = OverflowPolicy::RejectNewest;

/// Number of words dumped per bitplane in scanline capture manifests.
pub const VIDEO_SCANLINE_WORD_DUMP: usize = 48;

/// Number of early active scanlines retained for video capture diagnostics.
pub const EARLY_VIDEO_SCANLINE_DUMP: usize = 24;

/// Video register snapshot from the most recent rendered bitplane scanline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoScanlineSnapshot {
    /// Beam vertical position.
    pub vpos: u16,
    /// Destination framebuffer line.
    pub framebuffer_line: u16,
    /// Display window horizontal start.
    pub hstart: u16,
    /// Display window horizontal stop.
    pub hstop: u16,
    /// Display window vertical start.
    pub vstart: u16,
    /// Display window vertical stop.
    pub vstop: u16,
    /// Bitplane control register 0.
    pub bplcon0: u16,
    /// Bitplane control register 1.
    pub bplcon1: u16,
    /// Display data fetch start.
    pub ddfstrt: u16,
    /// Display data fetch stop.
    pub ddfstop: u16,
    /// Odd bitplane modulo.
    pub bpl1mod: u16,
    /// Even bitplane modulo.
    pub bpl2mod: u16,
    /// Bitplane pointers at the start of this scanline.
    pub bplpt: [u32; playfield::MAX_PLANES],
    /// Chip RAM words sampled from each bitplane pointer at the start of this scanline.
    pub bitplane_words: [[u16; VIDEO_SCANLINE_WORD_DUMP]; playfield::MAX_PLANES],
    /// Active plane count.
    pub num_planes: usize,
}

fn snapshot_bitplane_words(
    chip_ram: &[u8],
    bplpt: [u32; playfield::MAX_PLANES],
    num_planes: usize,
) -> [[u16; VIDEO_SCANLINE_WORD_DUMP]; playfield::MAX_PLANES] {
    let mut words = [[0; VIDEO_SCANLINE_WORD_DUMP]; playfield::MAX_PLANES];
    if chip_ram.is_empty() {
        return words;
    }

    for (plane, plane_words) in words
        .iter_mut()
        .enumerate()
        .take(num_planes.min(playfield::MAX_PLANES))
    {
        let base = bplpt[plane] as usize;
        for (word_index, word) in plane_words.iter_mut().enumerate() {
            let addr = (base + word_index * 2) % chip_ram.len();
            if addr + 1 < chip_ram.len() {
                *word = u16::from_be_bytes([chip_ram[addr], chip_ram[addr + 1]]);
            }
        }
    }
    words
}

/// Helper to translate raw Amiga keycodes to the required CIA-A SDR register format.
/// The Amiga ROM keyboard handler decodes SDR via: decoded = ror( ~SDR, 1 )
/// Therefore, the required SDR value is: SDR = ~( rol( decoded, 1 ) )
const fn translate_amiga_keycode(keycode: u8) -> u8 {
    !keycode.rotate_left(1)
}

/// Main emulator state combining CPU and all chipset subsystems.
pub struct Emulator {
    /// m68k CPU core.
    pub cpu: CpuCore,
    /// Amiga memory subsystem.
    pub memory: AmigaMemory,
    /// Custom chip register state.
    pub chipset: CustomChipState,
    /// Cycle-accurate event scheduler.
    pub events: EventScheduler,
    /// Copper coprocessor.
    pub copper: CopperState,
    /// Bitplane/playfield renderer.
    pub playfield: PlayfieldState,
    /// Blitter DMA engine.
    pub blitter: BlitterState,
    /// Floppy disk controller.
    pub floppy: FloppyController,
    /// Audio subsystem.
    pub audio: AudioState,
    /// Sprite engine.
    pub sprites: SpriteEngine,
    /// RGB565 framebuffer, sized for the taller of the two video standards.
    ///
    /// PAL is taller than NTSC, so this buffer holds either. Under NTSC the
    /// lines beyond the active height are never written.
    pub framebuffer: Vec<u16>,
    /// Video standard the chipset runs at.
    video_standard: VideoStandard,
    /// Frames completed since construction.
    ///
    /// This is the replay clock. It lives in the machine rather than in a shell so
    /// that a recording's frame indices mean the same thing to every caller.
    frames_run: u64,
    /// Optional recorder capturing input as it is delivered.
    input_recorder: Option<InputRecorder>,
    /// Optional recording being replayed, with the index of the next event to apply.
    replay: Option<(InputRecording, usize)>,
    /// Pending guest register writes drained from the memory log.
    ///
    /// Retained for the same reason as `copper_writes`. This one is the larger cost: a
    /// booting guest writes custom registers on nearly every scanline, so the previous
    /// `collect()` allocated roughly once per scanline, which a one-minute measurement
    /// showed as 978,521 allocations.
    guest_reg_writes: Vec<(u16, u16)>,
    /// Pending copper register writes for the scanline being run.
    ///
    /// Retained across scanlines and cleared before each use. A fresh `Vec` per scanline
    /// allocated whenever the copper produced at least one write, which the allocation
    /// measurement showed happening once per frame on a small copper list and more often
    /// on a real one.
    copper_writes: Vec<(u16, u16)>,
    /// Optional injected sink for instruction tracing.
    trace_sink: Option<Box<dyn TraceSink + Send>>,
    /// Optional trace limit (number of instructions).
    trace_limit: Option<u64>,
    /// Count of traced instructions.
    trace_count: u64,
    /// Whether a complete frame has been rendered.
    pub frame_ready: bool,
    /// Total CPU cycles executed since start.
    pub total_cycles: u64,
    /// First scanline in the current frame that rendered active bitplanes.
    pub first_video_scanline: Option<VideoScanlineSnapshot>,
    /// First active bitplane scanlines in the current frame.
    pub early_video_scanlines: Vec<VideoScanlineSnapshot>,
    /// Most recent scanline that rendered active bitplanes.
    pub last_video_scanline: Option<VideoScanlineSnapshot>,
    /// Pending keyboard events (keycode, pressed).
    key_events: BoundedQueue<(u8, bool)>,
    /// Mouse delta X accumulator.
    mouse_dx: i16,
    /// Mouse delta Y accumulator.
    mouse_dy: i16,
    /// Cached `GfxBase` address (discovered once from library list).
    gfxbase_cache: u32,
    /// Mouse button state (left pressed).
    mouse_left: bool,
    /// Mouse button state (right pressed).
    mouse_right: bool,
    /// Mouse X hardware quadrature counter.
    mouse_x_counter: u8,
    /// Mouse Y hardware quadrature counter.
    mouse_y_counter: u8,
    /// Keyboard transmission delay counter (in frames).
    keyboard_delay: u8,
}

impl Emulator {
    /// Update the dynamic disk status byte read by the CPU from CIA-A PRA.
    pub fn update_disk_status(&mut self) {
        let mut st: u8 = 0x3C; // default: all status bits high (active-low deasserted)
        if self.floppy.any_drive_selected() {
            let dr = self.floppy.first_selected_drive();
            let d = &self.floppy.drives[dr];

            // DSKTRACK0 (bit 4, active low)
            if d.cyl == 0 {
                st &= !0x10;
            }

            // DSKCHANGE (bit 2, active low)
            // Active (0) if there is no disk OR if the disk changed latch is true
            if d.data.is_none() || d.disk_changed {
                st &= !0x04;
            }

            // DSKRDY (bit 5, active low)
            if d.motor {
                if d.dskready {
                    st &= !0x20; // Ready (motor on + finished spin-up)
                }
            } else if self.floppy.drive_id_bit() != 0 {
                st &= !0x20; // Active-low drive ID bit
            }
        }
        self.memory.disk_status = st;
    }
    /// Create a new emulator with the given memory configuration.
    ///
    /// Schedules the initial `HSync` event.
    #[must_use]
    pub fn new(config: MemoryConfig) -> Self {
        let mut events = EventScheduler::new();
        events.schedule(EventType::HSync, 227);

        let cpu_type = config.cpu_type;
        let video_standard = config.video_standard;
        let mut memory = AmigaMemory::new(config);
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(cpu_type);
        cpu.reset(&mut memory);

        Self {
            cpu,
            memory,
            chipset: CustomChipState::new(video_standard),
            events,
            copper: CopperState::new(),
            playfield: PlayfieldState::new(),
            blitter: BlitterState::new(),
            floppy: FloppyController::new(),
            audio: AudioState::new(),
            sprites: SpriteEngine::new(),
            framebuffer: vec![0; FRAMEBUFFER_SIZE],
            video_standard,
            frames_run: 0,
            input_recorder: None,
            replay: None,
            guest_reg_writes: Vec::new(),
            copper_writes: Vec::new(),
            trace_sink: None,
            trace_limit: None,
            trace_count: 0,
            frame_ready: false,
            total_cycles: 0,
            first_video_scanline: None,
            early_video_scanlines: Vec::new(),
            last_video_scanline: None,
            key_events: BoundedQueue::new(MAX_KEY_EVENTS, KEY_EVENT_OVERFLOW),
            mouse_dx: 0,
            mouse_dy: 0,
            gfxbase_cache: 0,
            mouse_left: false,
            mouse_right: false,
            mouse_x_counter: 0,
            mouse_y_counter: 0,
            keyboard_delay: 0,
        }
    }

    /// Load Kickstart ROM data into memory.
    ///
    /// Re-resets the CPU so it picks up the correct reset vectors from the new ROM.
    pub fn load_rom(&mut self, data: &[u8]) {
        self.memory.load_rom(data);
        self.cpu.reset(&mut self.memory);
    }

    /// Enable an A2065-compatible Zorro II Ethernet card.
    pub fn enable_a2065(&mut self, mac_address: MacAddress) {
        self.memory.enable_a2065(mac_address);
    }

    /// Disable the emulated A2065-compatible Zorro II Ethernet card.
    pub fn disable_a2065(&mut self) {
        self.memory.disable_a2065();
    }

    /// Insert an ADF disk image into the specified floppy drive (0–3).
    pub fn insert_floppy(&mut self, drive: usize, data: Vec<u8>) {
        self.floppy.insert_disk(drive, data);
    }

    /// Returns whether the specified floppy drive (0–3) has dirty data.
    #[must_use]
    pub fn floppy_dirty(&self, drive: usize) -> bool {
        self.floppy.drives.get(drive).is_some_and(|d| d.dirty)
    }

    /// Clears the dirty flag on the specified floppy drive (0–3).
    pub fn clear_floppy_dirty(&mut self, drive: usize) {
        if let Some(d) = self.floppy.drives.get_mut(drive) {
            d.dirty = false;
        }
    }

    /// Extract the ADF disk image data from the specified floppy drive (0–3).
    #[must_use]
    pub fn extract_floppy(&self, drive: usize) -> Option<Vec<u8>> {
        self.floppy.drives.get(drive).and_then(|d| d.data.clone())
    }

    /// Insert a Virtual hardfile (.hdf) disk image into the Gayle IDE controller.
    pub fn insert_hdf(&mut self, data: Vec<u8>) {
        self.memory.ide.borrow_mut().insert_disk(data);
    }

    /// Check if the in-memory hardfile buffer is dirty.
    #[must_use]
    pub fn hdf_dirty(&self) -> bool {
        self.memory.ide.borrow().hdf_dirty
    }

    /// Reset the dirty flag of the hardfile buffer.
    pub fn clear_hdf_dirty(&mut self) {
        self.memory.ide.borrow_mut().hdf_dirty = false;
    }

    /// Extract the current hardfile byte vector from the IDE controller.
    #[must_use]
    pub fn extract_hdf(&self) -> Option<Vec<u8>> {
        self.memory.ide.borrow().disk_data.clone()
    }

    /// Set the floppy speed percentage. `0` selects turbo mode.
    pub fn set_floppy_speed_percent(&mut self, percent: u16) -> bool {
        self.floppy.set_speed_percent(percent)
    }

    /// Queue a keyboard event for CIA handling.
    ///
    /// Returns what the overflow policy did with the event. A full queue refuses the
    /// event and counts it; a caller that ignores the result gets the previous
    /// behaviour, but the loss is now recorded rather than invisible.
    pub fn key_event(&mut self, keycode: u8, pressed: bool) -> QueueAdmission {
        if let Some(recorder) = self.input_recorder.as_mut() {
            recorder.key(self.frames_run, keycode, pressed);
        }
        self.apply_key_event(keycode, pressed)
    }

    /// Apply a key event without recording it.
    ///
    /// Replay goes through here rather than reimplementing the effect. A second
    /// implementation drifts: the first version of this replay path updated the
    /// mouse deltas but not the mouse counters, and the determinism test caught it.
    fn apply_key_event(&mut self, keycode: u8, pressed: bool) -> QueueAdmission {
        self.key_events.push((keycode, pressed))
    }

    /// Key events currently queued for the guest.
    #[must_use]
    pub fn key_queue_depth(&self) -> usize {
        self.key_events.len()
    }

    /// Deepest the keyboard queue has ever been.
    ///
    /// A value equal to [`Emulator::key_queue_capacity`] means the queue reached its
    /// limit at least once. Without this a queue that saturated and drained would look
    /// exactly like one that was never busy.
    #[must_use]
    pub const fn key_queue_high_water(&self) -> usize {
        self.key_events.high_water()
    }

    /// Capacity of the keyboard queue.
    #[must_use]
    pub const fn key_queue_capacity(&self) -> usize {
        MAX_KEY_EVENTS
    }

    /// Key events lost to the overflow policy.
    #[must_use]
    pub const fn key_events_dropped(&self) -> u64 {
        self.key_events.dropped()
    }

    /// Overflow policy the keyboard queue applies when full.
    #[must_use]
    pub const fn key_queue_policy(&self) -> OverflowPolicy {
        self.key_events.policy()
    }

    /// Retained capacity of the guest register write buffer.
    ///
    /// This is the buffer whose absence cost roughly one allocation per scanline during a
    /// real boot. A test that measures allocations without reaching this path passes while
    /// the product allocates.
    #[must_use]
    pub fn guest_reg_writes_capacity(&self) -> usize {
        self.guest_reg_writes.capacity()
    }

    /// Retained capacity of the copper pending-write buffer.
    ///
    /// Exposed for allocation measurement. A capacity that stops growing is what makes the
    /// scanline loop allocation-free, and unlike a global allocator hook this works in the
    /// `no_std` profile and on a device. A total allocation count says something allocated;
    /// this says which buffer would have.
    #[must_use]
    pub fn copper_writes_capacity(&self) -> usize {
        self.copper_writes.capacity()
    }

    /// Retained capacity of the early-scanline diagnostic buffer.
    #[must_use]
    pub fn early_video_scanlines_capacity(&self) -> usize {
        self.early_video_scanlines.capacity()
    }

    /// Accumulate mouse movement deltas.
    pub fn mouse_move(&mut self, dx: i16, dy: i16) {
        if let Some(recorder) = self.input_recorder.as_mut() {
            recorder.mouse_move(self.frames_run, dx, dy);
        }
        self.apply_mouse_move(dx, dy);
    }

    /// Apply pointer motion without recording it.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn apply_mouse_move(&mut self, dx: i16, dy: i16) {
        self.mouse_dx = self.mouse_dx.saturating_add(dx);
        self.mouse_dy = self.mouse_dy.saturating_add(dy);
        self.mouse_x_counter = self.mouse_x_counter.wrapping_add(dx as u8);
        self.mouse_y_counter = self.mouse_y_counter.wrapping_add(dy as u8);
    }

    /// Set mouse button state.
    pub fn mouse_button(&mut self, left: bool, right: bool) {
        if let Some(recorder) = self.input_recorder.as_mut() {
            recorder.mouse_buttons(self.frames_run, left, right);
        }
        self.apply_mouse_buttons(left, right);
    }

    /// Apply pointer button state without recording it.
    fn apply_mouse_buttons(&mut self, left: bool, right: bool) {
        self.mouse_left = left;
        self.mouse_right = right;
        self.memory.mouse_left = left;
    }

    /// Run one full frame of the selected video standard.
    ///
    /// 312 scanlines under PAL, 262 under NTSC.
    pub fn run_frame(&mut self) {
        // Replayed input is applied here rather than by the caller, so the ordering
        // between input and emulation is a property of the machine. A shell that
        // applied it itself could get the order wrong and would produce a different
        // digest for the same recording.
        self.apply_replayed_input();
        self.frame_ready = false;
        self.first_video_scanline = None;
        self.early_video_scanlines.clear();
        for _ in 0..self.video_standard.scanlines() {
            self.run_scanline();
        }
        self.frames_run = self.frames_run.saturating_add(1);
    }

    /// Deliver every recorded event scheduled for the current frame.
    ///
    /// Events are consumed in order from a cursor, so a recording is walked once
    /// rather than searched per frame.
    fn apply_replayed_input(&mut self) {
        let frame = self.frames_run;
        let Some((recording, cursor)) = self.replay.take() else {
            return;
        };
        let mut cursor = cursor;
        while let Some(recorded) = recording.events().get(cursor) {
            if recorded.frame != frame {
                break;
            }
            cursor += 1;
            match recorded.event {
                InputEvent::Key { keycode, pressed } => {
                    // A replayed event that the queue refuses is itself deterministic,
                    // and the queue's counters record the loss.
                    let _ = self.apply_key_event(keycode, pressed);
                }
                InputEvent::MouseMove { dx, dy } => self.apply_mouse_move(dx, dy),
                InputEvent::MouseButtons { left, right } => {
                    self.apply_mouse_buttons(left, right);
                }
            }
        }
        self.replay = Some((recording, cursor));
    }

    /// Frames completed since construction.
    ///
    /// This is the clock a recording is stamped against.
    #[must_use]
    pub const fn frames_run(&self) -> u64 {
        self.frames_run
    }

    /// Start recording input as it is delivered.
    ///
    /// Recording happens inside the three input entry points, so a recording is
    /// complete by construction: there is no other way input reaches the machine.
    pub fn start_input_recording(&mut self) {
        self.input_recorder = Some(InputRecorder::new());
    }

    /// Stop recording and return what was captured.
    ///
    /// Returns `None` if no recording was running.
    ///
    /// # Errors
    ///
    /// Returns [`ReplayError::FramesOutOfOrder`] if frames went backwards, which
    /// would mean the caller drove the machine backwards.
    pub fn finish_input_recording(&mut self) -> Option<Result<InputRecording, ReplayError>> {
        self.input_recorder.take().map(InputRecorder::finish)
    }

    /// Whether input is being recorded.
    #[must_use]
    pub const fn is_recording_input(&self) -> bool {
        self.input_recorder.is_some()
    }

    /// Replay `recording` from the current frame onwards.
    ///
    /// Events whose frame index is already behind the machine are never applied, so
    /// attach a recording before running any frames.
    pub fn attach_input_replay(&mut self, recording: InputRecording) {
        self.replay = Some((recording, 0));
    }

    /// Recording currently being replayed, if any.
    #[must_use]
    pub fn replayed_recording(&self) -> Option<&InputRecording> {
        self.replay.as_ref().map(|(recording, _)| recording)
    }

    /// Whether every event of an attached recording has been applied.
    #[must_use]
    pub fn replay_exhausted(&self) -> bool {
        self.replay
            .as_ref()
            .is_none_or(|(recording, cursor)| *cursor >= recording.len())
    }

    /// Attach a trace sink and enable CPU execution tracing.
    ///
    /// `limit` bounds the number of recorded instructions; `None` records
    /// without a bound. Attaching a sink resets the recorded count.
    ///
    /// The core neither opens files nor accepts host paths. Transport
    /// ownership, including any buffered writer, belongs to the adapter that
    /// supplies the sink.
    pub fn set_trace_sink(&mut self, sink: Box<dyn TraceSink + Send>, limit: Option<u64>) {
        self.trace_sink = Some(sink);
        self.trace_limit = limit;
        self.trace_count = 0;
    }

    /// Flush the attached trace sink, if any.
    pub fn flush_trace(&mut self) {
        if let Some(sink) = self.trace_sink.as_mut() {
            sink.flush();
        }
    }

    /// Flush and detach the trace sink, disabling tracing.
    pub fn clear_trace_sink(&mut self) {
        self.flush_trace();
        self.trace_sink = None;
        self.trace_limit = None;
    }

    /// Number of instructions recorded through the trace sink.
    #[must_use]
    pub const fn trace_count(&self) -> u64 {
        self.trace_count
    }

    /// Format and write one trace record to the attached sink.
    ///
    /// Does nothing when no sink is attached or the trace limit is reached.
    pub fn write_trace_line(&mut self) {
        if let Some(sink) = self.trace_sink.as_mut() {
            if let Some(limit) = self.trace_limit {
                if self.trace_count >= limit {
                    return;
                }
            }

            let pc = self.cpu.pc;
            // Safely read the opcode word at PC
            let opcode = AddressBus::read_word(&mut self.memory, pc);
            let (disasm, _) = m68k::dasm::disassemble(pc, opcode, self.cpu.cpu_type);
            let dar = self.cpu.dar;
            let sr = self.cpu.get_sr();

            sink.write_record(format_args!(
                "PC: {:08X} | OP: {:04X} ({:<20}) | D0: {:08X} D1: {:08X} D2: {:08X} D3: {:08X} | A0: {:08X} A1: {:08X} A2: {:08X} A7: {:08X} | SR: {:04X}",
                pc,
                opcode,
                disasm,
                dar[0],
                dar[1],
                dar[2],
                dar[3],
                dar[8],
                dar[9],
                dar[10],
                dar[15],
                sr
            ));

            self.trace_count += 1;
        }
    }

    /// Execute a single CPU instruction (for debugging/tracing).
    pub fn step_instruction(&mut self) {
        self.write_trace_line();
        let mut handler = NoOpHleHandler;
        let _ = self
            .cpu
            .step_with_hle_handler(&mut self.memory, &mut handler);
    }

    /// Execute one scanline worth of emulation.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn run_scanline(&mut self) {
        // Update audio filter state based on CIA-A PRA bit 1 (LED state)
        self.audio.filter_active = (self.memory.cia_a_pra & 2) != 0;

        // Sync readable registers into memory so CPU reads correct values
        self.sync_readable_regs();

        // Execute CPU instructions for this scanline
        let mut cycles_used: usize = 0;
        let mut handler = NoOpHleHandler;
        while cycles_used < CYCLES_PER_LINE {
            self.process_gayle_ide_interrupts();

            // Sync interrupt registers so CPU reads see current state
            self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq & 0x7FFF;
            self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena & 0x7FFF;

            // Update interrupt level for CpuCore
            let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
            if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
                self.cpu.int_level = u32::from(self.chipset.interrupt_level());
            } else {
                self.cpu.int_level = 0;
            }

            self.write_trace_line();
            let step_res = self
                .cpu
                .step_with_hle_handler(&mut self.memory, &mut handler);
            let cycles = match step_res {
                StepResult::Ok { cycles } => cycles,
                _ => 0,
            };
            if cycles <= 0 || self.cpu.is_stopped() {
                cycles_used = CYCLES_PER_LINE;
                break;
            }
            cycles_used += cycles as usize;
            // Update HPOS based on cycles consumed (2 CPU cycles = 1 color clock)
            self.chipset.hpos = u16::try_from((cycles_used / 2).min(226)).unwrap_or(226);
            // Sync beam position so CPU reads of VHPOSR see advancing hpos
            self.memory.custom_regs[(custom::VHPOSR / 2) as usize] =
                (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);
            // Dispatch register writes immediately. The buffer is moved out so the drain
            // of the memory log does not overlap the dispatch calls, then put back with
            // its capacity, which is what keeps this loop allocation-free.
            let mut writes = core::mem::take(&mut self.guest_reg_writes);
            writes.clear();
            writes.extend(self.memory.drain_reg_writes());
            #[allow(
                clippy::iter_with_drain,
                reason = "into_iter would consume the buffer and reintroduce the per-scanline allocation this retains"
            )]
            for (offset, value) in writes.drain(..) {
                self.dispatch_register_write(offset, value);
            }
            self.guest_reg_writes = writes;
            // Handle CIA-B PRB writes (disk drive selection/motor/step)
            if self.memory.cia_b_prb_dirty {
                self.memory.cia_b_prb_dirty = false;
                let prb = self.memory.cia.borrow().cia_b.prb;
                self.floppy.disk_select(prb);
                self.update_disk_status();
            }
        }
        self.total_cycles += cycles_used as u64;

        // Advance chipset beam by one full scanline
        self.chipset.hpos = 0;
        if self.chipset.vpos >= self.video_standard.last_line() {
            self.chipset.vpos = 0;
        } else {
            self.chipset.vpos += 1;
        }
        let vpos = self.chipset.vpos;

        // Run copper for this scanline
        if self.copper.enabled {
            let chip_ram = self.memory.chip_ram();
            // Reuse the retained buffer. `core::mem::take` moves it out so the borrow of
            // chip RAM below does not conflict with holding `&mut self`; the buffer is put
            // back before this block ends, keeping its capacity.
            let mut copper_writes = core::mem::take(&mut self.copper_writes);
            copper_writes.clear();
            for h in 0u16..227 {
                if let Some(action) = self.copper.cycle(chip_ram, vpos, h) {
                    match action {
                        CopperAction::WriteRegister { offset, value } => {
                            // COPJMP1/2 must be handled immediately (affects copper PC)
                            match offset {
                                custom::COPJMP1 => self.copper.strobe_cop1(),
                                custom::COPJMP2 => {
                                    self.copper.strobe_cop2();
                                }
                                _ => copper_writes.push((offset, value)),
                            }
                        }
                    }
                }
            }
            // Drain rather than consume, so the buffer survives to be put back with its
            // capacity intact.
            #[allow(
                clippy::iter_with_drain,
                reason = "into_iter would consume the buffer and reintroduce the per-scanline allocation this retains"
            )]
            for (offset, value) in copper_writes.drain(..) {
                self.memory.custom_regs[(offset / 2) as usize] = value;
                self.dispatch_register_write(offset, value);
            }
            self.copper_writes = copper_writes;
        }

        // Sync playfield state from shadow registers (copper has updated them)
        let regs = &self.memory.custom_regs;
        self.playfield.bplcon0 = regs[(custom::BPLCON0 / 2) as usize];
        self.playfield.bplcon1 = regs[(0x102 / 2) as usize];
        self.playfield.bplcon2 = regs[(0x104 / 2) as usize];
        self.playfield.diwstrt = regs[(0x08E / 2) as usize];
        self.playfield.diwstop = regs[(0x090 / 2) as usize];
        self.playfield.diwhigh = regs[(custom::DIWHIGH / 2) as usize];
        self.playfield.ddfstrt = regs[(0x092 / 2) as usize];
        self.playfield.ddfstop = regs[(0x094 / 2) as usize];
        for i in 0usize..32 {
            let c = regs[0x180 / 2 + i];
            self.playfield.color[i] = c & 0x0FFF;
        }

        // Render this scanline AFTER copper sets up registers for this line.
        let (_, _, vstart, vstop) = self.playfield.display_window();
        let active_height = self.video_standard.active_height();
        let framebuffer_line = vpos
            .checked_sub(vstart)
            .filter(|line| *line < active_height);
        if let Some(framebuffer_line) = framebuffer_line {
            let bitplane_dma = self.chipset.dmaen(custom::DMA_BITPLANE);
            let saved_bplcon0 = self.playfield.bplcon0;
            let num_planes = self.playfield.num_planes();
            let chip_ram = self.memory.chip_ram();
            if bitplane_dma && num_planes > 0 {
                let (hstart, hstop, vstart, vstop) = self.playfield.display_window();
                let bplpt = self.playfield.bplpt;
                let snapshot = VideoScanlineSnapshot {
                    vpos,
                    framebuffer_line,
                    hstart,
                    hstop,
                    vstart,
                    vstop,
                    bplcon0: self.playfield.bplcon0,
                    bplcon1: self.playfield.bplcon1,
                    ddfstrt: self.playfield.ddfstrt,
                    ddfstop: self.playfield.ddfstop,
                    bpl1mod: self.memory.custom_regs[(custom::BPL1MOD / 2) as usize],
                    bpl2mod: self.memory.custom_regs[(custom::BPL2MOD / 2) as usize],
                    bplpt,
                    bitplane_words: snapshot_bitplane_words(chip_ram, bplpt, num_planes),
                    num_planes,
                };
                if self.early_video_scanlines.len() < EARLY_VIDEO_SCANLINE_DUMP {
                    self.early_video_scanlines.push(snapshot);
                }
                self.first_video_scanline.get_or_insert(snapshot);
                self.last_video_scanline = Some(snapshot);
            }
            if !bitplane_dma {
                self.playfield.bplcon0 = 0;
            }

            let mut line_buffer = [0u16; DISPLAY_WIDTH];
            self.playfield
                .render_scanline(vpos, chip_ram, &mut line_buffer);
            self.playfield.bplcon0 = saved_bplcon0;

            // Sprite DMA and rendering
            let sprite_dma = self.chipset.dmaen(custom::DMA_SPRITE);
            for i in 0..8 {
                if !sprite_dma {
                    continue;
                }
                if self.sprites.sprites[i].active {
                    // Active: fetch image data, then render
                    self.sprites.fetch_data(i, chip_ram, self.playfield.fmode);
                    self.sprites.render_into_line(
                        &mut line_buffer,
                        &self.playfield.color_aga,
                        &self.playfield.color,
                        i,
                        playfield::DISPLAY_LEFT_HPOS,
                        2,
                        self.playfield.bplcon4,
                        self.playfield.fmode,
                    );
                    // Deactivate at vstop
                    if vpos + 1 == SpriteEngine::vstop(&self.sprites.sprites[i]) {
                        self.sprites.sprites[i].active = false;
                        self.sprites.sprites[i].armed = false;
                    }
                } else if !self.sprites.sprites[i].armed {
                    // Not yet armed: fetch pos/ctl to learn vstart/vstop
                    self.sprites.fetch_data(i, chip_ram, self.playfield.fmode);
                    self.sprites.sprites[i].armed = true;
                } else if vpos == SpriteEngine::vstart(&self.sprites.sprites[i]) {
                    // Armed and vstart matches: activate
                    self.sprites.sprites[i].active = true;
                    // Fetch first line of data immediately
                    self.sprites.fetch_data(i, chip_ram, self.playfield.fmode);
                    self.sprites.render_into_line(
                        &mut line_buffer,
                        &self.playfield.color_aga,
                        &self.playfield.color,
                        i,
                        playfield::DISPLAY_LEFT_HPOS,
                        2,
                        self.playfield.bplcon4,
                        self.playfield.fmode,
                    );
                }
            }
            // Add modulo to bitplane pointers at end of line
            #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
            if bitplane_dma
                && vpos >= vstart
                && vpos < vstop
                && self.playfield.num_planes().min(6) > 0
            {
                let bpl1mod = self.memory.custom_regs[(0x108 / 2) as usize] as i16;
                let bpl2mod = self.memory.custom_regs[(0x10A / 2) as usize] as i16;
                let num_planes = self.playfield.num_planes().min(6);
                for i in 0..num_planes {
                    let m = if i % 2 == 0 { bpl1mod } else { bpl2mod };
                    if m >= 0 {
                        self.playfield.bplpt[i] = self.playfield.bplpt[i].wrapping_add(m as u32);
                    } else {
                        self.playfield.bplpt[i] =
                            self.playfield.bplpt[i].wrapping_sub(m.unsigned_abs().into());
                    }
                }
            }
            let offset = usize::from(framebuffer_line) * DISPLAY_WIDTH;
            self.framebuffer[offset..offset + DISPLAY_WIDTH].copy_from_slice(&line_buffer);
        }

        // CIA E-clock: ~45 ticks per scanline (709379 Hz / 15625 Hz)
        for _ in 0..45 {
            let mut cia = self.memory.cia.borrow_mut();
            if cia.cia_a.tick() {
                self.chipset.intreq |= custom::INT_PORTS;
            }
            if cia.cia_b.tick() {
                self.chipset.intreq |= custom::INT_EXTER;
            }
        }
        // Also fire INT_PORTS / INT_EXTER if CIA-A/B has any masked interrupt pending
        {
            let cia = self.memory.cia.borrow();
            if cia.cia_a.icr_ir {
                self.chipset.intreq |= custom::INT_PORTS;
            }
            if cia.cia_b.icr_ir {
                self.chipset.intreq |= custom::INT_EXTER;
            }
        }
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;

        // CIA-B TOD clocked by HSync (every scanline)
        self.memory.cia.borrow_mut().cia_b.tick_tod();

        // Tick floppy drive spin-up delays and update status
        self.floppy.tick_scanline();
        self.update_disk_status();

        // Disk index pulse: only fires when a disk is present and spinning.
        // Without a disk, no index hole exists so no pulse is generated.
        // This is critical: without index pulses, trackdisk.device times out
        // and the boot code shows the "insert disk" hand.
        // Disk index pulse: fires once per revolution when motor is spinning.
        if self.floppy.any_drive_selected()
            && self.floppy.motor_on()
            && self.floppy.has_disk()
            && self.chipset.vpos == 0
        {
            // Fire index pulse once per revolution (~300ms real, once per frame here)
            let mut cia = self.memory.cia.borrow_mut();
            if cia.cia_b.set_flag() {
                self.chipset.intreq |= custom::INT_EXTER;
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }
        }

        // Process pending key events into CIA-A serial data register
        if self.keyboard_delay == 0 && !self.memory.cia.borrow().cia_a.icr_ir {
            if let Some((keycode, pressed)) = self.key_events.peek().copied() {
                // Amiga keyboard protocol: bit 7 = 0 for press, 1 for release
                let code = if pressed { keycode } else { keycode | 0x80 };
                let sdr_val = translate_amiga_keycode(code);
                {
                    let mut cia_ref = self.memory.cia.borrow_mut();
                    let cia_a = &mut cia_ref.cia_a;
                    cia_a.sdr = sdr_val;
                    cia_a.icr_data |= 0x08; // Set Serial Port interrupt bit (ICR_SP)
                    if (cia_a.icr_mask & 0x08) != 0 {
                        cia_a.icr_ir = true;
                    }
                }
                self.key_events.pop();
                self.keyboard_delay = 3; // Enforce a 3-frame delay between key events
            }
        }

        // Floppy DMA: advance the selected drive according to the configured speed.
        if self.chipset.dmaen(crate::custom::DMA_DISK) {
            let cycles = self.floppy.dma_word_cycles_for_scanline();
            self.run_floppy_dma_cycles(cycles);
        }

        // VBlank handling
        if vpos == 0 {
            if self.keyboard_delay > 0 {
                self.keyboard_delay -= 1;
            }
            {
                self.chipset.intreq |= custom::INT_VERTB;
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
            }

            // Pre-allocate signal bit 31 in the boot task's tc_SigAlloc.
            // On real hardware, input.device or timer.device allocates this bit
            // before Intuition runs. Our device initialization timing differs,
            // leaving bit 31 free. Intuition then allocates it and later
            // FreeEntry incorrectly frees a stack-allocated buffer, corrupting
            // the memory free list.
            if self.total_cycles > 5_000_000 && self.total_cycles < 5_100_000 {
                let chip = self.memory.chip_ram_mut();
                if chip.len() > 8 {
                    let eb = u32::from_be_bytes([chip[4], chip[5], chip[6], chip[7]]) as usize;
                    if eb > 0 && eb + 0x118 < chip.len() {
                        let tt = u32::from_be_bytes([
                            chip[eb + 0x114],
                            chip[eb + 0x115],
                            chip[eb + 0x116],
                            chip[eb + 0x117],
                        ]) as usize;
                        if tt > 0 && tt + 0x16 < chip.len() {
                            // Set bit 31 in tc_SigAlloc (offset $12 from task)
                            chip[tt + 0x12] |= 0x80;
                        }
                    }
                }
            }

            // On real hardware, the graphics.library VBLANK server writes
            // GfxBase->copinit to COP1LC every frame. Our interrupt delivery
            // timing doesn't allow the handler to run before restart, so we
            // read copinit directly. GfxBase is cached after first discovery.
            if let Some(copinit) = self.gfx_copinit() {
                if copinit != 0
                    && copinit < u32::try_from(self.memory.chip_ram().len()).unwrap_or(u32::MAX)
                {
                    self.copper.cop1lc = copinit;
                }
            }
            // Sync colors from ViewPort ColorMap into the copper list.
            // On real hardware, LoadRGB4 updates both ColorMap and copper list
            // via DspIns. Our MrgCop doesn't set DspIns, so we patch the copper
            // list directly from the ColorMap at VBLANK.
            self.sync_colormap_to_copper();
            self.copper.restart_vertical_blank();
            self.frame_ready = true;
            // Reset sprites for new frame — they re-fetch pos/ctl from DMA
            for sprite in &mut self.sprites.sprites {
                sprite.active = false;
                sprite.armed = false;
            }
            // CIA-A TOD clocked by VSync (once per frame)
            self.memory.cia.borrow_mut().cia_a.tick_tod();
            // Reset mouse deltas at frame boundary
            self.mouse_dx = 0;
            self.mouse_dy = 0;

            // Set unit+$126 = 1 (disk changed) once trackdisk's unit exists.
            // On real hardware, CIA-B FLAG fires on DSKCHANGE when no disk is
            // present, and trackdisk's EXTER handler sets this flag. We set it
            // directly because the FLAG timing during early boot is complex.
            {
                let off = 0x4856usize; // unit ($C04730) + $126 = $C04856
                if self.memory.slow_ram.len() > off
                    && self.memory.slow_ram[off] == 0
                    && self.memory.slow_ram[0x4730] != 0
                // unit exists
                {
                    self.memory.slow_ram[off] = 1;
                }
            }

            // CIA-B FLAG mask enabled for DSKCHANGE detection.
        }

        // Sync INTREQR/INTENAR so the CPU reads correct values in interrupt handlers
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;

        // Update interrupt level for next scanline
        let pending = self.chipset.intreq & self.chipset.intena & 0x3FFF;
        if pending != 0 && (self.chipset.intena & custom::INT_SETCLR) != 0 {
            self.cpu.int_level = u32::from(self.chipset.interrupt_level());
        } else {
            self.cpu.int_level = 0;
        }
    }

    fn run_floppy_dma_cycles(&mut self, cycles: usize) {
        for _ in 0..cycles {
            {
                let chip_ram = self.memory.chip_ram_mut();
                self.floppy.disk_dma_cycle(chip_ram);
            }
            self.memory.dskbytr.set(self.floppy.dskbytr_val);
        }

        self.deliver_floppy_interrupts();
    }

    fn deliver_floppy_interrupts(&mut self) {
        if self.floppy.pending_sync_irq {
            self.floppy.pending_sync_irq = false;
            self.chipset.intreq |= 0x1000; // DSKSYNC
            self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        }
        if self.floppy.pending_blk_irq {
            self.floppy.pending_blk_irq = false;
            self.chipset.intreq |= custom::INT_DSKBLK;
            self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
        }
    }

    fn process_gayle_ide_interrupts(&mut self) {
        let ide = self.memory.ide.borrow();
        if ide.pending_irq && (ide.devcon & 0x02) == 0 {
            self.memory.gayle_irq |= 0x80;
        }

        if (self.memory.gayle_irq & self.memory.gayle_intena & 0x80) != 0 {
            self.chipset.intreq |= crate::custom::INT_PORTS;
        }
    }

    /// Sync live chipset state into the custom register shadow so CPU reads are correct.
    fn sync_readable_regs(&mut self) {
        let regs = &mut self.memory.custom_regs;
        regs[(custom::VPOSR / 2) as usize] = self.chipset.vposr();
        regs[(custom::VHPOSR / 2) as usize] = (self.chipset.vpos << 8) | (self.chipset.hpos & 0xFF);

        // BBUSY stays clear: a blit completes within the write that starts it,
        // so no emulated time passes while the blitter is busy.
        regs[(custom::DMACONR / 2) as usize] = self.chipset.dmacon & 0x7FFF;
        regs[(custom::INTENAR / 2) as usize] = self.chipset.intena & 0x7FFF;
        regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq & 0x7FFF;
        regs[(custom::BEAMCON0 / 2) as usize] = self.video_standard.beamcon0();
        // SERDATR ($018): TBE (bit 13) + TSRE (bit 12) = transmit buffer empty
        regs[(0x018 / 2) as usize] = 0x3000;
        // POTGOR ($016): active-high button state (bits 8-15 = all buttons released)
        let mut potgor: u16 = 0xFF00;
        if self.mouse_right {
            potgor &= !(1 << 10); // Clear bit 10 (Port 1 Pin 9 Right Mouse Button pressed, active-low)
        }
        regs[(0x016 / 2) as usize] = potgor;
        // JOY0DAT ($00A): Port 1 mouse quadrature counters
        regs[(0x00A / 2) as usize] =
            (u16::from(self.mouse_y_counter) << 8) | u16::from(self.mouse_x_counter);
        // JOY1DAT ($00C): no joystick movement
        regs[(0x00C / 2) as usize] = 0x0000;
        // DENISEID ($07C): OCS Denise returns $FFFF (register doesn't exist)
        regs[(0x07C / 2) as usize] = 0xFFFF;
    }

    /// Dispatch a single custom chip register write to the appropriate subsystem.
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    pub fn dispatch_register_write(&mut self, offset: u16, value: u16) {
        match offset {
            custom::BPLCON0 => self.playfield.bplcon0 = value,
            custom::BPLCON1 => self.playfield.bplcon1 = value,
            custom::BPLCON2 => self.playfield.bplcon2 = value,
            custom::BPLCON3 => self.playfield.bplcon3 = value,
            custom::BPLCON4 => self.playfield.bplcon4 = value,
            custom::FMODE => self.playfield.fmode = value,
            custom::DIWSTRT => self.playfield.diwstrt = value,
            custom::DIWSTOP => self.playfield.diwstop = value,
            custom::DIWHIGH => self.playfield.diwhigh = value,
            custom::DDFSTRT => self.playfield.ddfstrt = value,
            custom::DDFSTOP => self.playfield.ddfstop = value,
            custom::DMACON => {
                self.chipset.write_register(offset, value);
                self.copper.enabled = self.chipset.dmaen(custom::DMA_COPPER);
            }
            custom::INTENA | custom::INTREQ => {
                self.chipset.write_register(offset, value);
                self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
                self.memory.custom_regs[(custom::INTENAR / 2) as usize] = self.chipset.intena;
            }
            custom::COP1LCH => {
                self.copper.cop1lc = (self.copper.cop1lc & 0x0000_FFFF) | (u32::from(value) << 16);
                self.copper.cop1lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP1LCL => {
                self.copper.cop1lc = (self.copper.cop1lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop1lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCH => {
                self.copper.cop2lc = (self.copper.cop2lc & 0x0000_FFFF) | (u32::from(value) << 16);
                self.copper.cop2lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COP2LCL => {
                self.copper.cop2lc = (self.copper.cop2lc & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.copper.cop2lc &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::COPJMP1 => self.copper.strobe_cop1(),
            custom::COPJMP2 => self.copper.strobe_cop2(),
            custom::DSKLEN => {
                let adkcon = self.memory.read_custom_reg(custom::ADKCONR);
                self.floppy.write_dsklen(value, adkcon);
            }
            custom::DSKSYNC => self.floppy.write_dsksync(value),
            custom::DSKPTH => {
                self.floppy.dskpt = (self.floppy.dskpt & 0x0000_FFFF) | (u32::from(value) << 16);
            }
            custom::DSKPTL => {
                self.floppy.dskpt = (self.floppy.dskpt & 0xFFFF_0000) | u32::from(value & 0xFFFE);
            }
            o if (custom::COLOR00..=custom::COLOR31).contains(&o) => {
                self.chipset.write_register(o, value);
                let idx = ((o - custom::COLOR00) / 2) as usize;
                self.playfield.color[idx] = value & 0x0FFF;

                let bank = ((self.playfield.bplcon3 >> 13) & 7) as usize;
                let colreg = bank * 32 + idx;

                let r4 = ((value >> 8) & 0x0F) as u32;
                let g4 = ((value >> 4) & 0x0F) as u32;
                let b4 = (value & 0x0F) as u32;

                let loct = (self.playfield.bplcon3 & 0x0200) != 0;
                if loct {
                    let old_color = self.playfield.color_aga[colreg];
                    let old_r = (old_color >> 16) & 0xFF;
                    let old_g = (old_color >> 8) & 0xFF;
                    let old_b = old_color & 0xFF;

                    let new_r = (old_r & 0xF0) | r4;
                    let new_g = (old_g & 0xF0) | g4;
                    let new_b = (old_b & 0xF0) | b4;

                    self.playfield.color_aga[colreg] = (new_r << 16) | (new_g << 8) | new_b;
                } else {
                    let new_r = (r4 << 4) | r4;
                    let new_g = (g4 << 4) | g4;
                    let new_b = (b4 << 4) | b4;

                    self.playfield.color_aga[colreg] = (new_r << 16) | (new_g << 8) | new_b;
                }
            }
            o if (custom::BPL1PTH..=custom::BPL6PTL).contains(&o) => {
                let reg_idx = ((o - custom::BPL1PTH) / 2) as usize;
                let plane = reg_idx / 2;
                if plane < self.playfield.bplpt.len() {
                    if reg_idx & 1 == 0 {
                        self.playfield.bplpt[plane] =
                            (self.playfield.bplpt[plane] & 0x0000_FFFF) | (u32::from(value) << 16);
                    } else {
                        self.playfield.bplpt[plane] =
                            (self.playfield.bplpt[plane] & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                    }
                }
            }
            o if (custom::SPR0PTH..=custom::SPR7PTL).contains(&o) => {
                let reg_idx = ((o - custom::SPR0PTH) / 2) as usize;
                let sprite = reg_idx / 2;
                if sprite < 8 {
                    if reg_idx & 1 == 0 {
                        self.sprites.sprites[sprite].pt = (self.sprites.sprites[sprite].pt
                            & 0x0000_FFFF)
                            | (u32::from(value) << 16);
                    } else {
                        self.sprites.sprites[sprite].pt = (self.sprites.sprites[sprite].pt
                            & 0xFFFF_0000)
                            | u32::from(value & 0xFFFE);
                    }
                    // Writing PTL re-arms the sprite for pos/ctl fetch
                    if reg_idx & 1 == 1 {
                        self.sprites.sprites[sprite].armed = false;
                        self.sprites.sprites[sprite].active = false;
                    }
                }
                // Copper list sets up sprite pointers — ensure sprite DMA is active
                self.chipset.dmacon |= custom::DMA_SPRITE;
            }
            o if (custom::SPR0POS..=custom::SPR7DATB).contains(&o) => {
                let reg_idx = ((o - custom::SPR0POS) / 2) as usize;
                let sprite = reg_idx / 4;
                if sprite < 8 {
                    match reg_idx % 4 {
                        0 => self.sprites.sprites[sprite].pos = value,
                        1 => self.sprites.sprites[sprite].ctl = value,
                        2 => self.sprites.sprites[sprite].data_a[0] = value,
                        _ => self.sprites.sprites[sprite].data_b[0] = value,
                    }
                }
            }
            o if (custom::BLTCON0..=custom::BLTADAT).contains(&o) => {
                self.dispatch_blitter_write(o, value);
            }
            _ => {}
        }
    }

    /// Dispatch blitter register writes.
    fn dispatch_blitter_write(&mut self, offset: u16, value: u16) {
        match offset {
            custom::BLTCON0 => self.blitter.bltcon0 = value,
            custom::BLTCON0L => {
                self.blitter.bltcon0 = (self.blitter.bltcon0 & 0xFF00) | (value & 0x00FF);
            }
            custom::BLTCON1 => self.blitter.bltcon1 = value,
            custom::BLTAFWM => self.blitter.bltafwm = value,
            custom::BLTALWM => self.blitter.bltalwm = value,
            custom::BLTCPTH => {
                self.blitter.bltcpt =
                    (self.blitter.bltcpt & 0x0000_FFFF) | (u32::from(value) << 16);
                self.blitter.bltcpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTCPTL => {
                self.blitter.bltcpt =
                    (self.blitter.bltcpt & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.blitter.bltcpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTBPTH => {
                self.blitter.bltbpt =
                    (self.blitter.bltbpt & 0x0000_FFFF) | (u32::from(value) << 16);
                self.blitter.bltbpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTBPTL => {
                self.blitter.bltbpt =
                    (self.blitter.bltbpt & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.blitter.bltbpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTAPTH => {
                self.blitter.bltapt =
                    (self.blitter.bltapt & 0x0000_FFFF) | (u32::from(value) << 16);
                self.blitter.bltapt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTAPTL => {
                self.blitter.bltapt =
                    (self.blitter.bltapt & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.blitter.bltapt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTDPTH => {
                self.blitter.bltdpt =
                    (self.blitter.bltdpt & 0x0000_FFFF) | (u32::from(value) << 16);
                self.blitter.bltdpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTDPTL => {
                self.blitter.bltdpt =
                    (self.blitter.bltdpt & 0xFFFF_0000) | u32::from(value & 0xFFFE);
                self.blitter.bltdpt &= (self.memory.chip_ram().len() as u32).wrapping_sub(1);
            }
            custom::BLTCMOD | custom::BLTBMOD | custom::BLTAMOD | custom::BLTDMOD => {
                #[allow(clippy::cast_possible_wrap)]
                let signed = value as i16;
                match offset {
                    custom::BLTCMOD => self.blitter.bltcmod = signed,
                    custom::BLTBMOD => self.blitter.bltbmod = signed,
                    custom::BLTAMOD => self.blitter.bltamod = signed,
                    _ => self.blitter.bltdmod = signed,
                }
            }
            custom::BLTCDAT => self.blitter.bltcdat = value,
            custom::BLTBDAT => self.blitter.load_bdat(value),
            custom::BLTADAT => self.blitter.bltadat = value,
            custom::BLTSIZE => {
                self.blitter.start_legacy_size_blit(value);
                self.start_blitter_execution();
            }
            custom::BLTSIZV => self.blitter.set_vertical_size(value),
            custom::BLTSIZH => {
                self.blitter.start_horizontal_size_blit(value);
                self.start_blitter_execution();
            }
            _ => {}
        }
    }

    /// Execute a started blit to completion and signal it.
    ///
    /// The emulator owns its state for the whole operation, so the result is
    /// visible to the next access and independent of host scheduling. A blit
    /// therefore takes no emulated time, which is why the guest-visible BBUSY bit
    /// never reads set; cycle-accurate blitter timing is separate work.
    fn start_blitter_execution(&mut self) {
        self.blitter.execute_blit(&mut self.memory.chip_ram);
        self.chipset.intreq |= custom::INT_BLIT;
        self.memory.custom_regs[(custom::INTREQR / 2) as usize] = self.chipset.intreq;
    }

    /// Get the current framebuffer contents.
    #[must_use]
    pub fn framebuffer(&self) -> &[u16] {
        &self.framebuffer
    }

    /// Duration of one emulated frame under the selected video standard.
    ///
    /// Derived from the colour clock and the scanline count rather than from a
    /// rounded frame rate, so a PAL frame is 19.968 ms and an NTSC frame 16.615 ms.
    /// This is emulated time, not host time: the core never reads a host clock. A
    /// shell that paces against this value follows the standard automatically.
    #[must_use]
    pub const fn frame_period(&self) -> Duration {
        Duration::from_nanos(self.video_standard.frame_period_nanos())
    }

    /// Video standard the chipset runs at.
    #[must_use]
    pub const fn video_standard(&self) -> VideoStandard {
        self.video_standard
    }

    /// Active picture height in framebuffer lines under the selected standard.
    ///
    /// The framebuffer itself is always PAL-sized. A presenter must use this
    /// height rather than the buffer height, or an NTSC frame gains 45 lines of
    /// never-written padding at the bottom.
    #[must_use]
    pub const fn active_height(&self) -> u16 {
        self.video_standard.active_height()
    }

    /// Digest of the rendered frame.
    ///
    /// Not cryptographic; see [`crate::digest`]. Use it to compare two runs, not
    /// to make an integrity claim.
    #[must_use]
    pub fn frame_digest(&self) -> u64 {
        let mut digest = StateDigest::new();
        digest.write_u16_slice(&self.framebuffer);
        digest.finish()
    }

    /// Digest of the emulated machine state.
    ///
    /// Covers the CPU, elapsed cycles, custom register shadow, interrupt and DMA
    /// state, and chip RAM, in a fixed order. Host-owned state such as trace
    /// sinks and media paths is deliberately excluded, so the digest answers
    /// whether the emulated machine reached the same place.
    ///
    /// Not cryptographic; see [`crate::digest`].
    #[must_use]
    pub fn state_digest(&self) -> u64 {
        let mut digest = StateDigest::new();
        digest.write_u32(self.cpu.pc);
        digest.write_u16(self.cpu.get_sr());
        for register in self.cpu.dar {
            digest.write_u32(register);
        }
        digest.write_u64(self.total_cycles);
        digest.write_u16_slice(&self.memory.custom_regs);
        digest.write_u16(self.chipset.intreq);
        digest.write_u16(self.chipset.intena);
        digest.write_u16(self.chipset.dmacon);
        digest.write_u16(self.chipset.vpos);
        digest.write_u16(self.chipset.hpos);
        digest.write_u16(self.video_standard.digest_tag());
        digest.write_u64(self.frames_run);
        // Pending input is machine state. Without it two runs that differ only in a
        // queued keystroke would be reported as identical, which would make the
        // replay determinism claim vacuous.
        digest.write_u64(self.key_events.dropped());
        for (keycode, pressed) in self.key_events.iter() {
            digest.write_u16(u16::from(*keycode));
            digest.write_u16(u16::from(*pressed));
        }
        // Big-endian bytes rather than a cast: the digest must not depend on host
        // byte order, and this needs no MSRV-sensitive conversion.
        digest.write_bytes(&self.mouse_dx.to_be_bytes());
        digest.write_bytes(&self.mouse_dy.to_be_bytes());
        digest.write_u16(u16::from(self.mouse_x_counter));
        digest.write_u16(u16::from(self.mouse_y_counter));
        digest.write_u16(u16::from(self.mouse_left));
        digest.write_u16(u16::from(self.mouse_right));
        {
            let cia = self.memory.cia.borrow();
            cia.cia_a.write_digest(&mut digest);
            cia.cia_b.write_digest(&mut digest);
        }
        digest.write_bytes(self.memory.chip_ram());
        // Slow and fast RAM were previously outside the digest, so two runs could
        // differ there and still be reported as the same state.
        digest.write_bytes(self.memory.slow_ram_bytes());
        digest.write_bytes(self.memory.fast_ram_bytes());
        for drive in &self.floppy.drives {
            digest.write_u16(u16::from(drive.cyl));
            digest.write_u16(u16::from(drive.motor));
            digest.write_u32(drive.mfm_pos);
            digest.write_u16(u16::from(drive.dskready));
            digest.write_u16(u16::from(drive.disk_changed));
            digest.write_u16(u16::from(drive.dirty));
            // Drive metadata only. Media contents are in media_digest, which costs
            // far more to compute and is therefore a separate question.
            digest.write_u64(drive.data.as_ref().map_or(0, |data| data.len() as u64));
        }
        digest.finish()
    }

    /// Digest of loaded media contents.
    ///
    /// Kept apart from [`Self::state_digest`] because hashing a hardfile can cost
    /// gigabytes of reads, which a caller comparing machine state after every frame
    /// should not pay. Not cryptographic; see [`crate::digest`].
    ///
    /// Covers floppy images and the Gayle IDE hardfile. It does not cover the ROM,
    /// which does not change, or MFM track buffers, which are derived from the image.
    #[must_use]
    pub fn media_digest(&self) -> u64 {
        let mut digest = StateDigest::new();
        for drive in &self.floppy.drives {
            match drive.data.as_ref() {
                Some(data) => {
                    digest.write_u16(1);
                    digest.write_bytes(data);
                }
                None => digest.write_u16(0),
            }
        }
        match self.memory.ide.borrow().disk_data.as_ref() {
            Some(data) => {
                digest.write_u16(1);
                digest.write_bytes(data);
            }
            None => digest.write_u16(0),
        }
        digest.finish()
    }

    /// Read GfxBase->copinit (the system copper list pointer).
    /// Caches `GfxBase` after first successful lookup.
    fn gfx_copinit(&mut self) -> Option<u32> {
        if self.gfxbase_cache == 0 {
            // Find GfxBase by traversing the library list
            let chip = self.memory.chip_ram();
            if chip.len() < 8 {
                return None;
            }
            let eb = u32::from_be_bytes([chip[4], chip[5], chip[6], chip[7]]);
            if eb == 0 {
                return None;
            }
            // LibList at ExecBase + $17A: traverse nodes looking for graphics.library
            let mut node = self.read_long_phys(eb + 0x17A)?;
            for _ in 0..30 {
                if node == 0 {
                    break;
                }
                // Check lib_Node.ln_Name for "graphics"
                let name_ptr = self.read_long_phys(node + 10)?;
                if (0x00FC_0000..0x0100_0000).contains(&name_ptr) {
                    let rom_off = (name_ptr - 0x00FC_0000) as usize;
                    if rom_off + 8 < self.memory.rom_data().len()
                        && &self.memory.rom_data()[rom_off..rom_off + 8] == b"graphics"
                    {
                        self.gfxbase_cache = node;
                        break;
                    }
                }
                // Next node
                node = self.read_long_phys(node)?;
            }
        }
        if self.gfxbase_cache == 0 {
            return None;
        }
        // Read copinit at GfxBase + $26
        let copinit = self.read_long_phys(self.gfxbase_cache + 0x26)?;
        Some(copinit)
    }

    /// Read a big-endian u32 from physical memory (chip RAM or slow RAM).
    fn read_long_phys(&self, addr: u32) -> Option<u32> {
        let a = addr as usize;
        if a + 3 < self.memory.chip_ram().len() {
            let ram = self.memory.chip_ram();
            Some(u32::from_be_bytes([
                ram[a],
                ram[a + 1],
                ram[a + 2],
                ram[a + 3],
            ]))
        } else if addr >= 0x00C0_0000 {
            let off = (addr - 0x00C0_0000) as usize;
            if off + 3 < self.memory.slow_ram.len() {
                let ram = &self.memory.slow_ram;
                Some(u32::from_be_bytes([
                    ram[off],
                    ram[off + 1],
                    ram[off + 2],
                    ram[off + 3],
                ]))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Patch the copper list colors for the hand display.
    ///
    /// On real hardware, `LoadRGB4` updates the copper list via `DspIns`.
    /// Since our `MrgCop` doesn't set `DspIns` and `ColorMap` is NULL,
    /// `LoadRGB4` cannot update the copper list. We detect the hand display
    /// (2 planes, BPL1PT in the hand bitmap area) and write the known
    /// Kickstart 1.3 hand colors directly.
    fn sync_colormap_to_copper(&mut self) {
        let cop2 = self.copper.cop2lc as usize;
        let chip = self.memory.chip_ram_mut();
        if cop2 + 20 >= chip.len() {
            return;
        }
        // Check if this is the hand copper list (COLOR00=$0FFF at cop2+4)
        let first_reg = u16::from_be_bytes([chip[cop2 + 4], chip[cop2 + 5]]);
        let first_val = u16::from_be_bytes([chip[cop2 + 6], chip[cop2 + 7]]);
        if first_reg != 0x0180 || first_val != 0x0FFF {
            return;
        }
        // Check if all 4 colors are $0FFF (unpatched)
        let second_val = u16::from_be_bytes([chip[cop2 + 10], chip[cop2 + 11]]);
        if second_val != 0x0FFF {
            return; // Already patched or different list
        }
        // Patch with Kickstart 1.3 hand colors.
        // COLOR00=$0FFF (white bg), COLOR01=$0000 (black outline),
        // COLOR02=$077C (blue fill), COLOR03=$0BBB (gray highlight).
        let colors: [u16; 4] = [0x0FFF, 0x0000, 0x077C, 0x0BBB];
        for (i, &color) in colors.iter().enumerate() {
            let off = cop2 + 4 + i * 4; // each color entry is 4 bytes (reg + value)
            if off + 3 < chip.len() {
                #[allow(clippy::cast_possible_truncation)]
                {
                    chip[off + 2] = (color >> 8) as u8;
                    chip[off + 3] = color as u8;
                }
            }
        }
    }

    /// Returns `true` if a complete frame has been rendered.
    #[must_use]
    pub const fn is_frame_ready(&self) -> bool {
        self.frame_ready
    }

    /// Clear the frame-ready flag after consuming the frame.
    pub fn clear_frame_ready(&mut self) {
        self.frame_ready = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m68k::AddressBus;

    #[test]
    fn new_creates_valid_state() {
        let emu = Emulator::new(MemoryConfig::a500());
        assert_eq!(emu.framebuffer.len(), FRAMEBUFFER_SIZE);
        assert!(!emu.frame_ready);
        assert_eq!(emu.total_cycles, 0);
        assert!(emu.events.is_pending(EventType::HSync));
    }

    #[test]
    fn load_rom_and_cpu_reads_reset_vector() {
        let mut emu = Emulator::new(MemoryConfig::a500());

        // Build a minimal ROM: SSP at 0x0000_0800, PC at 0x00FC_0008
        let mut rom = vec![0u8; 256 * 1024];
        // Initial SSP (address 0x00000000 via overlay)
        rom[0] = 0x00;
        rom[1] = 0x00;
        rom[2] = 0x08;
        rom[3] = 0x00;
        // Initial PC (address 0x00000004 via overlay)
        rom[4] = 0x00;
        rom[5] = 0xFC;
        rom[6] = 0x00;
        rom[7] = 0x08;
        // At PC=0x00FC0008 (ROM offset 8): NOP (0x4E71)
        rom[8] = 0x4E;
        rom[9] = 0x71;

        emu.load_rom(&rom);

        // The CPU reset happened before ROM load, so reset again to pick up vectors.
        // Re-reset to pick up the new vectors.
        emu.cpu.reset(&mut emu.memory);

        // After reset, PC should be at the reset vector address
        let pc = emu.cpu.pc;
        assert!(
            pc >= 0x00FC_0008,
            "PC should point to reset vector address, got {pc:#010X}"
        );
    }

    #[test]
    fn custom_pointer_low_writes_are_word_aligned() {
        let mut emu = Emulator::new(MemoryConfig::a500());

        emu.dispatch_register_write(custom::BLTDPTH, 0x0001);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2345);
        assert_eq!(emu.blitter.bltdpt, 0x0001_2344);

        emu.dispatch_register_write(custom::BPL1PTH, 0x0002);
        emu.dispatch_register_write(custom::BPL1PTL, 0x3457);
        assert_eq!(emu.playfield.bplpt[0], 0x0002_3456);

        emu.dispatch_register_write(custom::SPR0PTH, 0x0003);
        emu.dispatch_register_write(custom::SPR0PTL, 0x4569);
        assert_eq!(emu.sprites.sprites[0].pt, 0x0003_4568);

        emu.dispatch_register_write(custom::DSKPTH, 0x0004);
        emu.dispatch_register_write(custom::DSKPTL, 0x567B);
        assert_eq!(emu.floppy.dskpt, 0x0004_567A);
    }

    #[test]
    fn disk_status_reports_active_low_drive_id_bit() {
        let mut emu = Emulator::new(MemoryConfig::a500_plus());
        emu.insert_floppy(1, Vec::new());

        emu.floppy.selected = 0x0D; // DF1 selected, motor off
        emu.floppy.drives[1].drive_id = 0xFFFF_FFFF;
        emu.floppy.drives[1].id_shift_count = 0;
        emu.update_disk_status();
        assert_eq!(emu.memory.disk_status & 0x20, 0);

        emu.floppy.drives[1].drive_id = 0;
        emu.update_disk_status();
        assert_eq!(emu.memory.disk_status & 0x20, 0x20);
    }

    #[test]
    fn blitter_pointers_masked_by_chip_ram_size() {
        // A500 profile has 512KB chip RAM (mask = 0x0007_FFFF)
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.dispatch_register_write(custom::BLTAPTH, 0x000F); // outside 512KB
        emu.dispatch_register_write(custom::BLTAPTL, 0x0000);
        // 0x000F_0000 & 0x0007_FFFF = 0x0007_0000
        assert_eq!(emu.blitter.bltapt, 0x0007_0000);

        // A1200 profile has 2MB chip RAM (mask = 0x001F_FFFF)
        let mut emu_a1200 = Emulator::new(MemoryConfig::a1200());
        emu_a1200.dispatch_register_write(custom::BLTAPTH, 0x003F); // outside 2MB
        emu_a1200.dispatch_register_write(custom::BLTAPTL, 0x0000);
        // 0x003F_0000 & 0x001F_FFFF = 0x001F_0000
        assert_eq!(emu_a1200.blitter.bltapt, 0x001F_0000);
    }

    #[test]
    fn blitter_execution_completes_and_syncs_state() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;

        // Write some source data to Chip RAM at 0x1000
        emu.memory.write_byte(0x1000, 0xAA);
        emu.memory.write_byte(0x1001, 0xBB);
        emu.memory.write_byte(0x1002, 0xCC);
        emu.memory.write_byte(0x1003, 0xDD);

        // Setup blitter for A->D copy
        // bltcon0 = USE_A | USE_D | 0xF0 (copy A)
        emu.dispatch_register_write(custom::BLTCON0, 0x0F00 | 0x00D0 | 0x00F0);
        // bltapt = 0x1000
        emu.dispatch_register_write(custom::BLTAPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTAPTL, 0x1000);
        // bltdpt = 0x2000
        emu.dispatch_register_write(custom::BLTDPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2000);

        // Trigger blit: 1 row, 2 words wide
        emu.dispatch_register_write(custom::BLTSIZE, (1 << 6) | 2);

        // Reading the destination observes the completed blit in either runtime mode.
        let val1 = emu.memory.read_byte(0x2000);
        let val2 = emu.memory.read_byte(0x2001);
        let val3 = emu.memory.read_byte(0x2002);
        let val4 = emu.memory.read_byte(0x2003);

        assert_eq!(val1, 0xAA);
        assert_eq!(val2, 0xBB);
        assert_eq!(val3, 0xCC);
        assert_eq!(val4, 0xDD);

        // The blit completed inside the write that started it.
        assert!(!emu.blitter.busy);
        assert!(emu.blitter.done);
        assert_eq!(emu.blitter.bltapt, 0x1004);
        assert_eq!(emu.blitter.bltdpt, 0x2004);
    }

    #[test]
    fn completed_blit_preserves_postincremented_pointers() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;

        for (offset, value) in [0xAA, 0xBB, 0xCC, 0xDD, 0x12, 0x34, 0x56, 0x78]
            .into_iter()
            .enumerate()
        {
            emu.memory.write_byte(0x1000 + offset as u32, value);
        }

        emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
        emu.dispatch_register_write(custom::BLTAPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTAPTL, 0x1000);
        emu.dispatch_register_write(custom::BLTDPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2000);
        emu.dispatch_register_write(custom::BLTSIZE, (1 << 6) | 2);

        emu.dispatch_register_write(custom::BLTSIZE, (1 << 6) | 2);

        assert_eq!(emu.memory.read_byte(0x2004), 0x12);
        assert_eq!(emu.memory.read_byte(0x2005), 0x34);
        assert_eq!(emu.memory.read_byte(0x2006), 0x56);
        assert_eq!(emu.memory.read_byte(0x2007), 0x78);
        assert_eq!(emu.blitter.bltapt, 0x1008);
        assert_eq!(emu.blitter.bltdpt, 0x2008);
    }

    #[test]
    fn bltcon0l_updates_only_lower_control_byte() {
        let mut emu = Emulator::new(MemoryConfig::a1200());

        emu.dispatch_register_write(custom::BLTCON0, 0xAB00);
        emu.dispatch_register_write(custom::BLTCON0L, 0x00F0);

        assert_eq!(emu.blitter.bltcon0, 0xABF0);
    }

    #[test]
    fn bltsizv_bltsizh_start_extended_blit() {
        let mut emu = Emulator::new(MemoryConfig::a1200());
        emu.memory.overlay = false;

        emu.memory.write_byte(0x1000, 0x12);
        emu.memory.write_byte(0x1001, 0x34);
        emu.memory.write_byte(0x1002, 0x56);
        emu.memory.write_byte(0x1003, 0x78);

        emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
        emu.dispatch_register_write(custom::BLTAPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTAPTL, 0x1000);
        emu.dispatch_register_write(custom::BLTDPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2000);

        emu.dispatch_register_write(custom::BLTSIZV, 1);
        emu.dispatch_register_write(custom::BLTSIZH, 2);

        assert_eq!(emu.memory.read_byte(0x2000), 0x12);
        assert_eq!(emu.memory.read_byte(0x2001), 0x34);
        assert_eq!(emu.memory.read_byte(0x2002), 0x56);
        assert_eq!(emu.memory.read_byte(0x2003), 0x78);
        assert!(emu.blitter.done);
    }

    /// Drive one A-to-D area blit and return the resulting machine digest.
    ///
    /// The blit is large enough that a background worker could not reliably
    /// finish before the caller observes the result, which is what made the
    /// previous implementation's timing observable.
    fn run_blit_fixture(rounds: u16, height: u16, width_words: u16) -> (u64, u64) {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;

        for index in 0..usize::from(height) * usize::from(width_words) * 4 {
            let value = (index as u16).wrapping_mul(0x9E37).rotate_left(3) ^ 0x5A5A;
            emu.memory.chip_ram[0x1_0000 + index * 2] = (value >> 8) as u8;
            emu.memory.chip_ram[0x1_0000 + index * 2 + 1] = value as u8;
        }

        for round in 0..rounds {
            let destination = 0x3_0000 + u32::from(round) * 0x400;
            emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
            emu.dispatch_register_write(custom::BLTCON1, 0);
            emu.dispatch_register_write(custom::BLTAPTH, 0x0001);
            emu.dispatch_register_write(custom::BLTAPTL, 0x0000);
            emu.dispatch_register_write(custom::BLTDPTH, (destination >> 16) as u16);
            emu.dispatch_register_write(custom::BLTDPTL, destination as u16);
            emu.dispatch_register_write(custom::BLTAMOD, 0);
            emu.dispatch_register_write(custom::BLTDMOD, 0);
            emu.dispatch_register_write(custom::BLTSIZE, (height << 6) | width_words);

            // Observe immediately. This is where host scheduling used to leak
            // into the guest-visible BBUSY bit.
            let _ = emu.memory.read_custom_reg(custom::DMACONR);
        }

        (emu.state_digest(), emu.frame_digest())
    }

    /// Pinned digest of the blit fixture on a PAL machine.
    ///
    /// Both runtime profiles must reach this exact value, so the portable and
    /// desktop blitter paths cannot diverge silently.
    ///
    /// The state digest moves whenever a field joins the digest, which has happened
    /// twice: M1-013 added the video standard, and M1-009 added the frame counter,
    /// pending input, CIA state, slow and fast RAM, and per-drive metadata. The frame
    /// digest has not moved, because the fixture renders no frame.
    const BLIT_FIXTURE_DIGEST: (u64, u64) = (0xa0f3_bdef_f8fc_a20f, 0x42a5_a130_53f1_6d25);

    #[test]
    fn both_runtime_profiles_reach_the_pinned_state() {
        assert_eq!(run_blit_fixture(16, 24, 12), BLIT_FIXTURE_DIGEST);
    }

    #[test]
    fn repeated_blit_runs_reach_the_same_state() {
        let expected = run_blit_fixture(16, 24, 12);
        for round in 1..8 {
            assert_eq!(
                run_blit_fixture(16, 24, 12),
                expected,
                "run {round} diverged, so blitter execution is not deterministic"
            );
        }
    }

    #[test]
    fn completed_blit_requests_the_blitter_interrupt() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;
        emu.memory.write_byte(0x1000, 0x5A);
        emu.memory.write_byte(0x1001, 0xA5);

        emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
        emu.dispatch_register_write(custom::BLTCON1, 0);
        emu.dispatch_register_write(custom::BLTAPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTAPTL, 0x1000);
        emu.dispatch_register_write(custom::BLTDPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2000);
        emu.dispatch_register_write(custom::BLTSIZE, (1 << 6) | 1);

        assert!(
            emu.chipset.intreq & custom::INT_BLIT != 0,
            "a completed blit must request the blitter interrupt in both runtime profiles"
        );
        assert_eq!(
            emu.memory.custom_regs[(custom::INTREQR / 2) as usize] & custom::INT_BLIT,
            custom::INT_BLIT,
            "the readable interrupt shadow must show the request"
        );
        assert!(!emu.blitter.busy);
        assert!(emu.blitter.done);
    }

    #[test]
    fn blitter_wait_bit_reads_clear_after_the_blit() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;

        emu.dispatch_register_write(custom::BLTCON0, 0x09F0);
        emu.dispatch_register_write(custom::BLTAPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTAPTL, 0x1000);
        emu.dispatch_register_write(custom::BLTDPTH, 0x0000);
        emu.dispatch_register_write(custom::BLTDPTL, 0x2000);
        emu.dispatch_register_write(custom::BLTSIZE, (32 << 6) | 16);

        emu.sync_readable_regs();

        // Blits are instantaneous in emulated time, so the guest wait idiom must
        // observe BBUSY clear rather than a host scheduling artifact.
        assert_eq!(
            emu.memory.read_custom_reg(custom::DMACONR) & 0x4000,
            0,
            "BBUSY must read clear once the blit is complete"
        );
    }

    #[test]
    fn frame_period_follows_the_colour_clock_not_a_rounded_rate() {
        let emu = Emulator::new(MemoryConfig::a500());
        let period = emu.frame_period();

        // 227 colour clocks x 312 lines at 3,546,895 Hz is 19.968 ms, so the naive
        // 20 ms would be wrong by 32 microseconds per frame.
        assert_eq!(period.as_nanos(), 19_967_887);
        assert_ne!(period, core::time::Duration::from_millis(20));

        // The implied rate is PAL's 50.08 Hz rather than a flat 50.
        let rate = 1.0 / period.as_secs_f64();
        assert!((rate - 50.08).abs() < 0.01, "implied rate was {rate}");
    }

    /// Machine configuration for the given standard, otherwise unchanged.
    fn config_with(standard: VideoStandard) -> MemoryConfig {
        MemoryConfig {
            video_standard: standard,
            ..MemoryConfig::a500()
        }
    }

    #[test]
    fn frame_period_follows_the_selected_standard() {
        let pal = Emulator::new(config_with(VideoStandard::Pal));
        let ntsc = Emulator::new(config_with(VideoStandard::Ntsc));

        assert_eq!(pal.frame_period().as_nanos(), 19_967_887);
        assert_eq!(ntsc.frame_period().as_nanos(), 16_614_960);

        // 60.19 Hz, not the 60 Hz the option is usually labelled with.
        let rate = 1.0 / ntsc.frame_period().as_secs_f64();
        assert!((rate - 60.19).abs() < 0.01, "implied rate was {rate}");
    }

    #[test]
    fn the_beam_wrap_agrees_with_the_frame_length() {
        // This is the assertion that fails if only the frame length follows the
        // standard and the beam wrap stays at PAL's line 311: after one NTSC frame
        // the beam would sit at line 262 instead of back at the top, and it would
        // drift by 50 lines every frame from then on.
        for standard in [VideoStandard::Pal, VideoStandard::Ntsc] {
            let mut emu = Emulator::new(config_with(standard));
            emu.memory.overlay = false;
            assert_eq!(emu.chipset.vpos, 0);
            emu.run_frame();
            assert_eq!(
                emu.chipset.vpos, 0,
                "{standard:?} left the beam at line {} after a full frame",
                emu.chipset.vpos
            );
        }
    }

    #[test]
    fn an_ntsc_frame_costs_fewer_cycles_than_a_pal_frame() {
        let mut pal = Emulator::new(config_with(VideoStandard::Pal));
        let mut ntsc = Emulator::new(config_with(VideoStandard::Ntsc));
        pal.memory.overlay = false;
        ntsc.memory.overlay = false;

        pal.run_frame();
        ntsc.run_frame();

        // A scanline costs the same in both standards, so a frame's cost follows
        // its line count: 262 lines against 312.
        assert!(
            ntsc.total_cycles < pal.total_cycles,
            "an NTSC frame ran {} cycles, not fewer than PAL's {}",
            ntsc.total_cycles,
            pal.total_cycles
        );
    }

    #[test]
    fn active_height_follows_the_selected_standard() {
        let pal = Emulator::new(config_with(VideoStandard::Pal));
        let ntsc = Emulator::new(config_with(VideoStandard::Ntsc));

        assert_eq!(pal.active_height(), 288);
        assert_eq!(ntsc.active_height(), 243);

        // The framebuffer is PAL-sized in both cases, so a presenter that used the
        // buffer height would pad an NTSC frame with 45 never-written lines.
        assert_eq!(pal.framebuffer().len(), FRAMEBUFFER_SIZE);
        assert_eq!(ntsc.framebuffer().len(), FRAMEBUFFER_SIZE);
        assert!(usize::from(ntsc.active_height()) * DISPLAY_WIDTH < FRAMEBUFFER_SIZE);
    }

    #[test]
    fn vposr_and_beamcon0_report_the_selected_standard() {
        let mut pal = Emulator::new(config_with(VideoStandard::Pal));
        let mut ntsc = Emulator::new(config_with(VideoStandard::Ntsc));
        pal.sync_readable_regs();
        ntsc.sync_readable_regs();

        let pal_vposr = pal.memory.custom_regs[(custom::VPOSR / 2) as usize];
        let ntsc_vposr = ntsc.memory.custom_regs[(custom::VPOSR / 2) as usize];
        assert_eq!(pal_vposr & 0x1000, 0, "PAL must not set the NTSC bit");
        assert_eq!(ntsc_vposr & 0x1000, 0x1000, "NTSC must set bit 12");

        let pal_beam = pal.memory.custom_regs[(custom::BEAMCON0 / 2) as usize];
        let ntsc_beam = ntsc.memory.custom_regs[(custom::BEAMCON0 / 2) as usize];
        assert_eq!(pal_beam, custom::BEAMCON0_PAL);
        assert_eq!(ntsc_beam & custom::BEAMCON0_PAL, 0);
    }

    /// Run `frames` frames of a fresh machine under `recording` and digest the result.
    fn replay_fixture(recording: &InputRecording, frames: u64) -> (u64, u64) {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;
        emu.attach_input_replay(recording.clone());
        for _ in 0..frames {
            emu.run_frame();
        }
        (emu.state_digest(), emu.frame_digest())
    }

    /// A recording exercising all three event kinds across several frames.
    fn input_fixture() -> InputRecording {
        InputRecording::parse(
            "rumiga.input-recording.v1\n             0 buttons 0 0\n             1 key 40 down\n             2 mouse 5 -3\n             3 key 40 up\n             4 buttons 1 0\n             5 mouse -2 7\n",
        )
        .expect("fixture parses")
    }

    #[test]
    fn the_same_replay_reaches_the_same_state_twice() {
        let recording = input_fixture();

        let first = replay_fixture(&recording, 8);
        let second = replay_fixture(&recording, 8);

        // This is the acceptance criterion for M1-009: same replay, same digest.
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_recording_reaches_a_different_state() {
        let baseline = replay_fixture(&input_fixture(), 8);
        let shifted = InputRecording::parse(
            "rumiga.input-recording.v1\n             0 buttons 0 0\n             1 key 41 down\n             2 mouse 5 -3\n             3 key 41 up\n             4 buttons 1 0\n             5 mouse -2 7\n",
        )
        .expect("variant parses");

        // A digest that could not tell these apart would make the equality above
        // meaningless.
        assert_ne!(baseline.0, replay_fixture(&shifted, 8).0);
    }

    #[test]
    fn replay_delivers_each_event_on_its_own_frame() {
        let recording =
            InputRecording::parse("rumiga.input-recording.v1\n0 buttons 1 0\n2 buttons 0 1\n")
                .expect("fixture parses");
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;
        emu.attach_input_replay(recording);

        emu.run_frame();
        assert!(emu.mouse_left && !emu.mouse_right);
        emu.run_frame();
        // Frame 1 has no events, so the state from frame 0 persists.
        assert!(emu.mouse_left && !emu.mouse_right);
        emu.run_frame();
        assert!(!emu.mouse_left && emu.mouse_right);
        assert!(emu.replay_exhausted());
    }

    #[test]
    fn recording_captures_what_was_delivered() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;
        emu.start_input_recording();
        assert!(emu.is_recording_input());

        emu.mouse_button(false, false);
        emu.key_event(0x40, true);
        emu.run_frame();
        emu.mouse_move(3, -1);
        emu.mouse_button(true, false);
        emu.run_frame();

        let recording = emu
            .finish_input_recording()
            .expect("a recording was running")
            .expect("frames only advance");
        assert!(!emu.is_recording_input());

        // Frame stamps come from the machine, so the second frame's events carry 1.
        assert_eq!(recording.len(), 4);
        assert_eq!(recording.events()[0].frame, 0);
        assert_eq!(recording.events()[2].frame, 1);
        assert_eq!(recording.last_frame(), Some(1));
    }

    #[test]
    fn a_recording_replays_to_the_state_it_was_captured_from() {
        let mut recorded = Emulator::new(MemoryConfig::a500());
        recorded.memory.overlay = false;
        recorded.start_input_recording();
        recorded.mouse_button(false, false);
        recorded.key_event(0x40, true);
        recorded.run_frame();
        recorded.mouse_move(3, -1);
        recorded.run_frame();
        recorded.key_event(0x40, false);
        recorded.run_frame();
        let expected = (recorded.state_digest(), recorded.frame_digest());
        let recording = recorded
            .finish_input_recording()
            .expect("recording ran")
            .expect("ordered");

        // Round trip through the text form as well, so the format is on the path.
        let reparsed = InputRecording::parse(&recording.to_text()).expect("round trip");
        assert_eq!(replay_fixture(&reparsed, 3), expected);
    }

    #[test]
    fn the_frame_counter_advances_with_the_machine() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.memory.overlay = false;
        assert_eq!(emu.frames_run(), 0);

        emu.run_frame();
        emu.run_frame();

        assert_eq!(emu.frames_run(), 2);
    }

    #[test]
    fn the_state_digest_covers_slow_and_fast_ram() {
        let config = MemoryConfig {
            slow_ram_size: 512 * 1024,
            fast_ram_size: 1024 * 1024,
            ..MemoryConfig::a500()
        };
        let mut baseline = Emulator::new(config.clone());
        let mut mutated = Emulator::new(config);
        baseline.memory.overlay = false;
        mutated.memory.overlay = false;

        mutated.memory.slow_ram[0] ^= 0xFF;

        // Before M1-009 these two would have digested identically.
        assert_ne!(baseline.state_digest(), mutated.state_digest());
    }

    #[test]
    fn the_media_digest_is_separate_from_the_state_digest() {
        let mut without = Emulator::new(MemoryConfig::a500());
        let mut with_disk = Emulator::new(MemoryConfig::a500());
        without.memory.overlay = false;
        with_disk.memory.overlay = false;

        with_disk.insert_floppy(0, vec![0xA5; 512]);

        // Media contents are digested apart because hashing a hardfile is expensive.
        assert_ne!(without.media_digest(), with_disk.media_digest());
    }

    #[test]
    fn the_keyboard_queue_reports_saturation_instead_of_swallowing_events() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        let capacity = emu.key_queue_capacity();

        for index in 0..capacity {
            assert_eq!(
                emu.key_event(u8::try_from(index).unwrap_or(0), true),
                QueueAdmission::Accepted
            );
        }

        // Before M1-008 this event vanished with no record: the bound was an unnamed
        // length check and nothing counted the loss.
        assert_eq!(emu.key_event(0x40, true), QueueAdmission::Rejected);

        assert_eq!(emu.key_queue_depth(), capacity);
        assert_eq!(emu.key_queue_high_water(), capacity);
        assert_eq!(emu.key_events_dropped(), 1);
    }

    #[test]
    fn the_keyboard_queue_keeps_what_was_already_typed() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        for index in 0..emu.key_queue_capacity() {
            emu.key_event(u8::try_from(index).unwrap_or(0), true);
        }

        emu.key_event(0x7F, true);

        // RejectNewest preserves typing order, which is why the policy is named.
        assert_eq!(emu.key_events.peek().copied(), Some((0, true)));
    }

    #[test]
    fn the_keyboard_high_water_mark_outlives_the_backlog() {
        let mut emu = Emulator::new(MemoryConfig::a500());
        emu.key_event(0x40, true);
        emu.key_event(0x41, true);
        while emu.key_events.pop().is_some() {}

        // An empty queue that once held two events must not read as never busy.
        assert_eq!(emu.key_queue_depth(), 0);
        assert_eq!(emu.key_queue_high_water(), 2);
        assert_eq!(emu.key_events_dropped(), 0);
    }

    #[test]
    fn state_digest_separates_the_two_standards() {
        let pal = Emulator::new(config_with(VideoStandard::Pal));
        let ntsc = Emulator::new(config_with(VideoStandard::Ntsc));

        // The two machines differ in nothing else, so a digest that ignored the
        // standard would call them identical.
        assert_ne!(pal.state_digest(), ntsc.state_digest());
    }

    #[test]
    fn state_digest_separates_distinct_machine_states() {
        let baseline = run_blit_fixture(1, 4, 4);
        let more_work = run_blit_fixture(2, 4, 4);
        let taller = run_blit_fixture(1, 8, 4);

        assert_ne!(
            baseline, more_work,
            "a second blit must change the machine digest"
        );
        assert_ne!(
            baseline, taller,
            "a taller blit must change the machine digest"
        );
    }

    #[test]
    fn test_mouse_and_keyboard_handling() {
        let mut emu = Emulator::new(MemoryConfig::a500());

        // 1. Verify keyboard event triggers interrupt on serial port (bit 3)
        // Enable Serial Port interrupt in CIA-A ICR mask (0x08)
        emu.memory.cia.borrow_mut().cia_a.write(0xD, 0x88); // Set bit 7 (SET) and bit 3 (SP)
        assert_eq!(emu.memory.cia.borrow().cia_a.icr_mask & 0x08, 0x08);

        // Queue key press
        emu.key_event(0x10, true); // 'Q' pressed

        // Run scanline to process the queued key
        emu.run_scanline();

        // CIA-A ICR_SP interrupt should be pending
        {
            let cia_a = &emu.memory.cia.borrow().cia_a;
            assert_eq!(cia_a.sdr, 0xDF);
            assert!(cia_a.icr_ir);
            assert_ne!(cia_a.icr_data & 0x08, 0);
        }

        // Reading REG_ICR (0xD) should return the interrupt and clear it
        let icr = emu.memory.cia.borrow_mut().cia_a.read(0xD);
        assert_ne!(icr & 0x08, 0);
        assert!(!emu.memory.cia.borrow().cia_a.icr_ir);

        // 2. Verify mouse deltas and button mapping
        emu.mouse_move(10, -5);
        emu.mouse_button(true, true);

        emu.sync_readable_regs();

        // Left mouse button is active-low in CIA-A PRA bit 6, so reading REG_PRA when pressed should have bit 6 clear
        let pra = emu.memory.read_byte(0x00BF_E001);
        assert_eq!(pra & 0x40, 0); // Bit 6 is 0 (Left mouse button pressed)

        // Right button is active-low in POTGOR bit 10
        let potgor = emu.memory.custom_regs[0x016 / 2];
        assert_eq!(potgor & (1 << 10), 0); // Bit 10 is 0 (Right mouse button pressed)

        // Mouse quadrature counters in JOY0DAT
        let joy0dat = emu.memory.custom_regs[0x00A / 2];
        let x_counter = (joy0dat & 0xFF) as u8;
        let y_counter = ((joy0dat >> 8) & 0xFF) as u8;
        assert_eq!(x_counter, 10);
        assert_eq!(y_counter, 251); // -5 as u8 = 251
    }
}
