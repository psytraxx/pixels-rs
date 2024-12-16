use line_drawing::Bresenham;
use micromath::{
    Quaternion,
    vector::{F32x3, I32x2},
};
use minifb::{Key, Window, WindowOptions};
use std::time::{Duration, Instant};

const WIDTH: usize = 320;
const HEIGHT: usize = 170;

// Cube and projection constants
const FOV: f32 = 150.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const TARGET_FPS: u64 = 30;
const FRAME_TIME: Duration = Duration::from_millis(1000 / TARGET_FPS); // 1/60 second
const FPS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const ROTATION_SPEED: f32 = 0.03;

fn main() {
    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT]; // Initialize the framebuffer
    let mut window = Window::new(
        "Pixel Drawing with minifb",
        WIDTH,
        HEIGHT,
        WindowOptions::default(),
    )
    .expect("Unable to create window");

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
    let mut last_time = Instant::now();
    let mut last_fps_update = Instant::now();
    let half_width = (WIDTH / 2) as i32;
    let half_height = (HEIGHT / 2) as i32;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let frame_start = Instant::now();

        // Clear the framebuffer
        buffer.fill(0);

        // Update rotation based on keyboard input
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
        }

        // Transform and project vertices
        let transformed_vertices: Vec<I32x2> = cube_vertices
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

        // FPS calculation and display
        let current_time = Instant::now();
        if current_time.duration_since(last_fps_update) >= FPS_UPDATE_INTERVAL {
            let fps = 1.0 / current_time.duration_since(last_time).as_secs_f32();
            println!("FPS: {:.2}", fps);
            last_fps_update = current_time;
        }
        last_time = current_time;

        window.update_with_buffer(&buffer, WIDTH, HEIGHT).unwrap();

        let frame_time = frame_start.elapsed();
        if frame_time < FRAME_TIME {
            //std::thread::sleep(FRAME_TIME - frame_time);
        }
    }
}

#[inline]
fn draw_line(buffer: &mut [u32], start: &I32x2, end: &I32x2) {
    for (x, y) in Bresenham::new((start.x, start.y), (end.x, end.y)) {
        if x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
            buffer[y as usize * WIDTH + x as usize] = 0xFFFFFF;
        }
    }
}
