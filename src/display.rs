use core::convert::Infallible;
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
use embedded_hal_bus::spi::{DeviceError, ExclusiveDevice};
use esp_hal::delay::Delay;
use esp_hal::dma::{DmaChannel0, DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::gpio::{GpioPin, Level, Output};
use esp_hal::peripherals::SPI2;
use esp_hal::spi::master::{Config, Spi, SpiDmaBus};
use esp_hal::spi::Error;
use esp_hal::time::RateExtU32;
use esp_println::println;
use mipidsi::interface::{SpiError, SpiInterface};
use mipidsi::models::RM67162;
use mipidsi::options::{Orientation, Rotation};
use mipidsi::{Builder, Display as MipiDisplay};
use static_cell::StaticCell;

use crate::config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
const LINE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RgbColor::WHITE, 2);
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

pub struct Display {
    display: MipiDisplayWrapper<'static>,
    framebuf: FrameBuf<Rgb565, DisplayBuffer>,
}

/// Display interface trait for ST7789 LCD controller
///
/// Provides basic drawing operations for text and primitives.
/// Implementations should handle the low-level display communication.
pub trait DisplayTrait {
    /// Error type
    type Error: core::fmt::Debug;

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
    pub sck: GpioPin<47>,
    pub mosi: GpioPin<18>,
    pub cs: GpioPin<6>,
    pub pmicen: GpioPin<38>,
    pub dc: GpioPin<7>,
    pub rst: GpioPin<17>,
    pub spi: SPI2,
    pub dma: DmaChannel0,
}

impl Display {
    pub fn new(p: DisplayPeripherals) -> Result<Self, DisplayError> {
        // SPI pins
        let sck = Output::new(p.sck, Level::Low);
        let mosi = Output::new(p.mosi, Level::Low);
        let cs = Output::new(p.cs, Level::High);

        let mut pmicen = Output::new(p.pmicen, Level::Low);
        pmicen.set_high();
        println!("PMICEN set high");

        #[allow(clippy::manual_div_ceil)]
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
        let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        // Configure SPI
        let spi = Spi::new(p.spi, Config::default().with_frequency(80_u32.MHz()))
            .unwrap()
            .with_sck(sck)
            .with_mosi(mosi)
            .with_dma(p.dma)
            .with_buffers(dma_rx_buf, dma_tx_buf);

        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        let dc_pin = p.dc;

        const DISPLAY_BUFFER_SIZE: usize = 512;
        static DISPLAY_BUFFER: StaticCell<[u8; DISPLAY_BUFFER_SIZE]> = StaticCell::new();

        let di = SpiInterface::new(
            spi_device,
            Output::new(dc_pin, Level::Low),
            DISPLAY_BUFFER.init_with(|| [0u8; DISPLAY_BUFFER_SIZE]),
        );

        let mut delay = Delay::new();

        let rst_pin = p.rst;
        let display = Builder::new(RM67162, di)
            .orientation(Orientation {
                mirrored: false,
                rotation: Rotation::Deg270,
            })
            .reset_pin(Output::new(rst_pin, Level::High))
            .init(&mut delay)
            .unwrap();

        let data = [Rgb565::BLACK; LCD_PIXELS];
        let framebuf: FrameBuf<Rgb565, [Rgb565; _]> =
            FrameBuf::new(data, DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);

        Ok(Self { display, framebuf })
    }
}

impl DisplayTrait for Display {
    type Error = DisplayError;

    fn write(&mut self, text: &str, position: Point) -> Result<(), Self::Error> {
        Text::with_baseline(text, position, TEXT_STYLE, Baseline::Top).draw(&mut self.framebuf)?;
        Ok(())
    }

    fn draw_line(&mut self, start: Point, end: Point) -> Result<(), Self::Error> {
        Line::new(start, end)
            .into_styled(LINE_STYLE)
            .draw(&mut self.framebuf)?;
        Ok(())
    }

    fn update_with_buffer(&mut self) -> Result<(), Self::Error> {
        let pixel_iterator = self.framebuf.into_iter().map(|p| p.1);

        self.display
            .set_pixels(0, 0, DISPLAY_WIDTH - 1, DISPLAY_HEIGHT, pixel_iterator)?;

        // Clear the frame buffer
        self.framebuf.clear(RgbColor::BLACK)?;
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
