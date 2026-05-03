// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2025 Fabian Schmieder

//! Rumiga desktop binary — development and debugging target.

use rumiga_platform::VideoOutput;
use rumiga_platform_desktop::DesktopVideo;

const WIDTH: usize = 320;
const HEIGHT: usize = 256;

#[allow(clippy::cast_possible_truncation)]
fn build_gradient(framebuffer: &mut [u16]) {
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let r = (x * 31 / WIDTH) as u16;
            let g = (y * 63 / HEIGHT) as u16;
            let b = ((x + y) * 31 / (WIDTH + HEIGHT)) as u16;
            framebuffer[y * WIDTH + x] = (r << 11) | (g << 5) | b;
        }
    }
}

fn main() {
    println!("rumiga desktop emulator");

    let mut video = DesktopVideo::new("Rumiga", WIDTH, HEIGHT, 2).unwrap();

    let mut framebuffer = vec![0u16; WIDTH * HEIGHT];
    build_gradient(&mut framebuffer);

    #[allow(clippy::cast_possible_truncation)]
    let (w, h) = (WIDTH as u32, HEIGHT as u32);

    while video.is_open() {
        video.present_frame(&framebuffer, w, h);
    }
}
