use core::convert::Infallible;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::framebuffer::{buffer_size, Framebuffer};
use embedded_graphics::geometry::Point;
use embedded_graphics::image::Image;
use embedded_graphics::mono_font::iso_8859_1::FONT_10X20 as FONT;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::raw::{LittleEndian, RawU16};
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle};
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::Drawable;
use embedded_hal_bus::spi::{DeviceError, ExclusiveDevice};
use esp_hal::delay::Delay;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{DMA_CH0, GPIO17, GPIO18, GPIO38, GPIO47, GPIO6, GPIO7, SPI2};
use esp_hal::spi::master::{Config, Spi, SpiDmaBus};
use esp_hal::spi::Error;
use esp_hal::time::Rate;
use esp_println::println;
use mipidsi::interface::{SpiError, SpiInterface};
use mipidsi::models::RM67162;
use mipidsi::options::{Orientation, Rotation};
use mipidsi::{Builder, Display as MipiDisplay};
use static_cell::StaticCell;

pub const DISPLAY_WIDTH: usize = 536;
pub const DISPLAY_HEIGHT: usize = 240;
const DISPLAY_BUFFER_SIZE: usize = 512;
const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
const LINE_STYLE: PrimitiveStyle<Rgb565> = PrimitiveStyle::with_stroke(RgbColor::WHITE, 2);

type DisplayBuffer = Framebuffer<
    Rgb565,
    RawU16,
    LittleEndian,
    DISPLAY_WIDTH,
    DISPLAY_HEIGHT,
    { buffer_size::<Rgb565>(DISPLAY_WIDTH, DISPLAY_HEIGHT) },
>;

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
    framebuf: DisplayBuffer,
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
    pub sck: GPIO47<'static>,
    pub mosi: GPIO18<'static>,
    pub cs: GPIO6<'static>,
    pub pmicen: GPIO38<'static>,
    pub dc: GPIO7<'static>,
    pub rst: GPIO17<'static>,
    pub spi: SPI2<'static>,
    pub dma: DMA_CH0<'static>,
}

impl Display {
    pub fn new(p: DisplayPeripherals) -> Result<Self, DisplayError> {
        // SPI pins
        let sck = Output::new(p.sck, Level::Low, OutputConfig::default());
        let mosi = Output::new(p.mosi, Level::Low, OutputConfig::default());
        let cs = Output::new(p.cs, Level::High, OutputConfig::default());

        let mut pmicen = Output::new(p.pmicen, Level::Low, OutputConfig::default());
        pmicen.set_high();
        println!("PMICEN set high");

        #[allow(clippy::manual_div_ceil)]
        let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
        let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
        let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();

        // Configure SPI
        let spi = Spi::new(p.spi, Config::default().with_frequency(Rate::from_mhz(80)))
            .unwrap()
            .with_sck(sck)
            .with_mosi(mosi)
            .with_dma(p.dma)
            .with_buffers(dma_rx_buf, dma_tx_buf);

        let spi_device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();

        let dc_pin = p.dc;

        static DISPLAY_BUFFER: StaticCell<[u8; DISPLAY_BUFFER_SIZE]> = StaticCell::new();

        let di = SpiInterface::new(
            spi_device,
            Output::new(dc_pin, Level::Low, OutputConfig::default()),
            DISPLAY_BUFFER.init_with(|| [0u8; DISPLAY_BUFFER_SIZE]),
        );

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

        let framebuf = Framebuffer::<
            Rgb565,
            _,
            LittleEndian,
            DISPLAY_WIDTH,
            DISPLAY_HEIGHT,
            { buffer_size::<Rgb565>(DISPLAY_WIDTH, DISPLAY_HEIGHT) },
        >::new();

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
        Image::new(&self.framebuf.as_image(), Point::zero()).draw(&mut self.display)?;

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
