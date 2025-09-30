use alloc::vec::Vec;
use core::convert::Infallible;
use core::fmt::Debug;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
use embedded_graphics::mono_font::iso_8859_1::FONT_10X20 as FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::Primitive;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
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
        })
    }
}

impl DisplayTrait for Display {
    type Error = DisplayError;

    fn write(&mut self, text: &str, position: Point) -> Result<(), Self::Error> {
        let mut target = BufferDrawTarget {
            buffer: &mut self.back_buffer,
            width: DISPLAY_WIDTH as usize,
            height: DISPLAY_HEIGHT as usize,
        };
        Text::with_baseline(text, position, TEXT_STYLE, Baseline::Top).draw(&mut target)?;
        Ok(())
    }

    fn draw_line(&mut self, start: Point, end: Point) -> Result<(), Self::Error> {
        let mut target = BufferDrawTarget {
            buffer: &mut self.back_buffer,
            width: DISPLAY_WIDTH as usize,
            height: DISPLAY_HEIGHT as usize,
        };
        Line::new(start, end)
            .into_styled(LINE_STYLE)
            .draw(&mut target)?;
        Ok(())
    }

    fn update_with_buffer(&mut self) -> Result<(), Self::Error> {
        // Send front_buffer to display
        self.display.set_pixels(
            0,
            0,
            DISPLAY_WIDTH - 1,
            DISPLAY_HEIGHT - 1,
            self.front_buffer.iter().cloned(),
        )?;
        // Swap buffers
        core::mem::swap(&mut self.front_buffer, &mut self.back_buffer);
        // Clear back buffer
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
