# Pixels-rs

`pixels-rs` is a high-performance Rust project that demonstrates real-time 3D graphics with particle effects on an ESP32-S3 microcontroller, achieving **50-60 FPS** on an RM67162 AMOLED display with touch input capabilities.

## Features

### 3D Graphics
- **Interactive 3D wireframe cube** with dual control:
  - Automatic quaternion-based rotation
  - Touch-based gesture control (drag to rotate)
- **3D particle system** with 100 particles:
  - Emission from cube center
  - Physics-based bouncing within cube boundaries
  - Random vibrant colors (red, green, blue, yellow, cyan, magenta)
  - Full 3D rotation synchronized with cube

### Performance Optimizations
- **Tile-based rendering** (32×32 pixel tiles, 136 total)
- **Horizontal tile batching** reduces DMA transfers by ~80%
- **Double-buffered rendering** with selective clearing
- **Dirty tile tracking** updates only changed regions
- **3.5× FPS improvement** (from 17 FPS baseline to 50-60 FPS)
- Hardware-accelerated DMA transfers at 80 MHz SPI
- Real-time FPS counter

## Project Structure

- `main.rs`: Main application entry point with 3D rendering pipeline, particle system, and touch input
- `display.rs`: Advanced display abstraction with tile-based rendering and DMA optimization
- `config.rs`: Configuration constants for display dimensions

### Rendering Pipeline

1. **Particle Physics**: Emit, update positions, and constrain particles to cube boundaries
2. **3D Transformation**: Apply quaternion rotation to cube vertices and particles
3. **Perspective Projection**: Project 3D coordinates to 2D screen space
4. **Tile Marking**: Track which 32×32 tiles are affected by drawing operations
5. **Rendering**: Draw wireframe cube edges and colored particle points
6. **Tile Batching**: Merge adjacent dirty tiles horizontally for efficient DMA transfers
7. **Display Update**: Send only changed tiles (~85% reduction in data transfer)

## Hardware Requirements

- **ESP32-S3 microcontroller** (running at 240 MHz)
- **RM67162 AMOLED display** (536×240 pixels, 80 MHz SPI)
- **CST816S touch controller** (I2C interface)
- **PSRAM** for framebuffers (~512 KB for double buffering)
- **Pin Configuration**:
  - SPI (Display): GPIO 47 (SCK), 18 (MOSI), 6 (CS), 7 (DC), 17 (RST)
  - I2C (Touch): GPIO 2 (SCL), 3 (SDA)
  - Touch Interrupt: GPIO 21
  - Power Management: GPIO 38 (PMICEN)

## Dependencies

- **`esp-hal` (1.0.0-rc.1)**: ESP32-S3 hardware abstraction layer
- **`esp-rtos` (0.1.1)**: RTOS integration with embassy async runtime
- **`mipidsi` (git)**: RM67162 MIPI DSI display driver
- **`drivers` (git v0.9.0)**: CST816x touch controller driver
- **`embedded-graphics` (0.8.1)**: 2D graphics primitives and drawing
- **`micromath` (2.1.0)**: no_std quaternion and vector math
- **`embedded-hal-bus`**: Hardware abstraction for I2C/SPI communication

## Getting Started

1. **Set up ESP32 Rust environment**:
    ```sh
    . ~/export-esp.sh
    ```

2. **Build for development**:
    ```sh
    cargo build
    ```

3. **Build optimized release**:
    ```sh
    cargo build --release
    ```

Note: Debug builds use optimization level 's' for reasonable embedded performance.

## Usage

### Controls

1. **Automatic Rotation**: Cube continuously rotates around the Y-axis
2. **Touch Gesture Control**:
   - Touch and drag across the screen
   - Delta movement controls rotation on X and Y axes
   - Rotation is proportional to drag distance (sensitivity: 0.0005 rad/pixel)

### Particle System

- **100 particles** continuously emit from the cube's center
- **3 particles spawn per frame** with random velocities
- Particles **bounce** when hitting cube boundaries (±1.0 in each axis)
- Each particle has a **random color** from a vibrant palette
- Particles **rotate with the cube** for full 3D effect

## Configuration

### Display Settings (`config.rs`)
```rust
pub const DISPLAY_WIDTH: u16 = 536;
pub const DISPLAY_HEIGHT: u16 = 240;
```

### Rendering Constants (`display.rs`)
```rust
const TILE_SIZE: u16 = 32;        // 32×32 pixel tiles
const TILES_X: usize = 17;        // Tiles horizontally
const TILES_Y: usize = 8;         // Tiles vertically
const TOTAL_TILES: usize = 136;   // Total tile count
```

### 3D Rendering Parameters (`main.rs`)
```rust
const FOV: f32 = 200.0;                    // Field of view
const PROJECTION_DISTANCE: f32 = 4.0;      // Camera distance
const ROTATION_SPEED: f32 = 0.03;          // Auto-rotation speed
const ROTATION_SENSITIVITY: f32 = 0.0005;  // Touch sensitivity

// Particle system
const MAX_PARTICLES: usize = 100;     // Particle pool size
const EMISSION_RATE: usize = 3;       // Particles per frame
const PARTICLE_SPEED: f32 = 0.02;     // Initial velocity
```

## Performance Metrics

- **Baseline FPS**: ~17 FPS (full screen updates)
- **Final FPS**: 50-60 FPS (tile-based rendering)
- **Improvement**: 3.5× faster
- **Data Transfer Reduction**: ~85% (257 KB → 40-80 KB per frame)
- **DMA Transfers**: Reduced from 20-40 to 4-8 batched transfers per frame

## Technical Highlights

### Memory Management
- **DRAM heap**: 73,744 bytes for general allocations
- **PSRAM**: ~512 KB for double-buffered framebuffers (2 × 536 × 240 × 2 bytes)
- Stack-allocated particle array (no heap allocations during rendering)

### Rendering Optimizations
- **Selective clearing**: Only clears tiles that were dirty 2 frames ago
- **Dirty tracking**: Tracks current and previous frame tiles for complete updates
- **Horizontal batching**: Merges adjacent dirty tiles into single DMA transfers
- **Efficient iteration**: Uses flat_map for optimal pixel iteration

### 3D Graphics Techniques
- **Quaternion rotation**: Avoids gimbal lock, enables smooth interpolation
- **Perspective projection**: Realistic depth with z-clipping (z > 0.01)
- **Pre-calculated quaternions**: Automatic rotation uses pre-computed values

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.


