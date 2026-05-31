// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Paula audio emulation.
//!
//! Implements the four-channel DMA-driven audio subsystem of the Amiga,
//! including period-based sample playback, volume scaling, and configurable
//! stereo mixing.

#![allow(
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_precision_loss
)]

/// Number of audio channels (Paula has 4).
pub const NUM_CHANNELS: usize = 4;

/// PAL color clock frequency in Hz.
pub const AMIGA_CLOCK_PAL: u32 = 3_546_895;

/// Output sample rate in Hz.
pub const OUTPUT_SAMPLE_RATE: u32 = 44_100;

/// Per-channel stereo mix configuration.
#[derive(Clone, Copy, Debug)]
pub struct ChannelMix {
    /// Left channel percentage (0–100).
    pub left_pct: u8,
    /// Right channel percentage (0–100).
    pub right_pct: u8,
}

/// State of a single Paula audio channel.
#[derive(Clone, Debug)]
pub struct AudioChannel {
    /// Sample pointer (`AUDxLC`).
    pub ptr: u32,
    /// Length in words (`AUDxLEN`).
    pub len: u16,
    /// Period (`AUDxPER`) — lower value = higher frequency.
    pub period: u16,
    /// Volume 0–64 (`AUDxVOL`).
    pub volume: u16,
    /// Data register (`AUDxDAT`).
    pub dat: u16,
    /// Reload pointer (latched at start).
    pub ptr_reload: u32,
    /// Remaining length counter in words.
    pub len_counter: u16,
    /// Period countdown (decremented each DMA tick).
    pub period_counter: u16,
    /// Current output sample (signed 8-bit).
    pub sample_byte: i8,
    /// Which byte of dat to output next (true = high byte first).
    pub high_byte: bool,
    /// Channel DMA enabled.
    pub dma_enabled: bool,
    /// Interrupt pending (buffer empty).
    pub irq_pending: bool,
}

impl Default for AudioChannel {
    fn default() -> Self {
        Self {
            ptr: 0,
            len: 0,
            period: 1,
            volume: 0,
            dat: 0,
            ptr_reload: 0,
            len_counter: 0,
            period_counter: 0,
            sample_byte: 0,
            high_byte: true,
            dma_enabled: false,
            irq_pending: false,
        }
    }
}

/// Paula audio state for all four channels.
#[derive(Clone, Debug)]
pub struct AudioState {
    /// The four audio channels.
    pub channels: [AudioChannel; NUM_CHANNELS],
    /// Per-channel stereo mix configuration.
    pub channel_mix: [ChannelMix; NUM_CHANNELS],
    /// Fractional accumulator for sample generation (clock ticks carried over).
    frac_accum: u32,
    /// Left filter state for digital low-pass filtering.
    pub filter_state_l: f32,
    /// Right filter state for digital low-pass filtering.
    pub filter_state_r: f32,
    /// Whether the low-pass audio filter (LED filter) is active.
    pub filter_active: bool,
    /// Whether the IIR filter state has been initialized with the first sample.
    pub filter_initialized: bool,
}

impl AudioState {
    /// Creates a new `AudioState` with default Amiga stereo separation:
    /// channels 0 and 3 fully left, channels 1 and 2 fully right.
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: [
                AudioChannel::default(),
                AudioChannel::default(),
                AudioChannel::default(),
                AudioChannel::default(),
            ],
            channel_mix: [
                ChannelMix {
                    left_pct: 100,
                    right_pct: 0,
                },
                ChannelMix {
                    left_pct: 0,
                    right_pct: 100,
                },
                ChannelMix {
                    left_pct: 0,
                    right_pct: 100,
                },
                ChannelMix {
                    left_pct: 100,
                    right_pct: 0,
                },
            ],
            frac_accum: 0,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
            filter_active: false,
            filter_initialized: false,
        }
    }

    /// Sets the stereo mix percentages for a given channel.
    pub fn set_channel_mix(&mut self, channel: usize, left_pct: u8, right_pct: u8) {
        if channel < NUM_CHANNELS {
            self.channel_mix[channel] = ChannelMix {
                left_pct,
                right_pct,
            };
        }
    }

    /// Apply stereo separation scaling between mono (0%) and hard-pan (100%).
    ///
    /// At 100% (default), channels 0 and 3 are 100% left, 1 and 2 are 100% right.
    /// At 0%, all channels are mixed 50% left and 50% right (mono).
    /// Panning scales linearly in between.
    pub fn apply_separation(&mut self, separation: u8) {
        let sep = separation.min(100);
        let main_pct = 50 + sep / 2;
        let cross_pct = 50 - sep / 2;

        self.channel_mix[0] = ChannelMix {
            left_pct: main_pct,
            right_pct: cross_pct,
        };
        self.channel_mix[1] = ChannelMix {
            left_pct: cross_pct,
            right_pct: main_pct,
        };
        self.channel_mix[2] = ChannelMix {
            left_pct: cross_pct,
            right_pct: main_pct,
        };
        self.channel_mix[3] = ChannelMix {
            left_pct: main_pct,
            right_pct: cross_pct,
        };
    }

    /// Advances one channel by one DMA tick.
    ///
    /// Decrements `period_counter`; on underflow outputs the next sample byte
    /// from `dat`, fetches a new dat word when both bytes are consumed, and
    /// reloads the pointer when the length is exhausted.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    pub fn tick_channel(&mut self, ch: usize, chip_ram: &[u8]) {
        let channel = &mut self.channels[ch];
        if !channel.dma_enabled {
            return;
        }

        channel.period_counter = channel.period_counter.saturating_sub(1);
        if channel.period_counter == 0 {
            channel.period_counter = channel.period;

            if channel.high_byte {
                channel.sample_byte = ((channel.dat >> 8) & 0xFF) as u8 as i8;
                channel.high_byte = false;
            } else {
                channel.sample_byte = (channel.dat & 0xFF) as u8 as i8;
                channel.high_byte = true;
                // Both bytes consumed — fetch next word from chip RAM
                self.fetch_next_word(ch, chip_ram);
            }
        }
    }

    /// Fetches the next sample word from chip RAM for the given channel.
    fn fetch_next_word(&mut self, ch: usize, chip_ram: &[u8]) {
        let channel = &mut self.channels[ch];
        let addr = channel.ptr as usize;

        if addr + 1 < chip_ram.len() {
            channel.dat = (u16::from(chip_ram[addr]) << 8) | u16::from(chip_ram[addr + 1]);
        }
        channel.ptr = channel.ptr.wrapping_add(2);

        channel.len_counter = channel.len_counter.saturating_sub(1);
        if channel.len_counter == 0 {
            channel.ptr = channel.ptr_reload;
            channel.len_counter = channel.len;
            channel.irq_pending = true;
        }
    }

    /// Generates `num_samples` of stereo audio output.
    ///
    /// Ticks channels at the correct rate relative to [`OUTPUT_SAMPLE_RATE`],
    /// then mixes to stereo using per-channel volume and mix percentages.
    #[allow(clippy::cast_possible_truncation)]
    pub fn generate_samples(
        &mut self,
        chip_ram: &[u8],
        left: &mut [i16],
        right: &mut [i16],
        num_samples: usize,
    ) {
        for i in 0..num_samples {
            // Determine how many color clocks to advance for this output sample
            self.frac_accum += AMIGA_CLOCK_PAL;
            let ticks = self.frac_accum / OUTPUT_SAMPLE_RATE;
            self.frac_accum %= OUTPUT_SAMPLE_RATE;

            for _ in 0..ticks {
                for ch in 0..NUM_CHANNELS {
                    self.tick_channel(ch, chip_ram);
                }
            }

            // Mix channels
            let mut left_sum: i32 = 0;
            let mut right_sum: i32 = 0;

            for ch in 0..NUM_CHANNELS {
                let sample_value =
                    i32::from(self.channels[ch].sample_byte) * i32::from(self.channels[ch].volume);
                let mix = self.channel_mix[ch];
                left_sum += sample_value * i32::from(mix.left_pct) / 100;
                right_sum += sample_value * i32::from(mix.right_pct) / 100;
            }

            let mut left_val = left_sum as f32;
            let mut right_val = right_sum as f32;

            if !self.filter_initialized {
                self.filter_state_l = left_val;
                self.filter_state_r = right_val;
                self.filter_initialized = true;
            }

            if self.filter_active {
                // LED filter active: cut off above ~3.2 kHz (alpha = 0.35)
                let alpha = 0.35f32;
                self.filter_state_l += alpha * (left_val - self.filter_state_l);
                self.filter_state_r += alpha * (right_val - self.filter_state_r);
                left_val = self.filter_state_l;
                right_val = self.filter_state_r;
            } else {
                // Filter bypassed: still apply Paula's default high-end filter (~16 kHz anti-aliasing)
                let alpha = 0.85f32;
                self.filter_state_l += alpha * (left_val - self.filter_state_l);
                self.filter_state_r += alpha * (right_val - self.filter_state_r);
                left_val = self.filter_state_l;
                right_val = self.filter_state_r;
            }

            left[i] = left_val.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            right[i] = right_val.clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        }
    }
}

impl Default for AudioState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_stereo_mix() {
        let state = AudioState::new();
        assert_eq!(state.channel_mix[0].left_pct, 100);
        assert_eq!(state.channel_mix[0].right_pct, 0);
        assert_eq!(state.channel_mix[1].left_pct, 0);
        assert_eq!(state.channel_mix[1].right_pct, 100);
        assert_eq!(state.channel_mix[2].left_pct, 0);
        assert_eq!(state.channel_mix[2].right_pct, 100);
        assert_eq!(state.channel_mix[3].left_pct, 100);
        assert_eq!(state.channel_mix[3].right_pct, 0);
    }

    #[test]
    fn volume_scaling() {
        let mut state = AudioState::new();
        state.channels[0].sample_byte = 127;
        state.channels[0].volume = 64;

        let mut left = [0i16; 1];
        let mut right = [0i16; 1];
        state.generate_samples(&[], &mut left, &mut right, 1);
        let full = left[0];

        // Reset and test volume 0 on a fresh state to avoid IIR filter carry-over
        let mut state_silent = AudioState::new();
        state_silent.channels[0].sample_byte = 127;
        state_silent.channels[0].volume = 0;
        state_silent.generate_samples(&[], &mut left, &mut right, 1);
        assert_eq!(left[0], 0);
        assert_ne!(full, 0);
    }

    #[test]
    fn period_counter_lower_means_faster() {
        // Channel with period=2 should change samples faster than period=100
        let chip_ram = [0x7F, 0x80, 0x40, 0x60]; // two words of sample data

        let mut state = AudioState::new();
        let ch = &mut state.channels[0];
        ch.dma_enabled = true;
        ch.period = 2;
        ch.period_counter = 2;
        ch.dat = 0x7F80;
        ch.high_byte = true;
        ch.volume = 64;
        ch.ptr = 0;
        ch.ptr_reload = 0;
        ch.len = 2;
        ch.len_counter = 2;

        // Tick a few times with short period
        let mut changes_fast = 0i8;
        let mut prev = state.channels[0].sample_byte;
        for _ in 0..10 {
            state.tick_channel(0, &chip_ram);
            if state.channels[0].sample_byte != prev {
                changes_fast += 1;
                prev = state.channels[0].sample_byte;
            }
        }

        // Now with long period
        let mut state2 = AudioState::new();
        let ch2 = &mut state2.channels[0];
        ch2.dma_enabled = true;
        ch2.period = 100;
        ch2.period_counter = 100;
        ch2.dat = 0x7F80;
        ch2.high_byte = true;
        ch2.volume = 64;
        ch2.ptr = 0;
        ch2.ptr_reload = 0;
        ch2.len = 2;
        ch2.len_counter = 2;

        let mut changes_slow = 0i8;
        let mut prev2 = state2.channels[0].sample_byte;
        for _ in 0..10 {
            state2.tick_channel(0, &chip_ram);
            if state2.channels[0].sample_byte != prev2 {
                changes_slow += 1;
                prev2 = state2.channels[0].sample_byte;
            }
        }

        assert!(changes_fast > changes_slow);
    }

    #[test]
    fn tick_produces_correct_sample_bytes() {
        let chip_ram = [0x12, 0x34]; // next word to fetch

        let mut state = AudioState::new();
        let ch = &mut state.channels[0];
        ch.dma_enabled = true;
        ch.period = 1;
        ch.period_counter = 1;
        ch.dat = 0xA0_50; // high=0xA0 (-96), low=0x50 (80)
        ch.high_byte = true;
        ch.ptr = 0;
        ch.ptr_reload = 0;
        ch.len = 1;
        ch.len_counter = 1;

        // First tick: outputs high byte
        state.tick_channel(0, &chip_ram);
        assert_eq!(state.channels[0].sample_byte, -96_i8); // 0xA0 as i8

        // Second tick: outputs low byte, then fetches next word
        state.tick_channel(0, &chip_ram);
        assert_eq!(state.channels[0].sample_byte, 0x50_i8); // 80
    }

    #[test]
    fn custom_mix_percentages() {
        let mut state = AudioState::new();
        state.set_channel_mix(0, 50, 50);

        state.channels[0].sample_byte = 100;
        state.channels[0].volume = 64;

        let mut left = [0i16; 1];
        let mut right = [0i16; 1];
        state.generate_samples(&[], &mut left, &mut right, 1);

        // 100 * 64 * 50 / 100 = 3200
        assert_eq!(left[0], 3200);
        assert_eq!(right[0], 3200);
    }

    #[test]
    fn test_low_pass_filter_attenuates_high_frequencies() {
        let mut state = AudioState::new();
        state.channels[0].volume = 64;
        state.channels[0].dma_enabled = true;
        state.channels[0].period = 1;
        state.channels[0].period_counter = 1;
        state.channels[0].high_byte = true;

        // Populate chip_ram with alternating high/low values (high frequency square wave)
        let chip_ram = [100u8, 156u8, 100u8, 156u8, 100u8, 156u8];

        // 1. Run with filter inactive
        state.filter_active = false;
        state.filter_state_l = 0.0;
        let mut left_inactive = [0i16; 10];
        let mut right_inactive = [0i16; 10];
        state.generate_samples(&chip_ram, &mut left_inactive, &mut right_inactive, 10);

        // 2. Run with filter active
        let mut state_active = AudioState::new();
        state_active.channels[0].volume = 64;
        state_active.channels[0].dma_enabled = true;
        state_active.channels[0].period = 1;
        state_active.channels[0].period_counter = 1;
        state_active.channels[0].high_byte = true;
        state_active.filter_active = true;
        state_active.filter_state_l = 0.0;
        let mut left_active = [0i16; 10];
        let mut right_active = [0i16; 10];
        state_active.generate_samples(&chip_ram, &mut left_active, &mut right_active, 10);

        // Verify attenuation: the absolute sum of filtered samples must be strictly less
        let sum_inactive: i32 = left_inactive.iter().map(|&s| i32::from(s.abs())).sum();
        let sum_active: i32 = left_active.iter().map(|&s| i32::from(s.abs())).sum();

        println!("Sum inactive: {sum_inactive}, Sum active: {sum_active}");
        assert!(
            sum_active < sum_inactive,
            "Active low pass should attenuate high frequencies"
        );
    }

    #[test]
    fn test_apply_separation() {
        let mut state = AudioState::new();

        // Test 100% (default)
        state.apply_separation(100);
        assert_eq!(state.channel_mix[0].left_pct, 100);
        assert_eq!(state.channel_mix[0].right_pct, 0);
        assert_eq!(state.channel_mix[1].left_pct, 0);
        assert_eq!(state.channel_mix[1].right_pct, 100);

        // Test 0% (mono)
        state.apply_separation(0);
        assert_eq!(state.channel_mix[0].left_pct, 50);
        assert_eq!(state.channel_mix[0].right_pct, 50);
        assert_eq!(state.channel_mix[1].left_pct, 50);
        assert_eq!(state.channel_mix[1].right_pct, 50);

        // Test 50%
        state.apply_separation(50);
        assert_eq!(state.channel_mix[0].left_pct, 75);
        assert_eq!(state.channel_mix[0].right_pct, 25);
        assert_eq!(state.channel_mix[1].left_pct, 25);
        assert_eq!(state.channel_mix[1].right_pct, 75);
    }
}
