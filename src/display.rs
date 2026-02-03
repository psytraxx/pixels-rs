use core::convert::Infallible;
use core::fmt::Debug;
use core::future::Future;
use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::mono_font::iso_8859_1::FONT_10X20 as FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::Primitive;
use embedded_graphics::primitives::{Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::Drawable;
use embedded_hal_bus::spi::{DeviceError, ExclusiveDevice, NoDelay};
use esp_hal::dma::DmaTxBuf;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{DMA_CH0, GPIO17, GPIO18, GPIO47, GPIO6, GPIO7, SPI2};
use esp_hal::spi::master::{Config as SpiConfig, Spi, SpiDmaBus};
use esp_hal::spi::{Error, Mode};
use esp_hal::time::Rate;
use esp_hal::{dma_buffers, Async};
use lcd_async::interface::{SpiError, SpiInterface};
use lcd_async::models::RM67162;
use lcd_async::options::{Orientation, Rotation};
use lcd_async::raw_framebuf::RawFrameBuf;
use lcd_async::Builder;
use static_cell::StaticCell;

use crate::config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
const LINE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RgbColor::WHITE, 2);

pub type MipiDisplayWrapper<'a> = lcd_async::Display<
    SpiInterface<ExclusiveDevice<SpiDmaBus<'a, Async>, Output<'a>, NoDelay>, Output<'a>>,
    RM67162,
    Output<'a>,
>;

const TILE_SIZE: u16 = 32; // 32x32 pixel tiles
const TILES_X: usize = DISPLAY_WIDTH.div_ceil(TILE_SIZE) as usize; // 17 tiles wide
const TILES_Y: usize = DISPLAY_HEIGHT.div_ceil(TILE_SIZE) as usize; // 8 tiles high
const TOTAL_TILES: usize = TILES_X * TILES_Y; // 136 tiles total

const PIXEL_SIZE: usize = 2; // RGB565 = 2 bytes per pixel
const FRAME_SIZE: usize = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize) * PIXEL_SIZE;

static FRAME_BUFFER: StaticCell<[u8; FRAME_SIZE]> = StaticCell::new();

pub struct Display<'a> {
    display: MipiDisplayWrapper<'static>,
    raw_fb: RawFrameBuf<embedded_graphics::pixelcolor::Rgb565, &'a mut [u8]>,
    current_tiles: TileTracker, // Tiles drawn this frame
    prev_tiles: TileTracker,    // Tiles to clear (from 2 frames ago)
}

#[derive(Clone, Copy)]
struct TileTracker {
    dirty: [bool; TOTAL_TILES],
}

impl TileTracker {
    fn new() -> Self {
        Self {
            dirty: [false; TOTAL_TILES],
        }
    }

    fn mark_rect(&mut self, x1: u16, y1: u16, x2: u16, y2: u16) {
        let min_x = x1.min(x2).min(DISPLAY_WIDTH - 1);
        let max_x = x1.max(x2).min(DISPLAY_WIDTH - 1);
        let min_y = y1.min(y2).min(DISPLAY_HEIGHT - 1);
        let max_y = y1.max(y2).min(DISPLAY_HEIGHT - 1);

        let tile_x1 = (min_x / TILE_SIZE) as usize;
        let tile_x2 = (max_x / TILE_SIZE) as usize;
        let tile_y1 = (min_y / TILE_SIZE) as usize;
        let tile_y2 = (max_y / TILE_SIZE) as usize;

        for ty in tile_y1..=tile_y2 {
            for tx in tile_x1..=tile_x2 {
                let tile_idx = ty * TILES_X + tx;
                if tile_idx < TOTAL_TILES {
                    self.dirty[tile_idx] = true;
                }
            }
        }
    }

    fn clear(&mut self) {
        self.dirty.fill(false);
    }

    fn is_dirty(&self, tile_idx: usize) -> bool {
        tile_idx < TOTAL_TILES && self.dirty[tile_idx]
    }
}

/// Display interface trait for LCD controller
///
/// Provides basic drawing operations for text and primitives.
/// Implementations should handle the low-level display communication.
pub trait DisplayTrait {
    /// Error type
    type Error: Debug;

    /// Writes text to the display at the specified position
    ///
    /// # Arguments
    /// * `text` - The text string to display
    /// * `position` - Starting position coordinates as Point(x,y)
    ///
    /// # Returns
    /// * `Ok(())` on successful write
    /// * `Err(Error)` if the write operation fails
    fn write(&mut self, text: &str, position: Point) -> Result<(), Self::Error>;

    /// Updates the display with the current framebuffer contents
    ///
    /// # Returns
    /// * `Ok(())` on successful update
    /// * `Err(Error)` if the update operation fails
    fn update_with_buffer(&mut self) -> impl Future<Output = Result<(), Self::Error>>;

    /// Draws a line between two points
    ///
    /// # Arguments
    /// * `begin` - Starting point coordinates as Point(x,y)  
    /// * `end` - Ending point coordinates as Point(x,y)
    ///
    /// # Returns
    /// * `Ok(())` on successful line draw
    /// * `Err(Error)` if the draw operation fails
    fn draw_line(&mut self, begin: Point, end: Point) -> Result<(), Self::Error>;
}

pub struct DisplayPeripherals {
    pub sck: GPIO47<'static>,
    pub mosi: GPIO18<'static>,
    pub cs: GPIO6<'static>,
    pub dc: GPIO7<'static>,
    pub rst: GPIO17<'static>,
    pub spi: SPI2<'static>,
    pub dma: DMA_CH0<'static>,
}

impl<'a> Display<'a> {
    pub async fn new(p: DisplayPeripherals) -> Result<Self, DisplayError> {
        // SPI pins
        let dc = Output::new(p.dc, Level::Low, OutputConfig::default());
        let sck = Output::new(p.sck, Level::Low, OutputConfig::default());
        let mosi = Output::new(p.mosi, Level::Low, OutputConfig::default());
        let cs = Output::new(p.cs, Level::High, OutputConfig::default());

        #[allow(clippy::manual_div_ceil)]
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(8192);
        let dma_rx_buf = esp_hal::dma::DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        // Configure SPI
        let spi_dma = Spi::new(
            p.spi,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(80))
                .with_mode(Mode::_0),
        )
        .unwrap()
        .with_sck(sck)
        .with_mosi(mosi)
        .with_dma(p.dma)
        .into_async();

        // Create the SPI DMA bus with the configured buffers
        let spi = SpiDmaBus::new(spi_dma, dma_rx_buf, dma_tx_buf);

        // Attach the SPI device using the chip-select control pin (no delay used)
        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        // Create the SPI interface for the display driver using the SPI device, DC pin, and initialization buffer
        let di = SpiInterface::new(spi_device, dc);

        let mut delay = embassy_time::Delay;

        let rst_pin = p.rst;
        let display = Builder::new(RM67162, di)
            .orientation(Orientation {
                mirrored: false,
                rotation: Rotation::Deg270,
            })
            .reset_pin(Output::new(rst_pin, Level::High, OutputConfig::default()))
            .init(&mut delay)
            .await
            .unwrap();

        // Initialize frame buffer
        let frame_buffer = FRAME_BUFFER.init_with(|| [0; FRAME_SIZE]);

        // Create a framebuffer for drawing
        let raw_fb = RawFrameBuf::<Rgb565, _>::new(
            frame_buffer.as_mut_slice(),
            DISPLAY_WIDTH.into(),
            DISPLAY_HEIGHT.into(),
        );

        Ok(Self {
            display,
            raw_fb,
            current_tiles: TileTracker::new(),
            prev_tiles: TileTracker::new(),
        })
    }
}

impl<'a> DisplayTrait for Display<'a> {
    type Error = DisplayError;

    fn write(&mut self, text: &str, position: Point) -> Result<(), Self::Error> {
        // Estimate text bounds (10x20 font)
        let text_width = (text.len() as u16) * 10;
        let text_height = 20u16;

        let x = position.x.max(0) as u16;
        let y = position.y.max(0) as u16;
        let x2 = (x + text_width).min(DISPLAY_WIDTH - 1);
        let y2 = (y + text_height).min(DISPLAY_HEIGHT - 1);

        // Mark tiles dirty
        self.current_tiles.mark_rect(x, y, x2, y2);

        Text::with_baseline(text, position, TEXT_STYLE, Baseline::Top).draw(&mut self.raw_fb)?;
        Ok(())
    }

    fn draw_line(&mut self, start: Point, end: Point) -> Result<(), Self::Error> {
        // Mark tiles dirty (add small padding for 2-pixel stroke)
        let x1 = start.x.max(0).saturating_sub(2) as u16;
        let y1 = start.y.max(0).saturating_sub(2) as u16;
        let x2 = (end.x.max(0) + 2).min(DISPLAY_WIDTH as i32 - 1) as u16;
        let y2 = (end.y.max(0) + 2).min(DISPLAY_HEIGHT as i32 - 1) as u16;

        self.current_tiles.mark_rect(x1, y1, x2, y2);

        Line::new(start, end)
            .into_styled(LINE_STYLE)
            .draw(&mut self.raw_fb)?;
        Ok(())
    }

    async fn update_with_buffer(&mut self) -> Result<(), Self::Error> {
        // Send the entire framebuffer to the display
        // lcd_async uses show_raw_data which takes a byte slice
        let buffer = self.raw_fb.as_bytes();

        let result = self
            .display
            .show_raw_data(0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, buffer)
            .await;

        result.map_err(DisplayError::SpiError)?;

        // Save current tiles for clearing next frame
        self.prev_tiles = self.current_tiles;
        self.current_tiles.clear();

        Ok(())
    }
}

impl<'a> Display<'a> {
    /// Draws a small colored point (3x3 pixels) at the specified position
    pub fn draw_colored_point(
        &mut self,
        position: Point,
        color: Rgb565,
    ) -> Result<(), DisplayError> {
        use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
        use embedded_graphics::Drawable;

        let style = PrimitiveStyleBuilder::new().fill_color(color).build();

        // Draw 3x3 rectangle
        let x = position.x.saturating_sub(1).max(0) as u16;
        let y = position.y.saturating_sub(1).max(0) as u16;
        let x2 = (position.x + 1).min(DISPLAY_WIDTH as i32 - 1) as u16;
        let y2 = (position.y + 1).min(DISPLAY_HEIGHT as i32 - 1) as u16;

        self.current_tiles.mark_rect(x, y, x2, y2);

        Rectangle::new(position - Point::new(1, 1), Size::new(3, 3))
            .into_styled(style)
            .draw(&mut self.raw_fb)?;

        Ok(())
    }

    /// Clears only the dirty tiles of the buffer - call this at the start of each frame
    pub fn clear_buffer(&mut self) {
        // Clear tiles that were dirty last frame
        for tile_idx in 0..TOTAL_TILES {
            if self.prev_tiles.is_dirty(tile_idx) {
                let tile_x = (tile_idx % TILES_X) as u16;
                let tile_y = (tile_idx / TILES_X) as u16;

                let x_start = tile_x * TILE_SIZE;
                let y_start = tile_y * TILE_SIZE;
                let x_end = ((tile_x + 1) * TILE_SIZE).min(DISPLAY_WIDTH);
                let y_end = ((tile_y + 1) * TILE_SIZE).min(DISPLAY_HEIGHT);
                let style = PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::BLACK)
                    .build();

                let _ = Rectangle::new(
                    Point::new(x_start as i32, y_start as i32),
                    Size::new((x_end - x_start) as u32, (y_end - y_start) as u32),
                )
                .into_styled(style)
                .draw(&mut self.raw_fb);
            }
        }
    }
}

#[derive(Debug)]
pub enum DisplayError {
    Infallible,
    SpiError(#[allow(unused)] SpiError<DeviceError<Error, Infallible>, Infallible>),
}

impl From<SpiError<DeviceError<Error, Infallible>, Infallible>> for DisplayError {
    fn from(err: SpiError<DeviceError<Error, Infallible>, Infallible>) -> Self {
        DisplayError::SpiError(err)
    }
}

impl From<Infallible> for DisplayError {
    fn from(_: Infallible) -> Self {
        Self::Infallible
    }
}
