#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::{cell::RefCell, fmt::Write};
use display::{Display, DisplayPeripherals, DisplayTrait};
use drivers::cst816x::{CST816x, Event};
use embedded_graphics::prelude::Point;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc::psram_allocator;
use esp_backtrace as _;
use esp_hal::gpio::InputConfig;
use esp_hal::main;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::time::Instant;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, gpio::Input, i2c::master::I2c};
use heapless::String;
use micromath::{vector::F32x3, Quaternion};

use crate::display::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

mod display;

// Cube and projection constants
const FOV: f32 = 200.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const ROTATION_SPEED: f32 = 0.03;

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::_240MHz));

    let mut rtc = Rtc::new(peripherals.LPWR);
    rtc.rwdt.disable();

    let mut timer_group0 = TimerGroup::new(peripherals.TIMG0);
    timer_group0.wdt.disable();
    let mut timer_group1 = TimerGroup::new(peripherals.TIMG1);
    timer_group1.wdt.disable();

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
        pmicen: peripherals.GPIO38,
        spi: peripherals.SPI2,
        dma: peripherals.DMA_CH0,
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

    // initialize touchpad
    let touch_int = peripherals.GPIO21;
    let touch_int = Input::new(
        touch_int,
        InputConfig::default().with_pull(esp_hal::gpio::Pull::Up),
    );

    let mut touchpad = CST816x::new(RefCellDevice::new(&i2c_ref_cell), touch_int);

    let mut initial_touch_x: i32 = 0;
    let mut initial_touch_y: i32 = 0;
    let mut text_x: u16 = 0;

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

        let ms_per_frame = current_time - last_time;
        if ms_per_frame > 0 {
            let mut text = String::<16>::new();
            write!(text, "FPS: {}", 1000 / ms_per_frame).expect("Write failed");

            display
                .write(&text, Point::new(text_x as i32, 0))
                .expect("Write text failed");

            // Update text position for scrolling effect using modulo
            text_x = (text_x + 1) % DISPLAY_WIDTH as u16;
        }

        last_time = current_time;

        display
            .update_with_buffer()
            .expect("Update with buffer failed");
    }
}
