#![no_std]
#![no_main]
#![feature(generic_arg_infer)]

use config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use core::fmt::Write;
use display::{Display, DisplayPeripherals, DisplayTrait};
use embedded_graphics::prelude::Point;
use esp_alloc::{heap_allocator, psram_allocator};
use esp_hal::{clock::CpuClock, entry, time};
use heapless::String;
use micromath::{vector::F32x3, Quaternion};
use {defmt_rtt as _, esp_backtrace as _};

extern crate alloc;

mod config;
mod display;

// Cube and projection constants
const FOV: f32 = 150.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const ROTATION_SPEED: f32 = 0.03;

#[entry]
fn main() -> ! {
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

    psram_allocator!(peripherals.PSRAM, esp_hal::psram);

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
    let half_width = (DISPLAY_WIDTH / 2) as i32;
    let half_height = (DISPLAY_HEIGHT / 2) as i32;

    //while window.is_open() && !window.is_key_down(Key::Escape) {

    loop {
        // FPS calculation and display
        let current_time = time::now().duration_since_epoch().to_millis();

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

        let q = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), ROTATION_SPEED);
        rotation = q * rotation;

        // Transform and project vertices
        let mut transformed_vertices = [(0i32, 0i32); 8]; // Fixed size array

        for (i, &v) in cube_vertices.iter().enumerate() {
            let rotated = rotation.rotate(v);
            let x = rotated.x;
            let y = rotated.y;
            let z = rotated.z + PROJECTION_DISTANCE;

            let px = ((x * FOV) / z) as i32 + half_width;
            let py = ((y * FOV) / z) as i32 + half_height;
            transformed_vertices[i] = (px, py);
        }

        // Draw edges
        for &(start, end) in &edges {
            let begin = Point::new(transformed_vertices[start].0, transformed_vertices[start].1);
            let end = Point::new(transformed_vertices[end].0, transformed_vertices[end].1);
            display.draw_line(begin, end).expect("Draw line failed");
        }

        let ms_per_frame = current_time - last_time;
        if (ms_per_frame) > 0 {
            let mut text = String::<16>::new();
            write!(text, "FPS: {}", 1000 / ms_per_frame).expect("Write failed");
            display
                .write(&text, Point::new(0, 0))
                .expect("Write text failed");
        }

        last_time = current_time;

        display
            .update_with_buffer()
            .expect("Update with buffer failed");
    }
}
