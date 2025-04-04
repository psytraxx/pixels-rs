#![no_std]
#![no_main]
#![feature(generic_arg_infer)]

use config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use core::{cell::RefCell, fmt::Write};
use defmt::info;
use display::{Display, DisplayPeripherals, DisplayTrait};
use drivers::cst816x::{CST816x, Event};
use embedded_graphics::prelude::Point;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc::{heap_allocator, psram_allocator};
use esp_backtrace as _;
use esp_hal::main;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, gpio::Input, i2c::master::I2c, time};
use heapless::String;
use micromath::{vector::F32x3, Quaternion};
use {defmt_rtt as _, esp_backtrace as _};

extern crate alloc;

mod config;
mod display;

// Cube and projection constants
const FOV: f32 = 200.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const ROTATION_SPEED: f32 = 0.03;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init({
        let mut config = esp_hal::Config::default();
        config.cpu_clock = CpuClock::_240MHz;
        config
    });

    let mut rtc = Rtc::new(peripherals.LPWR);
    rtc.rwdt.disable();

    let mut timer_group0 = TimerGroup::new(peripherals.TIMG0);
    timer_group0.wdt.disable();
    let mut timer_group1 = TimerGroup::new(peripherals.TIMG1);
    timer_group1.wdt.disable();

    heap_allocator!(size: 72 * 1024);

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
    let mut buffer = [0_u8; 512];
    let mut display = Display::new(display_peripherals, &mut buffer).expect("Display init failed");

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
    let touch_int = Input::new(touch_int, esp_hal::gpio::Pull::Up);

    let mut touchpad = CST816x::new(RefCellDevice::new(&i2c_ref_cell), touch_int);

    let mut initial_touch_x: i32 = 0;
    let mut initial_touch_y: i32 = 0;
    let mut text_x: u16 = 0;

    loop {
        // FPS calculation and display
        let current_time = time::now().duration_since_epoch().to_millis();

        if let Ok(touch_event) = touchpad.read_touch() {
            match touch_event.event {
                Event::Down => {
                    initial_touch_x = touch_event.x as i32;
                    initial_touch_y = touch_event.y as i32;
                    info!("Touch Down at ({}, {})", initial_touch_x, initial_touch_y);
                }
                Event::Up => {
                    // Touch Lift
                    info!("Touch Lift at ({}, {})", touch_event.x, touch_event.y);

                    // Calculate the difference between initial and final touch positions
                    let delta_x = touch_event.x as i32 - initial_touch_x;
                    let delta_y = touch_event.y as i32 - initial_touch_y;

                    //info!("Touch Delta: ({}, {})", delta_x, delta_y);

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

                    //info!("Applied rotation: {}", defmt::Debug2Format(&rotation));
                }
                _ => {
                    //ingore other touch events
                }
            }
        }

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
        if ms_per_frame > 0 {
            let mut text = String::<16>::new();
            write!(text, "FPS: {}", 1000 / ms_per_frame).expect("Write failed");

            display
                .write(&text, Point::new(text_x as i32, 0))
                .expect("Write text failed");
            text_x += 1;
            if text_x > DISPLAY_WIDTH {
                text_x = 0;
            }
        }

        last_time = current_time;

        display
            .update_with_buffer()
            .expect("Update with buffer failed");
    }
}
