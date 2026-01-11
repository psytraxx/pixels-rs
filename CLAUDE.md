# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`pixels-rs` is an embedded Rust application for ESP32-S3 microcontrollers that renders interactive 3D graphics with a particle system on an RM67162 AMOLED display (536x240) with CST816S touch controller support. The project achieves 50-60 FPS through tile-based rendering optimizations, quaternion-based 3D rotation, and double buffering.

**Target Hardware:**
- ESP32-S3 microcontroller (running at 240MHz)
- RM67162 AMOLED display (536x240 pixels, 80 MHz SPI)
- CST816S touch controller (I2C)
- PSRAM (octal mode, ~512KB for framebuffers)
- Pin Configuration:
  - SPI (Display): GPIO 47 (SCK), 18 (MOSI), 6 (CS), 7 (DC), 17 (RST)
  - I2C (Touch): GPIO 2 (SCL), 3 (SDA)
  - Touch Interrupt: GPIO 21
  - Power Management: GPIO 38 (PMICEN)

## Build Commands

**Setup environment:**
```sh
. ~/export-esp.sh
```

**Build for development:**
```sh
cargo build
```

**Build optimized release:**
```sh
cargo build --release
```

**Flash and monitor:**
The default runner in `.cargo/config.toml` automatically flashes and monitors when you run:
```sh
cargo run --release
```

This executes: `espflash flash -c esp32s3 -s 16mb -m dio -f 80mhz --no-skip --monitor`

**Notes:**
- Debug builds use optimization level 's' for reasonable embedded performance
- Release builds use LTO 'fat' and optimization level 's' for size optimization
- The build requires `build-std = ["alloc", "core"]` for no_std target

## Architecture

### Core Module Structure

- **main.rs**: Application entry point with 3D rendering pipeline and particle system
  - Hardware initialization (SPI, I2C, PSRAM, display, touch controller)
  - 3D wireframe cube rendering with quaternion-based rotation
  - Particle system with 200 particles, physics-based bouncing, and random colors
  - Touch gesture processing (drag-to-rotate)
  - FPS calculation and display
  - Infinite render loop with tile-aware double buffering

- **display.rs**: Advanced display abstraction with tile-based rendering
  - Wraps mipidsi RM67162 driver with custom SPI interface
  - Implements `DisplayTrait` for text, lines, and colored points
  - Tile-based rendering system (32x32 pixel tiles, 136 total tiles)
  - Dirty tile tracking with horizontal batching for DMA optimization
  - Double buffering with selective clearing
  - DMA transfers at 80 MHz SPI

- **config.rs**: Display dimension constants
  - `DISPLAY_WIDTH: u16 = 536`
  - `DISPLAY_HEIGHT: u16 = 240`

### Key Architectural Patterns

**Tile-Based Rendering:**
The display is divided into 32x32 pixel tiles (17 tiles wide × 8 tiles high = 136 total). The system tracks which tiles are "dirty" (need updating) each frame using `TileTracker`:
- `current_tiles`: Tiles drawn to in the current frame
- `prev_tiles`: Tiles that were dirty 2 frames ago (used for selective clearing)

On `update_with_buffer()`, the system batches horizontally adjacent dirty tiles to minimize DMA transfers (~80% reduction). Only changed regions are sent to the display, reducing data transfer from ~257KB (full screen) to 40-80KB per frame.

**Double Buffering with Selective Clearing:**
The display maintains two framebuffers allocated in PSRAM:
1. `back_buffer`: All drawing operations write here
2. `front_buffer`: Contains the previous frame's data, sent to display

The render cycle:
1. Clear only tiles that were dirty 2 frames ago in back_buffer
2. Draw current frame to back_buffer, marking tiles as dirty
3. Swap buffers (back becomes front)
4. Send only dirty tiles from front_buffer to display via batched DMA
5. Save current dirty tiles for clearing in 2 frames

This approach achieves 50-60 FPS (3.5× improvement over full-screen updates).

**Memory Management:**
- DRAM heap: 73,744 bytes for general allocations
- PSRAM (octal mode): ~512KB for framebuffers (2 × 536 × 240 × 2 bytes)
- DMA buffers: 32,000-byte static buffers (rx/tx) for SPI transfers
- Display staging buffer: 512 bytes for command/data
- Particle array: Stack-allocated (200 × ~32 bytes)

**Rendering Pipeline:**
1. **Clear**: Selectively clear tiles dirty 2 frames ago in back_buffer
2. **Input**: Process touch events (touch down captures position, touch up calculates delta)
3. **Rotation**: Apply pre-calculated automatic rotation quaternion + touch rotation
4. **Particle Physics**: Emit 3 particles/frame, update positions, bounce at cube boundaries (±1.0)
5. **3D Transform**: Apply quaternion rotation to cube vertices and active particles
6. **Projection**: Perspective projection with z-clipping (z > 0.01)
7. **Rendering**: Draw cube edges (white, 2px stroke) and particles (colored 3x3 rectangles)
8. **Text**: Render FPS counter (top-left, no heap allocations)
9. **Display**: Swap buffers, batch dirty tiles horizontally, DMA transfer to screen

**3D Graphics:**
- Quaternion rotation: Avoids gimbal lock, enables smooth composition
- Pre-calculated automatic rotation: `q_auto = Quaternion::axis_angle(Y_AXIS, ROTATION_SPEED)`
- Perspective projection: `screen_pos = (vertex * FOV / z) + center`
- Constants: FOV = 200.0, PROJECTION_DISTANCE = 4.0
- Rotation speed: 0.03 rad/frame (auto), 0.0005 rad/pixel (touch)

**Particle System:**
- MAX_PARTICLES = 200, EMISSION_RATE = 3/frame, PARTICLE_SPEED = 0.02
- Particles emit from cube center (0,0,0) with random normalized velocities
- Physics: Simple velocity integration with boundary reflection at ±1.0
- Pseudo-random generation: Uses millisecond timestamp for deterministic randomness
- Colors: RED, GREEN, BLUE, YELLOW, CYAN, MAGENTA (randomly assigned on spawn)
- Rendering: Each particle is a 3x3 colored rectangle, rotates with cube

**Touch Control:**
The async CST816x driver (drivers crate) monitors GPIO21 for touch interrupts:
- `Event::Down`: Capture initial touch position (initial_touch_x, initial_touch_y)
- `Event::Up`: Calculate delta, convert to rotation quaternions for X/Y axes, compose with current rotation
- Delta-based rotation: `qy * qx * rotation` (Y rotation from horizontal drag, X rotation from vertical drag)

### Important Hardware Details

**PSRAM Access:**
The `psram_allocator!` macro must be called after heap initialization to enable PSRAM allocations. PSRAM mode is configured via env var: `ESP_HAL_CONFIG_PSRAM_MODE = "octal"`.

**Power Management:**
GPIO38 (PMICEN) must be set high to enable the power management IC before display initialization.

**DMA Configuration:**
SPI DMA uses 32,000-byte buffers (rx/tx) created with `dma_buffers!` macro. The display staging buffer is 512 bytes, allocated in a static cell for 'static lifetime.

**SPI Driver in RAM:**
`ESP_HAL_PLACE_SPI_DRIVER_IN_RAM = "true"` ensures SPI driver code is placed in RAM for performance.

**Clippy Lint:**
`mem::forget` is explicitly denied at the crate level because it's unsafe with esp-hal types holding DMA buffers during transfers.

**Display Orientation:**
The display uses `Rotation::Deg270` with `mirrored: false` to achieve the correct orientation.

## Key Dependencies

- **esp-hal (1.0.0)**: Hardware abstraction layer for ESP32-S3 with PSRAM support
- **esp-rtos (0.2.0)**: RTOS integration with embassy async runtime, esp-alloc, and esp-radio
- **mipidsi (git master)**: MIPI DSI display driver for RM67162 (awaiting official release > 0.9.0)
- **drivers (git tag v0.14.0)**: CST816x async touch controller driver
- **embedded-graphics (0.8.1)**: 2D graphics primitives and drawing traits
- **micromath (2.1.0)**: no_std quaternion and vector math
- **embedded-hal-bus (0.3.0)**: Async SPI device abstraction
- **static_cell (2.1.1)**: Static cell allocation for DMA buffers
- **num-traits (0.2.19)**: no_std numeric traits with libm

## Common Modification Patterns

**Changing Display Resolution:**
1. Update `DISPLAY_WIDTH` and `DISPLAY_HEIGHT` in config.rs
2. Recalculate tile constants in display.rs: `TILES_X`, `TILES_Y`, `TOTAL_TILES`
3. Verify PSRAM allocation is sufficient: `2 × width × height × 2` bytes

**Adding Graphics Primitives:**
Create a `BufferDrawTarget` from `back_buffer` and use embedded-graphics `Drawable` trait. Mark affected tiles dirty with `self.current_tiles.mark_rect(x1, y1, x2, y2)`. See `draw_colored_point()` for reference.

**Adjusting 3D Rendering:**
- FOV / PROJECTION_DISTANCE (main.rs): Change camera perspective
- ROTATION_SPEED (main.rs): Adjust automatic rotation rate
- ROTATION_SENSITIVITY (main.rs): Modify touch gesture response
- Pre-calculate constant quaternions outside the loop for performance

**Modifying Particle System:**
- MAX_PARTICLES: Change particle pool size (impacts stack usage)
- EMISSION_RATE: Particles spawned per frame
- PARTICLE_SPEED: Initial particle velocity magnitude
- Boundary constraints: Currently ±1.0, modify clamp/bounce logic in particle update loop
- Colors: Edit color selection logic based on `color_seed`

**Memory Optimization:**
- Heap size: Adjust `esp_alloc::heap_allocator!(size: 73744)` if more DRAM needed
- PSRAM allocations: Use `Vec` after calling `psram_allocator!` macro
- Stack allocations: Large arrays like particle pool use stack; monitor stack usage
- No allocation rendering: FPS text uses pre-allocated buffer and manual formatting

**Performance Tuning:**
- Tile size: Smaller tiles = finer granularity, more tracking overhead
- Batching: Horizontal batching merges adjacent dirty tiles; could extend to 2D
- Particle count: Directly impacts rendering load
- DMA buffer size: 32KB balances latency and throughput
