# Pixels-rs

`pixels-rs` is a Rust project that demonstrates real-time 3D graphics on an ESP32-S3 microcontroller with RM67162 AMOLED display and touch input capabilities.

## Features

- Interactive 3D cube with dual control:
  - Automatic rotation
  - Touch-based gesture control
- Hardware-accelerated graphics using DMA
- Quaternion-based smooth rotation
- Real-time FPS counter

## Project Structure

- `main.rs`: Main application entry point, handling display, touch input, and 3D rendering
- `display.rs`: Display abstraction layer and graphics primitives
- `config.rs`: Configuration constants for display dimensions

## Hardware Requirements

- ESP32-S3 microcontroller
- RM67162 AMOLED display (536x240)
- CST816S touch controller
- SPI interface for display (pins 47, 18, - 6, 7, 17)
- I2C interface for touch (pins 2, 3)

## Dependencies

- `mipidsi`: Display driver
- `cst816s-rs`: Touch controller driver
- `embedded-graphics`: 2D graphics primitives
- `micromath`: Mathematical operations including quaternion support
- `esp-hal`: ESP32-S3 hardware abstraction layer
- `embedded-hal-bus`: Hardware abstraction for I2C/SPI communication

## Getting Started

1. Clone the repository:
    ```sh
    git clone https://github.com/yourusername/pixels-rs.git
    cd pixels-rs
    ```

2. Build the project:
    ```sh
    cargo build --release
    ```

## Usage

The cube can be manipulated in two ways:
1. Automatic rotation around the Y-axis
2. Touch interaction:
   - Touch and drag to rotate the cube
   - The rotation angle is proportional to the drag distance

## Configuration

Display dimensions in `config.rs`:
```rust
pub const DISPLAY_HEIGHT: u16 = 240;
pub const DISPLAY_WIDTH: u16 = 536;
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.


