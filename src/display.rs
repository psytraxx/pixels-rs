use alloc::vec::Vec;
use core::cmp::{max, min};
use core::convert::Infallible;
use core::fmt::Debug;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
use embedded_graphics::mono_font::iso_8859_1::FONT_10X20 as FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::{Dimensions, Primitive};
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::{Drawable, Pixel};
use embedded_hal_bus::spi::{DeviceError, ExclusiveDevice};
use esp_hal::delay::Delay;
use esp_hal::dma::DmaTxBuf;
use esp_hal::dma_buffers;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{DMA_CH0, GPIO17, GPIO18, GPIO47, GPIO6, GPIO7, SPI2};
use esp_hal::spi::master::{Config as SpiConfig, Spi, SpiDmaBus};
use esp_hal::spi::{Error, Mode};
use esp_hal::time::Rate;
use mipidsi::interface::{SpiError, SpiInterface};
use mipidsi::models::RM67162;
use mipidsi::options::{Orientation, Rotation};
use mipidsi::{Builder, Display as MipiDisplay};
use static_cell::StaticCell;

use crate::config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
const LINE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RgbColor::WHITE, 2);

pub type MipiDisplayWrapper<'a> = MipiDisplay<
    SpiInterface<
        'a,
        ExclusiveDevice<
            SpiDmaBus<'a, esp_hal::Blocking>,
            Output<'a>,
            embedded_hal_bus::spi::NoDelay,
        >,
        Output<'a>,
    >,
    RM67162,
    Output<'a>,
>;

pub struct Display {
    display: MipiDisplayWrapper<'static>,
    front_buffer: Vec<Rgb565>,
    back_buffer: Vec<Rgb565>,
    dirty_region: Option<Rectangle>,
}

struct BufferDrawTarget<'a> {
    buffer: &'a mut Vec<Rgb565>,
    width: usize,
    height: usize,
}

impl<'a> DrawTarget for BufferDrawTarget<'a> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x >= 0
                && coord.x < self.width as i32
                && coord.y >= 0
                && coord.y < self.height as i32
            {
                let index = (coord.y as usize) * self.width + coord.x as usize;
                if index < self.buffer.len() {
                    self.buffer[index] = color;
                }
            }
        }
        Ok(())
    }
}

impl<'a> OriginDimensions for BufferDrawTarget<'a> {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

/// Display interface trait for ST7789 LCD controller
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
    fn update_with_buffer(&mut self) -> Result<(), Self::Error>;

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

impl Display {
    pub fn new(p: DisplayPeripherals) -> Result<Self, DisplayError> {
        // SPI pins
        let dc = Output::new(p.dc, Level::Low, OutputConfig::default());
        let sck = Output::new(p.sck, Level::Low, OutputConfig::default());
        let mosi = Output::new(p.mosi, Level::Low, OutputConfig::default());
        let cs = Output::new(p.cs, Level::High, OutputConfig::default());

        #[allow(clippy::manual_div_ceil)]
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
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
        .with_dma(p.dma);

        // Create the SPI DMA bus with the configured buffers
        let spi = SpiDmaBus::new(spi_dma, dma_rx_buf, dma_tx_buf);

        // Attach the SPI device using the chip-select control pin (no delay used)
        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        const DISPLAY_BUFFER_SIZE: usize = 512;
        static DISPLAY_BUFFER: StaticCell<[u8; DISPLAY_BUFFER_SIZE]> = StaticCell::new();
        let buffer = DISPLAY_BUFFER.init([0_u8; 512]);

        // Create the SPI interface for the display driver using the SPI device, DC pin, and initialization buffer
        let di = SpiInterface::new(spi_device, dc, buffer);

        let mut delay = Delay::new();

        let rst_pin = p.rst;
        let display = Builder::new(RM67162, di)
            .orientation(Orientation {
                mirrored: false,
                rotation: Rotation::Deg270,
            })
            .reset_pin(Output::new(rst_pin, Level::High, OutputConfig::default()))
            .init(&mut delay)
            .unwrap();

        let buffer_size = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize);
        let mut front_buffer = Vec::new();
        front_buffer.resize(buffer_size, Rgb565::BLACK);
        let mut back_buffer = Vec::new();
        back_buffer.resize(buffer_size, Rgb565::BLACK);

        Ok(Self {
            display,
            front_buffer,
            back_buffer,
            dirty_region: None,
        })
    }
}

impl Display {
    /// Marks an axis-aligned region as dirty so it gets flushed on the next update.
    pub fn mark_region_dirty(&mut self, region: Rectangle) {
        let display_rect = Rectangle::with_corners(
            Point::new(0, 0),
            Point::new((DISPLAY_WIDTH - 1) as i32, (DISPLAY_HEIGHT - 1) as i32),
        );

        let clipped = region.intersection(&display_rect);

        if clipped.size.width == 0 || clipped.size.height == 0 {
            return;
        }

        self.dirty_region = Some(match self.dirty_region.take() {
            Some(existing) => {
                let existing_br = existing.bottom_right().unwrap_or(existing.top_left);
                let clipped_br = clipped.bottom_right().unwrap_or(clipped.top_left);

                let top_left = Point::new(
                    min(existing.top_left.x, clipped.top_left.x),
                    min(existing.top_left.y, clipped.top_left.y),
                );
                let bottom_right = Point::new(
                    max(existing_br.x, clipped_br.x),
                    max(existing_br.y, clipped_br.y),
                );

                Rectangle::with_corners(top_left, bottom_right)
            }
            None => clipped,
        });
    }
}

impl DisplayTrait for Display {
    type Error = DisplayError;

    fn write(&mut self, text: &str, position: Point) -> Result<(), Self::Error> {
        let drawable = Text::with_baseline(text, position, TEXT_STYLE, Baseline::Top);
        self.mark_region_dirty(drawable.bounding_box());
        let mut target = BufferDrawTarget {
            buffer: &mut self.back_buffer,
            width: DISPLAY_WIDTH as usize,
            height: DISPLAY_HEIGHT as usize,
        };
        drawable.draw(&mut target)?;
        Ok(())
    }

    fn draw_line(&mut self, start: Point, end: Point) -> Result<(), Self::Error> {
        let styled = Line::new(start, end).into_styled(LINE_STYLE);
        self.mark_region_dirty(styled.bounding_box());
        let mut target = BufferDrawTarget {
            buffer: &mut self.back_buffer,
            width: DISPLAY_WIDTH as usize,
            height: DISPLAY_HEIGHT as usize,
        };
        styled.draw(&mut target)?;
        Ok(())
    }

    fn update_with_buffer(&mut self) -> Result<(), Self::Error> {
        let Some(region) = self.dirty_region.take() else {
            return Ok(());
        };

        let top_left = region.top_left;
        let bottom_right = region.bottom_right().unwrap_or(region.top_left);

        let x_start = max(top_left.x, 0) as u16;
        let y_start = max(top_left.y, 0) as u16;
        let x_end = min(bottom_right.x, (DISPLAY_WIDTH - 1) as i32) as u16;
        let y_end = min(bottom_right.y, (DISPLAY_HEIGHT - 1) as i32) as u16;

        let width = DISPLAY_WIDTH as usize;
        for y in y_start..=y_end {
            let row = y as usize;
            let start = row * width + x_start as usize;
            let len = (x_end - x_start + 1) as usize;
            let slice = &self.back_buffer[start..start + len];
            self.display
                .set_pixels(x_start, y, x_end, y, slice.iter().copied())?;
        }

        core::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
        self.back_buffer.fill(Rgb565::BLACK);
        Ok(())
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
