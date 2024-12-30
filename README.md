# Pixels-rs

`pixels-rs` is a Rust project that demonstrates the use of an embedded graphics library to render a rotating 3D cube on an RM67162 AMOLED display. The project is designed to run on an ESP32 microcontroller.

## Features

- Renders a 3D cube with perspective projection
- Uses quaternion-based rotation for smooth animation
- Displays frames per second (FPS) on the screen
- Configurable display parameters

## Project Structure

- `main.rs`: The main entry point of the application. Initializes the display and handles the rendering loop.
- `rm67162.rs`: Contains the driver implementation for the RM67162 AMOLED display.
- `config.rs`: Configuration constants for the display dimensions.
- `display.rs`: Abstraction layer for the display operations.

## Dependencies

- `embedded-graphics`: A 2D graphics library for embedded systems.
- `micromath`: A library for small-scale mathematical operations, including quaternions.
- `esp-hal`: Hardware abstraction layer for the ESP32 microcontroller.
- `heapless`: Provides heapless data structures.
- `defmt`: A framework for efficient, deferred formatting of log messages.

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

## Configuration

The display dimensions are defined in `config.rs`:
```rust
pub const DISPLAY_HEIGHT: u16 = 240;
pub const DISPLAY_WIDTH: u16 = 536;
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Acknowledgements

- [LilyGo](https://www.lilygo.cc/) for the AMOLED display reference implementation.
- The Rust embedded community for their contributions to the ecosystem.


