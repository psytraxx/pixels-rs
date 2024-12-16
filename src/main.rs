use micromath::F32;
#[allow(unused_imports)]
use micromath::{
    F32Ext, Quaternion,
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

    // Rotation angles
    let mut angle_x: F32 = F32(0.0);
    let mut angle_y: F32 = F32(0.0);

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

    let mut last_time = Instant::now();
    let mut last_fps_update = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let frame_start = Instant::now();

        // Clear the framebuffer
        buffer.fill(0);

        // Increment rotation angles
        angle_x += 0.02;
        angle_y += 0.03;

        // Transform and project vertices
        let mut projected_vertices: [I32x2; 8] = [I32x2 { x: 0, y: 0 }; 8];
        for (i, &vertex) in cube_vertices.iter().enumerate() {
            let rotated_vertex = rotate_vertex(vertex, angle_x, angle_y);
            projected_vertices[i] = project_vertex(rotated_vertex);
        }

        // Draw edges
        for &(start, end) in &edges {
            draw_line(
                &mut buffer,
                &projected_vertices[start],
                &projected_vertices[end],
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
            std::thread::sleep(FRAME_TIME - frame_time);
        }
    }
}

/// Rotates a vertex around the X and Y axes using direct matrix multiplication
#[inline]
fn rotate_vertex(vertex: F32x3, angle_x: F32, angle_y: F32) -> F32x3 {
    // First rotate around X
    let (sin_x, cos_x) = (angle_x.sin(), angle_x.cos());
    let y1 = vertex.y * cos_x - vertex.z * sin_x;
    let z1 = vertex.y * sin_x + vertex.z * cos_x;

    // Then rotate around Y
    let (sin_y, cos_y) = (angle_y.sin(), angle_y.cos());
    let x2 = vertex.x * cos_y + z1 * sin_y;
    let z2 = -vertex.x * sin_y + z1 * cos_y;

    F32x3::from((x2.0, y1.0, z2.0))
}

#[inline]
fn project_vertex(vertex: F32x3) -> I32x2 {
    let z = vertex.z + PROJECTION_DISTANCE;
    let scale = FOV / z;
    let x = (vertex.x * scale + (WIDTH as f32) * 0.5) as i32;
    let y = (vertex.y * scale + (HEIGHT as f32) * 0.5) as i32;
    I32x2 { x, y }
}

#[inline]
fn draw_line(buffer: &mut [u32], start: &I32x2, end: &I32x2) {
    let (mut x0, mut y0) = (start.x, start.y);
    let (x1, y1) = (end.x, end.y);

    // Early bounds check
    if (x0 < 0 && x1 < 0)
        || (x0 >= WIDTH as i32 && x1 >= WIDTH as i32)
        || (y0 < 0 && y1 < 0)
        || (y0 >= HEIGHT as i32 && y1 >= HEIGHT as i32)
    {
        return;
    }

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    while x0 >= 0 && x0 < WIDTH as i32 && y0 >= 0 && y0 < HEIGHT as i32 {
        buffer[y0 as usize * WIDTH + x0 as usize] = 0xFFFFFF;

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
