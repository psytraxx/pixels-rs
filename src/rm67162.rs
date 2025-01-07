use defmt::info;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_hal::delay::DelayNs;
use mipidsi::{
    dcs::{ExitSleepMode, InterfaceExt, SetAddressMode, SetDisplayOn},
    interface::Interface,
    models::Model,
    options::{ModelOptions, Rotation},
};

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

// LILYGO 1.91 Inch AMOLED(RM67162) S3R8
// https://github.com/Xinyuan-LilyGO/LilyGo-AMOLED-Series/blob/8c72b786373fbaef46ce35a6db924d6e16a0c3ec/src/LilyGo_AMOLED.cpp#L806
// https://www.lilygo.cc/products/t-display-s3-amoled

// Define a structure for the LCD command
struct LcdCommand<'a> {
    /// Command address/opcode
    addr: u8,
    /// Command parameters
    params: &'a [u8],
}

/// Memory Access Control (MADCTL) register bits
const RM67162_MADCTL_MY: u8 = 0x80; // Row address order
const RM67162_MADCTL_MX: u8 = 0x40; // Column address order
const RM67162_MADCTL_MV: u8 = 0x20; // Row/Column exchange
const RM67162_MADCTL_RGB: u8 = 0x00; // RGB color order

/// Memory Data Access Control register
const LCD_CMD_MADCTL: u8 = 0x36;

/// Display initialization command sequence
/// Sets up display parameters, power settings and enables the display
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
        {0x11, {0x00}, 0x01 | 0x80}, // Sleep Out
        {0x29, {0x00}, 0x01 | 0x80}, // Display ON
    }; */
    LcdCommand {
        addr: 0xFE,
        params: &[0x04],
    }, // SET APGE3
    LcdCommand {
        addr: 0x6A,
        params: &[0x00],
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x05],
    }, // SET APGE4
    LcdCommand {
        addr: 0xFE,
        params: &[0x07],
    }, // SET APGE6
    LcdCommand {
        addr: 0x07,
        params: &[0x4F],
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x01],
    }, // SET APGE0
    LcdCommand {
        addr: 0x2A,
        params: &[0x02],
        //Set column start address
    },
    LcdCommand {
        addr: 0x2B,
        params: &[0x73],
        //Set row start address
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x0A],
    }, // SET APGE9
    LcdCommand {
        addr: 0x29,
        params: &[0x10],
        // display on
    },
    LcdCommand {
        addr: 0xFE,
        params: &[0x00],
        //CMD Mode Switch to User Command Set
    },
    LcdCommand {
        addr: 0x51,
        params: &[0xaf],
    }, // Set brightness (175 in decimal)
    LcdCommand {
        addr: 0x53,
        params: &[0x20],
        //Write CTRL display
    },
    LcdCommand {
        addr: 0x35,
        params: &[0x00],
        // set Tearing Effect Line on
    },
    LcdCommand {
        addr: 0x3A,
        params: &[0x75],
    }, // Interface Pixel Format 16bit/pixel
    LcdCommand {
        addr: 0xC4,
        params: &[0x80],
        // set_DSPI Mode to SPI_WRAM
    },
];

/// RM67162 AMOLED display driver implementation
/// Supports:
/// - 16-bit RGB565 color
/// - 240x536 resolution
/// - SPI interface with DMA
pub struct RM67162;

impl Model for RM67162 {
    type ColorFormat = Rgb565;
    const FRAMEBUFFER_SIZE: (u16, u16) = (DISPLAY_WIDTH, DISPLAY_HEIGHT);

    fn init<DELAY, DI>(
        &mut self,
        di: &mut DI,
        delay: &mut DELAY,
        options: &ModelOptions,
    ) -> Result<SetAddressMode, DI::Error>
    where
        DELAY: DelayNs,
        DI: Interface,
    {
        let madctl = SetAddressMode::from(options);

        // Send initialization commands
        for cmd in AMOLED_INIT_CMDS {
            // Write the command address
            di.write_raw(cmd.addr, cmd.params).unwrap();
        }

        // Manually set memory access control - the default doesn't work
        let custom_madctl = madctl_from_options(options);
        di.write_raw(LCD_CMD_MADCTL, &[custom_madctl]).unwrap();

        di.write_command(ExitSleepMode)?; // turn off sleep
        delay.delay_us(120_000);

        di.write_command(SetDisplayOn)?; // turn on display

        info!("Display initialized");
        Ok(madctl)
    }
}

/// Configures display orientation based on rotation settings
fn madctl_from_options(options: &ModelOptions) -> u8 {
    match options.orientation.rotation {
        Rotation::Deg0 => RM67162_MADCTL_RGB, //ok
        Rotation::Deg180 => RM67162_MADCTL_MX | RM67162_MADCTL_MY | RM67162_MADCTL_RGB,
        Rotation::Deg270 => RM67162_MADCTL_MX | RM67162_MADCTL_MV | RM67162_MADCTL_RGB,
        Rotation::Deg90 => RM67162_MADCTL_MV | RM67162_MADCTL_MY | RM67162_MADCTL_RGB,
    }
}
