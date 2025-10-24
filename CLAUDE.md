# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`pixels-rs` is an embedded Rust application for ESP32-S3 microcontrollers that renders interactive 3D graphics on an RM67162 AMOLED display (536x240) with CST816S touch controller support. The project demonstrates quaternion-based 3D rotation, perspective projection, double buffering, and real-time touch gesture controls.

**Target Hardware:**
- ESP32-S3 microcontroller (running at 240MHz)
- RM67162 AMOLED display (536x240 pixels)
- CST816S touch controller
- SPI interface for display (GPIO 47, 18, 6, 7, 17)
- I2C interface for touch (GPIO 2, 3)

## Build Commands

**Build for development:**
```sh
. ~/export-esp.sh
cargo build
```

**Build optimized release:**
```sh
. ~/export-esp.sh
cargo build --release
```

Note: Even debug builds use optimization level 's' to ensure reasonable performance on embedded hardware.

## Architecture

### Core Module Structure

- **main.rs**: Application entry point and render loop
  - Initializes hardware peripherals (SPI, I2C, PSRAM, display, touch)
  - Implements 3D cube rendering with quaternion-based rotation
  - Handles touch gestures and FPS calculation
  - Runs in infinite loop with double-buffered rendering

- **display.rs**: Display abstraction with double buffering
  - Wraps mipidsi RM67162 driver with custom interface
  - Implements `DisplayTrait` for text and line drawing
  - Manages front/back buffer swapping for flicker-free rendering
  - Uses DMA transfers for efficient SPI communication (80 MHz)

- **config.rs**: Display dimension constants
  - `DISPLAY_WIDTH: u16 = 536`
  - `DISPLAY_HEIGHT: u16 = 240`

### Key Architectural Patterns

**Double Buffering:**
The display module maintains two framebuffers (`front_buffer`, `back_buffer`) allocated in PSRAM. All drawing operations write to the back buffer. On `update_with_buffer()`, the front buffer is sent to the display via DMA, buffers are swapped, and the new back buffer is cleared. This prevents tearing artifacts.

**Memory Management:**
- DRAM heap: 73,744 bytes for general allocations
- PSRAM: Used for large framebuffers (2 × 536 × 240 × 2 bytes = ~512KB)
- Static cells for DMA buffers to ensure 'static lifetime requirements

**Rendering Pipeline:**
1. Process touch input → update rotation quaternion
2. Apply automatic rotation (pre-calculated quaternion multiplication)
3. Transform cube vertices using quaternion rotation
4. Perspective projection with depth clipping (z > 0.01)
5. Draw edges to back buffer
6. Render FPS text to back buffer
7. Swap buffers and display

**3D Graphics:**
- Quaternion-based rotation for smooth interpolation and gimbal lock avoidance
- Perspective projection: `screen_pos = (vertex * FOV / z) + center`
- Field of view: 200.0, Projection distance: 4.0
- Rotation speed: 0.03 rad/frame (automatic), touch sensitivity: 0.0005 rad/pixel

**Touch Control:**
The CST816x driver monitors GPIO21 for touch interrupts. On touch down, initial position is captured. On touch up, delta is calculated and converted to rotation quaternions for X and Y axes, which are composed with the current rotation state.

### Important Hardware Details

**PSRAM Access:**
The `psram_allocator!` macro must be called after heap initialization to enable PSRAM allocations for large buffers.

**Power Management:**
GPIO38 (PMICEN) must be set high to enable the power management IC before display initialization.

**DMA Configuration:**
SPI DMA uses 32,000-byte buffers (rx/tx) for efficient bulk transfers. The display buffer size is 512 bytes for command/data staging.

**Clippy Lint:**
`mem::forget` is explicitly denied because it's unsafe with esp-hal types that hold DMA buffers during transfers.

## Key Dependencies

- **esp-hal (1.0.0-rc.1)**: Hardware abstraction layer for ESP32-S3
- **esp-rtos (0.1.1)**: RTOS integration with embassy async runtime
- **mipidsi (git)**: MIPI DSI display driver for RM67162
- **drivers (git tag v0.9.0)**: CST816x touch controller driver
- **embedded-graphics (0.8.1)**: 2D graphics primitives
- **micromath (2.1.0)**: Quaternion and vector math (no_std)
- **embassy-executor/time**: Async executor and time management

## Common Modification Patterns

**Changing Display Resolution:**
Update `config.rs` constants and verify PSRAM allocation is sufficient for `2 × width × height × 2` bytes.

**Adding Graphics Primitives:**
Implement drawing in `display.rs` by creating a `BufferDrawTarget` from `back_buffer` and using embedded-graphics `Drawable` trait.

**Adjusting 3D Rendering:**
- Modify `FOV`, `PROJECTION_DISTANCE` in main.rs for camera behavior
- Change `ROTATION_SPEED` for automatic rotation rate
- Adjust `ROTATION_SENSITIVITY` in touch handler for gesture response

**Memory Constraints:**
If adding features that require more RAM, adjust the heap allocator size (currently 73,744 bytes) or utilize PSRAM for large allocations.
