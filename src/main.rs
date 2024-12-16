#![no_std]
#![no_main]

use defmt::info;
use display::{Display, DisplayPeripherals, DisplayTrait};
use embassy_executor::Spawner;
use embassy_time::Instant;
use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
use esp_alloc::heap_allocator;
use esp_hal::clock::CpuClock;
use esp_hal_embassy::main;
use heapless::Vec;
use micromath::{
    vector::{F32x3, I32x2},
    Quaternion,
};
use {defmt_rtt as _, esp_backtrace as _};

mod config;
mod display;

const WIDTH: usize = 320;
const HEIGHT: usize = 170;
const BUFFER_SIZE: usize = WIDTH * HEIGHT;

// Cube and projection constants
const FOV: f32 = 150.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const ROTATION_SPEED: f32 = 0.03;

#[main]
async fn main(_spawner: Spawner) {
    let mut buffer: Vec<Rgb565, BUFFER_SIZE> = Vec::new(); // Initialize the framebuffer
    buffer.resize(BUFFER_SIZE, Rgb565::BLACK).unwrap(); // Fill the buffer with zeros

    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::Clock240MHz;
        config
    });

    heap_allocator!(72 * 1024);

    let display_peripherals = DisplayPeripherals {
        backlight: peripherals.GPIO38,
        cs: peripherals.GPIO6,
        dc: peripherals.GPIO7,
        rst: peripherals.GPIO5,
        wr: peripherals.GPIO8,
        rd: peripherals.GPIO9,
        d0: peripherals.GPIO39,
        d1: peripherals.GPIO40,
        d2: peripherals.GPIO41,
        d3: peripherals.GPIO42,
        d4: peripherals.GPIO45,
        d5: peripherals.GPIO46,
        d6: peripherals.GPIO47,
        d7: peripherals.GPIO48,
    };

    let mut display = Display::new(display_peripherals).expect("Display init failed");

    // Define cube vertices
    let cube_vertices: [F32x3; 8] = [
        F32x3::from((-1.0, -1.0, -1.0)),
        F32x3::from((1.0, -1.0, -1.0)),
        F32x3::from((1.0, 1.0, -1.0)),
        F32x3::from((-1.0, 1.0, -1.0)),
        F32x3::from((-1.0, -1.0, 1.0)),
        F32x3::from((1.0, -1.0, 1.0)),
        F32x3::from((1.0, 1.0, 1.0)),
        F32x3::from((-1.0, 1.0, 1.0)),
    ];

    // Define cube edges (pairs of vertex indices)
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0), // Back face
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4), // Front face
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7), // Connecting edges
    ];

    let mut rotation = Quaternion::IDENTITY;
    let mut last_time = 0;
    let half_width = (WIDTH / 2) as i32;
    let half_height = (HEIGHT / 2) as i32;

    //while window.is_open() && !window.is_key_down(Key::Escape) {

    loop {
        // FPS calculation and display
        let current_time = Instant::now().as_millis();
        buffer.fill(Rgb565::BLACK);

        /* // Update rotation based on keyboard input
        if window.is_key_down(Key::Left) {
            let q = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), -ROTATION_SPEED);
            rotation = q * rotation;
        } else {
            let q = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), -0.01);
            rotation = q * rotation;
        }
        if window.is_key_down(Key::Right) {
            let q = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), ROTATION_SPEED);
            rotation = q * rotation;
        }
        if window.is_key_down(Key::Up) {
            let q = Quaternion::axis_angle(F32x3::from((1.0, 0.0, 0.0)), -ROTATION_SPEED);
            rotation = q * rotation;
        }
        if window.is_key_down(Key::Down) {
            let q = Quaternion::axis_angle(F32x3::from((1.0, 0.0, 0.0)), ROTATION_SPEED);
            rotation = q * rotation;
        } */

        let q = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), -0.10);
        rotation = q * rotation;

        // Transform and project vertices
        let transformed_vertices: Vec<I32x2, 8> = cube_vertices
            .iter()
            .map(|&v| {
                let rotated = rotation.rotate(v);
                let x = rotated.x;
                let y = rotated.y;
                let z = rotated.z + PROJECTION_DISTANCE;

                let px = ((x * FOV) / z) as i32 + half_width;
                let py = ((y * FOV) / z) as i32 + half_height;
                I32x2::from((px, py))
            })
            .collect();

        // Draw edges
        for &(start, end) in &edges {
            draw_line(
                &mut buffer,
                &transformed_vertices[start],
                &transformed_vertices[end],
            );
        }

        let ms_per_frame = current_time - last_time;
        if (ms_per_frame) > 0 {
            info!("FPS: {}", 1000 / ms_per_frame);
        }

        last_time = current_time;

        display.update_with_buffer(&buffer).unwrap();
    }
}

#[inline]
fn draw_line(buffer: &mut [Rgb565], start: &I32x2, end: &I32x2) {
    let (mut x0, mut y0) = (start.x, start.y);
    let (x1, y1) = (end.x, end.y);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && x0 < WIDTH as i32 && y0 >= 0 && y0 < HEIGHT as i32 {
            buffer[y0 as usize * WIDTH + x0 as usize] = Rgb565::WHITE;
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
