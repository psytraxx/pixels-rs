# Pixels-rs

A high-performance Rust application that demonstrates real-time 3D graphics with particle effects on an ESP32-S3 microcontroller, achieving **50-60 FPS** on an RM67162 AMOLED display with touch input capabilities.

## Features

- **Interactive 3D wireframe cube** with dual control:
  - Automatic quaternion-based rotation
  - Touch-based gesture control (drag to rotate)
- **3D particle system** with 200 particles:
  - Physics-based bouncing within cube boundaries
  - Random vibrant colors
  - Full 3D rotation synchronized with cube
- **Tile-based rendering** with horizontal batching
- **Double-buffered rendering** with selective clearing
- **Hardware-accelerated DMA** transfers at 80 MHz SPI
- **Real-time FPS counter**

## Performance

- **50-60 FPS** (3.5× improvement over baseline)
- **~85% reduction** in data transfer per frame (257KB → 40-80KB)
- **DMA transfer optimization** through horizontal tile batching

## Hardware Requirements

- ESP32-S3 microcontroller (240 MHz)
- RM67162 AMOLED display (536×240 pixels)
- CST816S touch controller
- PSRAM for framebuffers

## Quick Start

1. **Set up ESP32 Rust environment:**
   ```sh
   . ~/export-esp.sh
   ```

2. **Build and flash:**
   ```sh
   cargo run --release
   ```

## Controls

- **Automatic Rotation**: Cube continuously rotates around the Y-axis
- **Touch Gesture**: Touch and drag to rotate the cube interactively

## Development

For detailed architecture, build instructions, and modification patterns, see [CLAUDE.md](CLAUDE.md).

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
