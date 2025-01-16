# Pixels-rs

`pixels-rs` is a Rust project that demonstrates the use of an embedded graphics library to render an interactive 3D cube on an RM67162 AMOLED display with touch input capabilities. The project is designed to run on an ESP32-S3 microcontroller.

## Features

- Interactive 3D cube with touch-based rotation control
- Quaternion-based rotation for smooth animation
- Touch gesture recognition for cube manipulation
- Real-time FPS display
- Configurable display parameters
- Support for CST816S touch controller

## Project Structure

- `main.rs`: Main application entry point, handling display, touch input, and 3D rendering
- `display.rs`: Display abstraction layer and graphics primitives
- `config.rs`: Configuration constants for display dimensions

## Hardware Requirements

- ESP32-S3 microcontroller
- RM67162 AMOLED display
- CST816S touch controller
- I2C and SPI interfaces

## Dependencies

- `s3-display-amoled-touch-drivers`: Display and touch controller drivers
- `embedded-graphics`: 2D graphics library for embedded systems
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


