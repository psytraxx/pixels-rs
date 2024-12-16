use micromath::vector::{Vector2d, Vector3d};
#[allow(unused_imports)]
use micromath::{F32, F32Ext};
use minifb::{Key, Window, WindowOptions};
use std::time::{Duration, Instant};

const WIDTH: usize = 320;
const HEIGHT: usize = 170;

// Cube and projection constants
const FOV: f32 = 150.0; // Field of View
const PROJECTION_DISTANCE: f32 = 4.0;
const TARGET_FPS: u64 = 60;
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
    let cube_vertices: [Vector3d<F32>; 8] = [
        Vector3d::from((F32(-1.0), F32(-1.0), F32(-1.0))),
        Vector3d::from((F32(1.0), F32(-1.0), F32(-1.0))),
        Vector3d::from((F32(1.0), F32(1.0), F32(-1.0))),
        Vector3d::from((F32(-1.0), F32(1.0), F32(-1.0))),
        Vector3d::from((F32(-1.0), F32(-1.0), F32(1.0))),
        Vector3d::from((F32(1.0), F32(-1.0), F32(1.0))),
        Vector3d::from((F32(1.0), F32(1.0), F32(1.0))),
        Vector3d::from((F32(-1.0), F32(1.0), F32(1.0))),
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
        let mut projected_vertices: [Vector2d<i32>; 8] = [Vector2d { x: 0, y: 0 }; 8];
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

/// Rotates a vertex around the X and Y axes.
fn rotate_vertex(vertex: Vector3d<F32>, angle_x: F32, angle_y: F32) -> Vector3d<F32> {
    // Rotation around the X axis
    let cos_x = angle_x.cos();
    let sin_x = angle_x.sin();
    let rotated_x = Vector3d {
        x: vertex.x,
        y: vertex.y * cos_x - vertex.z * sin_x,
        z: vertex.y * sin_x + vertex.z * cos_x,
    };

    // Rotation around the Y axis
    let cos_y = angle_y.cos();
    let sin_y = angle_y.sin();

    Vector3d {
        x: rotated_x.x * cos_y + rotated_x.z * sin_y,
        y: rotated_x.y,
        z: -rotated_x.x * sin_y + rotated_x.z * cos_y,
    }
}

#[inline]
/// Projects a 3D vertex onto the 2D screen using perspective projection.
fn project_vertex(vertex: Vector3d<F32>) -> Vector2d<i32> {
    let z = vertex.z + PROJECTION_DISTANCE;
    let scale = FOV / z;
    Vector2d {
        x: (vertex.x * scale + WIDTH as f32 / 2.0).0 as i32,
        y: (vertex.y * scale + HEIGHT as f32 / 2.0).0 as i32,
    }
}

#[inline]
/// Draws a line between two 2D points on the screen.
fn draw_line(buffer: &mut [u32], start: &Vector2d<i32>, end: &Vector2d<i32>) {
    // Early bounds check
    if !is_line_visible(start, end) {
        return;
    }

    let x0: i32 = start.x;
    let y0: i32 = start.y;
    let x1: i32 = end.x;
    let y1: i32 = end.y;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x: i32 = x0;
    let mut y: i32 = y0;

    loop {
        if x >= 0 && x < WIDTH as i32 && y >= 0 && y < HEIGHT as i32 {
            let idx = y as usize * WIDTH + x as usize;
            buffer[idx] = 0xFFFFFF; // White color
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[inline]
fn is_line_visible(start: &Vector2d<i32>, end: &Vector2d<i32>) -> bool {
    let width = WIDTH as i32;
    let height = HEIGHT as i32;

    // Check if both points are completely outside the same boundary
    if (start.x < 0 && end.x < 0)
        || (start.x >= width && end.x >= width)
        || (start.y < 0 && end.y < 0)
        || (start.y >= height && end.y >= height)
    {
        return false;
    }
    true
}
