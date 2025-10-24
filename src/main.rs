#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};
use core::cell::RefCell;
use display::{Display, DisplayPeripherals, DisplayTrait};
use drivers::cst816x::{CST816x, Event};
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::Point;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc::psram_allocator;
use esp_backtrace as _;
use esp_hal::gpio::{InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::main;
use esp_hal::time::Instant;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{clock::CpuClock, gpio::Input, i2c::master::I2c};
use log::info;
use micromath::{vector::F32x3, F32Ext, Quaternion};

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
    let cube_edges = [
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

    // Particle system
    const MAX_PARTICLES: usize = 200;
    const EMISSION_RATE: usize = 3; // Particles per frame
    const PARTICLE_SPEED: f32 = 0.02;

    #[derive(Copy, Clone)]
    struct Particle {
        pos: F32x3,
        vel: F32x3,
        active: bool,
        color: Rgb565,
    }

    let mut particles = [Particle {
        pos: F32x3::from((0.0, 0.0, 0.0)),
        vel: F32x3::from((0.0, 0.0, 0.0)),
        active: false,
        color: Rgb565::WHITE,
    }; MAX_PARTICLES];

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

    // Pre-allocated buffer for FPS text to avoid allocations every frame
    let mut fps_buffer = [0u8; 16];

    // Pre-calculate the constant automatic rotation quaternion
    let q_auto = Quaternion::axis_angle(F32x3::from((0.0, 1.0, 0.0)), ROTATION_SPEED);

    loop {
        // Clear buffer at start of frame (optimization: clear before rendering instead of after swap)
        display.clear_buffer();

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

        // Emit new particles from center
        for _ in 0..EMISSION_RATE {
            // Find an inactive particle slot
            if let Some(p) = particles.iter_mut().find(|p| !p.active) {
                // Simple pseudo-random using time
                let t = current_time as f32;
                let rand_x = ((t * 0.123) % 1.0) * 2.0 - 1.0;
                let rand_y = ((t * 0.456) % 1.0) * 2.0 - 1.0;
                let rand_z = ((t * 0.789) % 1.0) * 2.0 - 1.0;

                // Normalize direction and apply speed
                let len = (rand_x * rand_x + rand_y * rand_y + rand_z * rand_z).sqrt();
                let vel = if len > 0.01 {
                    F32x3::from((
                        rand_x / len * PARTICLE_SPEED,
                        rand_y / len * PARTICLE_SPEED,
                        rand_z / len * PARTICLE_SPEED,
                    ))
                } else {
                    F32x3::from((PARTICLE_SPEED, 0.0, 0.0))
                };

                // Generate random color
                let color_seed = (t * 0.321) % 1.0;
                let color = if color_seed < 0.166 {
                    Rgb565::RED
                } else if color_seed < 0.333 {
                    Rgb565::GREEN
                } else if color_seed < 0.5 {
                    Rgb565::BLUE
                } else if color_seed < 0.666 {
                    Rgb565::YELLOW
                } else if color_seed < 0.833 {
                    Rgb565::CYAN
                } else {
                    Rgb565::MAGENTA
                };

                p.pos = F32x3::from((0.0, 0.0, 0.0)); // Emit from center
                p.vel = vel;
                p.active = true;
                p.color = color;
            }
        }

        // Update particles
        for p in particles.iter_mut() {
            if p.active {
                // Update position
                p.pos.x += p.vel.x;
                p.pos.y += p.vel.y;
                p.pos.z += p.vel.z;

                // Constrain to cube boundaries and bounce
                if p.pos.x > 1.0 || p.pos.x < -1.0 {
                    p.vel.x = -p.vel.x;
                    p.pos.x = p.pos.x.clamp(-1.0, 1.0);
                }
                if p.pos.y > 1.0 || p.pos.y < -1.0 {
                    p.vel.y = -p.vel.y;
                    p.pos.y = p.pos.y.clamp(-1.0, 1.0);
                }
                if p.pos.z > 1.0 || p.pos.z < -1.0 {
                    p.vel.z = -p.vel.z;
                    p.pos.z = p.pos.z.clamp(-1.0, 1.0);
                }
            }
        }

        // Render CUBE at center
        let cube_offset_x = 0; // Centered
        let mut cube_transformed = [(0i32, 0i32); 8];

        for (i, &v) in cube_vertices.iter().enumerate() {
            let rotated = rotation.rotate(v);
            let x = rotated.x;
            let y = rotated.y;
            let z = rotated.z + PROJECTION_DISTANCE;

            let projected_point = if z.abs() > 0.01 {
                let inv_z = 1.0 / z;
                let px = (x * FOV * inv_z) as i32 + half_width + cube_offset_x;
                let py = (y * FOV * inv_z) as i32 + half_height;
                (px, py)
            } else {
                (i32::MAX, i32::MAX)
            };
            cube_transformed[i] = projected_point;
        }

        // Draw cube edges
        for &(start, end) in &cube_edges {
            let p1 = cube_transformed[start];
            let p2 = cube_transformed[end];

            if p1.0 != i32::MAX && p2.0 != i32::MAX {
                let begin = Point::new(p1.0, p1.1);
                let end = Point::new(p2.0, p2.1);
                display.draw_line(begin, end).expect("Draw line failed");
            }
        }

        // Render particles
        for p in particles.iter() {
            if p.active {
                // Apply rotation to particle position
                let rotated = rotation.rotate(p.pos);
                let x = rotated.x;
                let y = rotated.y;
                let z = rotated.z + PROJECTION_DISTANCE;

                // Project to screen
                if z.abs() > 0.01 {
                    let inv_z = 1.0 / z;
                    let px = (x * FOV * inv_z) as i32 + half_width;
                    let py = (y * FOV * inv_z) as i32 + half_height;

                    // Draw particle as colored point
                    if px >= 1
                        && px < DISPLAY_WIDTH as i32 - 1
                        && py >= 1
                        && py < DISPLAY_HEIGHT as i32 - 1
                    {
                        display
                            .draw_colored_point(Point::new(px, py), p.color)
                            .expect("Draw particle failed");
                    }
                }
            }
        }

        let ms_per_frame = current_time - last_time;
        if ms_per_frame > 0 {
            // Use pre-allocated buffer and format FPS text without heap allocation
            let fps = 1000 / ms_per_frame;
            let mut cursor = 0;
            let prefix = b"FPS: ";
            fps_buffer[..prefix.len()].copy_from_slice(prefix);
            cursor += prefix.len();

            // Format the number manually to avoid allocation
            let mut num = fps;
            let mut digits = [0u8; 10];
            let mut digit_count = 0;
            if num == 0 {
                digits[0] = b'0';
                digit_count = 1;
            } else {
                while num > 0 {
                    digits[digit_count] = b'0' + (num % 10) as u8;
                    num /= 10;
                    digit_count += 1;
                }
            }
            // Reverse digits into buffer
            for i in 0..digit_count {
                fps_buffer[cursor] = digits[digit_count - 1 - i];
                cursor += 1;
            }

            let text = core::str::from_utf8(&fps_buffer[..cursor]).unwrap();
            display
                .write(text, Point::new(0, 0))
                .expect("Write text failed");
        }

        last_time = current_time;

        display
            .update_with_buffer()
            .expect("Update with buffer failed");
    }
}
