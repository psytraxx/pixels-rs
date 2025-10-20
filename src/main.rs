#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use alloc::string::String;
use config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use core::{
    cell::RefCell,
    cmp::{max, min},
    fmt::Write,
};
use display::{Display, DisplayPeripherals, DisplayTrait};
use drivers::cst816x::{CST816x, Event};
use embedded_graphics::prelude::Point;
use embedded_graphics::primitives::Rectangle;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc::psram_allocator;
use esp_backtrace as _;
use esp_hal::gpio::{InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::main;
use esp_hal::time::Instant;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, gpio::Input, i2c::master::I2c};
use log::info;
use micromath::{vector::F32x3, Quaternion};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

mod config;
mod display;

// Cube and projection constants
const FOV: f32 = 200.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const ROTATION_SPEED: f32 = 0.03;
const FPS_FONT_WIDTH: i32 = 10;
const FPS_FONT_HEIGHT: i32 = 20;

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_240MHz));

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 73744);

    let timer_group0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timer_group0.timer0);

    let i2c = I2c::new(peripherals.I2C0, esp_hal::i2c::master::Config::default())
        .unwrap()
        .with_sda(peripherals.GPIO3)
        .with_scl(peripherals.GPIO2);

    let i2c_ref_cell = RefCell::new(i2c);

    let display_peripherals = DisplayPeripherals {
        sck: peripherals.GPIO47,
        mosi: peripherals.GPIO18,
        cs: peripherals.GPIO6,
        dc: peripherals.GPIO7,
        rst: peripherals.GPIO17,
        spi: peripherals.SPI2,
        dma: peripherals.DMA_CH0,
    };

    psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    // Enable the power management IC by setting the PMICEN pin high
    let mut pmicen = Output::new(peripherals.GPIO38, Level::Low, OutputConfig::default());
    pmicen.set_high();
    info!("PMICEN set high");

    let mut display = Display::new(display_peripherals).expect("Display init failed");

    info!("Display initialized!");

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

    // initalize touchpad
    let touch_int = peripherals.GPIO21;
    let touch_int = Input::new(touch_int, InputConfig::default().with_pull(Pull::None));

    let mut touchpad = CST816x::new(RefCellDevice::new(&i2c_ref_cell), touch_int);

    let mut initial_touch_x: i32 = 0;
    let mut initial_touch_y: i32 = 0;
    let mut text_x: u16 = 0;
    let mut prev_cube_rect: Option<Rectangle> = None;
    let mut prev_text_rect: Option<Rectangle> = None;

    // Pre-calculate the constant automatic rotation quaternion
    let q_auto = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), ROTATION_SPEED);

    loop {
        // FPS calculation and display
        let current_time = Instant::now().duration_since_epoch().as_millis();

        if let Ok(touch_event) = touchpad.read_touch() {
            match touch_event.event {
                Event::Down => {
                    initial_touch_x = touch_event.x as i32;
                    initial_touch_y = touch_event.y as i32;
                    //println!("Touch Down at ({}, {})", initial_touch_x, initial_touch_y);
                }
                Event::Up => {
                    // Touch Lift
                    //println!("Touch Lift at ({}, {})", touch_event.x, touch_event.y);

                    // Calculate the difference between initial and final touch positions
                    let delta_x = touch_event.x as i32 - initial_touch_x;
                    let delta_y = touch_event.y as i32 - initial_touch_y;

                    //println!("Touch Delta: ({}, {})", delta_x, delta_y);

                    // Define rotation sensitivity
                    const ROTATION_SENSITIVITY: f32 = 0.0005;

                    // Calculate rotation angles based on touch movement
                    let angle_y = (delta_x as f32) * ROTATION_SENSITIVITY; // Rotate around Y-axis
                    let angle_x = (delta_y as f32) * ROTATION_SENSITIVITY; // Rotate around X-axis

                    // Create quaternions for the rotations
                    let qx = Quaternion::axis_angle(F32x3::from((1.0, 0.0, 0.0)), angle_x);
                    let qy = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), angle_y);

                    // Update the overall rotation
                    rotation = qy * qx * rotation;

                    //println!("Applied rotation: {:?}", &rotation);
                }
                _ => {
                    //ingore other touch events
                }
            }
        }

        // Apply pre-calculated automatic rotation
        rotation = q_auto * rotation;

        // Transform and project vertices
        let mut transformed_vertices = [(0i32, 0i32); 8]; // Fixed size array

        for (i, &v) in cube_vertices.iter().enumerate() {
            let rotated = rotation.rotate(v);
            let x = rotated.x;
            let y = rotated.y;
            let z = rotated.z + PROJECTION_DISTANCE;

            // Perspective projection with check for division by near-zero z
            let projected_point = if z.abs() > 0.01 {
                // Avoid division if z is too close to the camera plane
                let inv_z = 1.0 / z; // Calculate inverse z once
                let px = (x * FOV * inv_z) as i32 + half_width;
                let py = (y * FOV * inv_z) as i32 + half_height;
                (px, py)
            } else {
                // Point is too close or behind the camera, mark as invalid
                (i32::MAX, i32::MAX) // Use MAX as an indicator for clipping/invalid point
            };
            transformed_vertices[i] = projected_point;
        }

        // Compute current cube bounds for partial updates
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for &(x, y) in &transformed_vertices {
            if x != i32::MAX && y != i32::MAX {
                min_x = min(min_x, x);
                min_y = min(min_y, y);
                max_x = max(max_x, x);
                max_y = max(max_y, y);
            }
        }

        let cube_rect = if min_x <= max_x && min_y <= max_y {
            let margin = 2;
            let clipped_min_x = max(min_x - margin, 0);
            let clipped_min_y = max(min_y - margin, 0);
            let clipped_max_x = min(max_x + margin, (DISPLAY_WIDTH - 1) as i32);
            let clipped_max_y = min(max_y + margin, (DISPLAY_HEIGHT - 1) as i32);
            Some(Rectangle::with_corners(
                Point::new(clipped_min_x, clipped_min_y),
                Point::new(clipped_max_x, clipped_max_y),
            ))
        } else {
            None
        };

        if let Some(prev_rect) = prev_cube_rect {
            display.mark_region_dirty(prev_rect);
        }

        // Draw edges
        for &(start, end) in &edges {
            let p1 = transformed_vertices[start];
            let p2 = transformed_vertices[end];

            // Only draw the line if both points are valid (not projected off-screen)
            if p1.0 != i32::MAX && p2.0 != i32::MAX {
                let begin = Point::new(p1.0, p1.1);
                let end = Point::new(p2.0, p2.1);
                display.draw_line(begin, end).expect("Draw line failed");
            }
        }

        prev_cube_rect = cube_rect;

        let ms_per_frame = current_time - last_time;
        if ms_per_frame > 0 {
            let mut text = String::with_capacity(16);
            write!(text, "FPS: {}", 1000 / ms_per_frame).expect("Write failed");

            if let Some(prev_rect) = prev_text_rect {
                display.mark_region_dirty(prev_rect);
            }

            let text_width = (text.len() as i32) * FPS_FONT_WIDTH;
            let text_height = FPS_FONT_HEIGHT;
            let current_text_rect = Rectangle::new(
                Point::new(text_x as i32, 0),
                embedded_graphics::geometry::Size::new(text_width as u32, text_height as u32),
            );

            display
                .write(&text, Point::new(text_x as i32, 0))
                .expect("Write text failed");

            prev_text_rect = Some(current_text_rect);

            // Update text position for scrolling effect using modulo
            text_x = (text_x + 1) % DISPLAY_WIDTH;
        }

        last_time = current_time;

        display
            .update_with_buffer()
            .expect("Update with buffer failed");
    }
}
