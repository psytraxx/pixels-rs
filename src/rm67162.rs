use defmt::info;
use display_interface::{DataFormat, WriteOnlyDataCommand};
use embedded_graphics_core::{pixelcolor::Rgb565, prelude::IntoStorage};
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use mipidsi::{
    dcs::{Dcs, SetAddressMode, SoftReset, WriteMemoryStart},
    error::{Error, InitError},
    models::Model,
    options::ModelOptions,
};

// Define a structure for the LCD command
struct LcdCommand<'a> {
    addr: u8,                 // Command address
    params: &'a [u8],         // Command parameters
    delay_after: Option<u32>, // Delay in milliseconds after sending the command
}

// AMOLED initialization commands
const AMOLED_INIT_CMDS: &[LcdCommand] = &[
    /*const lcd_cmd_t rm67162_spi_cmd[RM67162_INIT_SPI_SEQUENCE_LENGTH] = {
        {0xFE, {0x04}, 0x01}, //SET APGE3
        {0x6A, {0x00}, 0x01},
        {0xFE, {0x05}, 0x01}, //SET APGE4
        {0xFE, {0x07}, 0x01}, //SET APGE6
        {0x07, {0x4F}, 0x01},
        {0xFE, {0x01}, 0x01}, //SET APGE0
        {0x2A, {0x02}, 0x01},
        {0x2B, {0x73}, 0x01},
        {0xFE, {0x0A}, 0x01}, //SET APGE9
        {0x29, {0x10}, 0x01},
        {0xFE, {0x00}, 0x01},
        {0x51, {AMOLED_DEFAULT_BRIGHTNESS}, 0x01},
        {0x53, {0x20}, 0x01},
        {0x35, {0x00}, 0x01},

        {0x3A, {0x75}, 0x01}, // Interface Pixel Format 16bit/pixel
        {0xC4, {0x80}, 0x01},
        {0x11, {0x00}, 0x01 | 0x80},
        {0x29, {0x00}, 0x01 | 0x80},
    }; */
    LcdCommand {
        addr: 0xFE,
        params: &[0x04],
        delay_after: None,
    }, // SET APGE3
    LcdCommand {
        addr: 0x6A,
        params: &[0x00],
        delay_after: None,
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x05],
        delay_after: None,
    }, // SET APGE4
    LcdCommand {
        addr: 0xFE,
        params: &[0x07],
        delay_after: None,
    }, // SET APGE6
    LcdCommand {
        addr: 0x07,
        params: &[0x4F],
        delay_after: None,
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x01],
        delay_after: None,
    }, // SET APGE0
    LcdCommand {
        addr: 0x2A,
        params: &[0x02],
        delay_after: None,
    },
    LcdCommand {
        addr: 0x2B,
        params: &[0x73],
        delay_after: None,
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x0A],
        delay_after: None,
    }, // SET APGE9
    LcdCommand {
        addr: 0x29,
        params: &[0x10],
        delay_after: None,
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x00],
        delay_after: None,
    },
    LcdCommand {
        addr: 0x51,
        params: &[0xff],
        delay_after: None,
    }, // Set brightness (255 in decimal)
    LcdCommand {
        addr: 0x53,
        params: &[0x20],
        delay_after: None,
    },
    LcdCommand {
        addr: 0x35,
        params: &[0x00],
        delay_after: None,
    },
    LcdCommand {
        addr: 0x3A,
        params: &[0x75],
        delay_after: None,
    }, // Interface Pixel Format 16bit/pixel
    LcdCommand {
        addr: 0xC4,
        params: &[0x80],
        delay_after: None,
    },
    LcdCommand {
        addr: 0x11,
        params: &[0x00],
        delay_after: Some(120),
    }, // Sleep Out with 120ms delay
    LcdCommand {
        addr: 0x29,
        params: &[0x00],
        delay_after: Some(120),
    }, // Display ON with 120ms delay
];

/// RM67162 display in Rgb565 color mode.
pub struct RM67162;

impl Model for RM67162 {
    type ColorFormat = Rgb565;
    const FRAMEBUFFER_SIZE: (u16, u16) = (240, 536);

    fn init<RST, DELAY, DI>(
        &mut self,
        dcs: &mut Dcs<DI>,
        delay: &mut DELAY,
        options: &ModelOptions,
        rst: &mut Option<RST>,
    ) -> Result<SetAddressMode, InitError<RST::Error>>
    where
        RST: OutputPin,
        DELAY: DelayNs,
        DI: WriteOnlyDataCommand,
    {
        let madctl = SetAddressMode::from(options);

        match rst {
            Some(ref mut rst) => self.hard_reset(rst, delay)?,
            None => dcs.write_command(SoftReset)?,
        }

        delay.delay_us(200_000);

        // Send initialization commands
        for cmd in AMOLED_INIT_CMDS {
            // Write the command address
            dcs.write_raw(cmd.addr, cmd.params).unwrap();

            // Apply delay if specified
            if let Some(ms) = cmd.delay_after {
                delay.delay_ms(ms);
            }
        }

        info!("Display initialized");
        Ok(madctl)
    }

    fn write_pixels<DI, I>(&mut self, dcs: &mut Dcs<DI>, colors: I) -> Result<(), Error>
    where
        DI: WriteOnlyDataCommand,
        I: IntoIterator<Item = Self::ColorFormat>,
    {
        dcs.write_command(WriteMemoryStart)?;

        let mut iter = colors.into_iter().map(Rgb565::into_storage);

        let buf = DataFormat::U16BEIter(&mut iter);
        dcs.di.send_data(buf)?;
        Ok(())
    }
}
