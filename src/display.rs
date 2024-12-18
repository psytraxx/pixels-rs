use core::convert::Infallible;
use display_interface_parallel_gpio::{DisplayError, Generic8BitBus, PGPIO8BitInterface};
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
use esp_hal::delay::Delay;
use esp_hal::gpio::{GpioPin, Level, Output};
use mipidsi::error::InitError;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, Display as MipiDisplay};

use crate::config::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const TEXT_STYLE: MonoTextStyle<Rgb565> = MonoTextStyle::new(&FONT, Rgb565::WHITE);
pub const LCD_PIXELS: usize = (DISPLAY_HEIGHT as usize) * (DISPLAY_WIDTH as usize);

type DisplayBuffer = [Rgb565; LCD_PIXELS];

type MipiDisplayWrapper<'a> = MipiDisplay<
    PGPIO8BitInterface<
        Generic8BitBus<
            Output<'a>,
            Output<'a>,
            Output<'a>,
            Output<'a>,
            Output<'a>,
            Output<'a>,
            Output<'a>,
            Output<'a>,
        >,
        Output<'a>,
        Output<'a>,
    >,
    ST7789,
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
    pub rst: GpioPin<5>,
    pub cs: GpioPin<6>,
    pub dc: GpioPin<7>,
    pub wr: GpioPin<8>,
    pub rd: GpioPin<9>,
    pub backlight: GpioPin<38>,
    pub d0: GpioPin<39>,
    pub d1: GpioPin<40>,
    pub d2: GpioPin<41>,
    pub d3: GpioPin<42>,
    pub d4: GpioPin<45>,
    pub d5: GpioPin<46>,
    pub d6: GpioPin<47>,
    pub d7: GpioPin<48>,
}

impl<'a> Display<'a> {
    pub fn new(p: DisplayPeripherals) -> Result<Self, Error> {
        let mut backlight = Output::new(p.backlight, Level::Low);

        let dc = Output::new(p.dc, Level::Low);
        let mut cs = Output::new(p.cs, Level::Low);
        let rst = Output::new(p.rst, Level::Low);
        let wr = Output::new(p.wr, Level::Low);
        let mut rd = Output::new(p.rd, Level::Low);

        cs.set_low();
        rd.set_high();

        let d0 = Output::new(p.d0, Level::Low);
        let d1 = Output::new(p.d1, Level::Low);
        let d2 = Output::new(p.d2, Level::Low);
        let d3 = Output::new(p.d3, Level::Low);
        let d4 = Output::new(p.d4, Level::Low);
        let d5 = Output::new(p.d5, Level::Low);
        let d6 = Output::new(p.d6, Level::Low);
        let d7 = Output::new(p.d7, Level::Low);

        let bus = Generic8BitBus::new((d0, d1, d2, d3, d4, d5, d6, d7));

        let di = PGPIO8BitInterface::new(bus, dc, wr);

        let mut delay = Delay::new();

        let display = Builder::new(mipidsi::models::ST7789, di)
            .display_size(DISPLAY_HEIGHT, DISPLAY_WIDTH)
            .display_offset((240 - DISPLAY_HEIGHT) / 2, 0)
            .orientation(Orientation::new().rotate(Rotation::Deg270))
            .invert_colors(ColorInversion::Inverted)
            .reset_pin(rst)
            .init(&mut delay)?;

        backlight.set_high();
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
            .into_styled(PrimitiveStyle::with_stroke(RgbColor::GREEN, 2))
            .draw(&mut self.framebuf)?;
        Ok(())
    }

    fn update_with_buffer(&mut self) -> Result<(), Error> {
        // Clear the frame buffer
        self.framebuf.clear(RgbColor::BLACK)?;

        self.display.draw_iter(self.framebuf.into_iter())?;
        //let pixel_iterator = self.framebuf.into_iter().map(|p| p.1);
        //self.display.set_pixels(0, 0, DISPLAY_WIDTH, DISPLAY_HEIGHT, pixel_iterator)?;

        Ok(())
    }
}

/// A clock error
#[derive(Debug)]
pub enum Error {
    DisplayInterface(#[expect(unused, reason = "Never read directly")] DisplayError),
    Infallible,
}

impl From<DisplayError> for Error {
    fn from(error: DisplayError) -> Self {
        Self::DisplayInterface(error)
    }
}

impl From<InitError<Infallible>> for Error {
    fn from(_: InitError<Infallible>) -> Self {
        Self::Infallible
    }
}
impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        Self::Infallible
    }
}
