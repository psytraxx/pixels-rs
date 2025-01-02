use crate::rm67162::RM67162;

use core::convert::Infallible;
use defmt::info;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Point;
use embedded_graphics::mono_font::iso_8859_1::FONT_10X20 as FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::Primitive;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::Drawable;
use embedded_graphics_framebuf::FrameBuf;
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::delay::Delay;
use esp_hal::dma::{Dma, DmaRxBuf, DmaTxBuf};
use esp_hal::gpio::{GpioPin, Level, Output};
use esp_hal::peripherals::{DMA, SPI2};
use esp_hal::spi::master::{Config, Spi, SpiDmaBus};
use esp_hal::{dma_buffers, prelude::*};
use mipidsi::interface::SpiInterface;
use mipidsi::options::Orientation;
use mipidsi::{Builder, Display as MipiDisplay};

use crate::config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
pub const LCD_PIXELS: usize = (DISPLAY_HEIGHT as usize) * (DISPLAY_WIDTH as usize);
type DisplayBuffer = [Rgb565; LCD_PIXELS];

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

pub struct Display<'a> {
    display: MipiDisplayWrapper<'a>,
    framebuf: FrameBuf<Rgb565, DisplayBuffer>,
}

/// Display interface trait for ST7789 LCD controller
///
/// Provides basic drawing operations for text and primitives.
/// Implementations should handle the low-level display communication.
pub trait DisplayTrait {
    /// Writes text to the display at the specified position
    ///
    /// # Arguments
    /// * `text` - The text string to display
    /// * `position` - Starting position coordinates as Point(x,y)
    ///
    /// # Returns
    /// * `Ok(())` on successful write
    /// * `Err(Error)` if the write operation fails
    fn write(&mut self, text: &str, position: Point) -> Result<(), Error>;

    /// Updates the display with the current framebuffer contents
    ///
    /// # Returns
    /// * `Ok(())` on successful update
    /// * `Err(Error)` if the update operation fails
    fn update_with_buffer(&mut self) -> Result<(), Error>;

    /// Draws a line between two points
    ///
    /// # Arguments
    /// * `begin` - Starting point coordinates as Point(x,y)  
    /// * `end` - Ending point coordinates as Point(x,y)
    ///
    /// # Returns
    /// * `Ok(())` on successful line draw
    /// * `Err(Error)` if the draw operation fails
    fn draw_line(&mut self, begin: Point, end: Point) -> Result<(), Error>;
}

pub struct DisplayPeripherals {
    pub sck: GpioPin<47>,
    pub mosi: GpioPin<18>,
    pub cs: GpioPin<6>,
    pub pmicen: GpioPin<38>,
    pub dc: GpioPin<7>,
    pub rst: GpioPin<17>,
    pub spi: SPI2,
    pub dma: DMA,
}

impl<'a> Display<'a> {
    pub fn new(p: DisplayPeripherals, buffer: &'a mut [u8]) -> Result<Self, Error> {
        // SPI pins
        let sck = Output::new(p.sck, Level::Low);
        let mosi = Output::new(p.mosi, Level::Low);
        let cs = Output::new(p.cs, Level::High);

        let mut pmicen = Output::new_typed(p.pmicen, Level::Low);
        pmicen.set_high();
        info!("PMICEN set high");

        let dma = Dma::new(p.dma);

        // Configure SPI
        let spi = Spi::new_with_config(
            p.spi,
            Config {
                frequency: 75.MHz(),
                ..Config::default()
            },
        )
        .with_sck(sck)
        .with_mosi(mosi)
        .with_dma(
            dma.channel0
                .configure(false, esp_hal::dma::DmaPriority::Priority0),
        );

        let dc_pin = p.dc;
        let rst_pin = p.rst;
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
        let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        let spi = SpiDmaBus::new(spi, dma_rx_buf, dma_tx_buf);

        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        let di = SpiInterface::new(spi_device, Output::new(dc_pin, Level::Low), buffer);

        let mut delay = Delay::new();

        let display = Builder::new(RM67162, di)
            .orientation(Orientation {
                mirrored: false,
                rotation: mipidsi::options::Rotation::Deg90,
            })
            .display_size(DISPLAY_WIDTH, DISPLAY_HEIGHT)
            .reset_pin(Output::new(rst_pin, Level::High))
            .init(&mut delay)
            .unwrap();

        let data = [Rgb565::BLACK; LCD_PIXELS];
        let framebuf: FrameBuf<Rgb565, [Rgb565; _]> =
            FrameBuf::new(data, DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);

        Ok(Self { display, framebuf })
    }
}

impl<'a> DisplayTrait for Display<'a> {
    fn write(&mut self, text: &str, position: Point) -> Result<(), Error> {
        Text::with_baseline(text, position, TEXT_STYLE, Baseline::Top).draw(&mut self.framebuf)?;
        Ok(())
    }

    fn draw_line(&mut self, start: Point, end: Point) -> Result<(), Error> {
        Line::new(start, end)
            .into_styled(PrimitiveStyle::with_stroke(RgbColor::WHITE, 2))
            .draw(&mut self.framebuf)?;
        Ok(())
    }

    fn update_with_buffer(&mut self) -> Result<(), Error> {
        let pixel_iterator = self.framebuf.into_iter().map(|p| p.1);

        self.display
            .set_pixels(0, 0, DISPLAY_WIDTH - 1, DISPLAY_HEIGHT, pixel_iterator)
            .map_err(|_| Error::DisplayInterface)?;

        // Clear the frame buffer
        self.framebuf.clear(RgbColor::BLACK)?;
        Ok(())
    }
}

/// A clock error
#[derive(Debug)]
pub enum Error {
    DisplayInterface,
    Infallible,
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        Self::Infallible
    }
}
