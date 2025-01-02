use defmt::info;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_hal::delay::DelayNs;
use mipidsi::{
    dcs::{InterfaceExt, SetAddressMode},
    interface::Interface,
    models::Model,
    options::{ModelOptions, Rotation},
};

use crate::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

//https://forum.lvgl.io/t/periodic-wrong-pixels-using-dma-spi-esp32/9049/6
//https://github.com/Xinyuan-LilyGO/LilyGo-AMOLED-Series/blob/8c72b786373fbaef46ce35a6db924d6e16a0c3ec/src/LilyGo_AMOLED.cpp#L806

/*// LILYGO 1.91 Inch AMOLED(RM67162) S3R8
// https://www.lilygo.cc/products/t-display-s3-amoled
static const DisplayConfigure_t RM67162_AMOLED_SPI  = {
    18,//BOARD_DISP_DATA0,          //MOSI
    7,//BOARD_DISP_DATA1,           //DC
    -1,//BOARD_DISP_DATA2,
    -1,//BOARD_DISP_DATA3,
    47,//BOARD_DISP_SCK,            //SCK
    6,//BOARD_DISP_CS,              //CS
    BOARD_NONE_PIN,//DC
    17,//BOARD_DISP_RESET,          //RST
    9, //BOARD_DISP_TE,
    8, //command bit
    24,//address bit
    40000000,
    (lcd_cmd_t *)rm67162_spi_cmd,
    RM67162_INIT_SPI_SEQUENCE_LENGTH,
    RM67162_WIDTH,//width
    RM67162_HEIGHT,//height
    0,//frameBufferSize
    false //fullRefresh
}; */

/*typedef struct __DisplayConfigure {
    int d0;
    int d1;
    int d2;
    int d3;
    int sck;
    int cs;
    int dc;
    int rst;
    int te;
    uint8_t cmdBit;
    uint8_t addBit;
    int  freq;
    lcd_cmd_t *initSequence;
    uint32_t initSize;
    uint16_t width;
    uint16_t height;
    uint32_t frameBufferSize;
    bool fullRefresh;
} DisplayConfigure_t; */

/// Commands used to initialize the RM67162 AMOLED display controller
/// Derived from LilyGo reference implementation

// Define a structure for the LCD command
struct LcdCommand<'a> {
    /// Command address/opcode
    addr: u8,
    /// Command parameters
    params: &'a [u8],
    /// Optional delay after command in milliseconds
    delay_after: Option<u32>,
}

/// Memory Access Control (MADCTL) register bits
const RM67162_MADCTL_MY: i32 = 0x80; // Row address order
const RM67162_MADCTL_MX: i32 = 0x40; // Column address order
const RM67162_MADCTL_MV: i32 = 0x20; // Row/Column exchange
const RM67162_MADCTL_RGB: i32 = 0x00; // RGB color order

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
    }, // Set brightness (175 in decimal)
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

        delay.delay_us(200_000);

        // Send initialization commands
        for cmd in AMOLED_INIT_CMDS {
            // Write the command address
            di.write_raw(cmd.addr, cmd.params).unwrap();

            // Apply delay if specified
            if let Some(ms) = cmd.delay_after {
                delay.delay_ms(ms);
            }
        }

        let test = madctl_from_options(options);
        di.write_raw(LCD_CMD_MADCTL, &[test as u8]).unwrap();

        info!("Display initialized");
        Ok(madctl)
    }
}

/// Configures display orientation based on rotation settings
fn madctl_from_options(options: &ModelOptions) -> i32 {
    match options.orientation.rotation {
        Rotation::Deg0 => RM67162_MADCTL_RGB, //ok
        Rotation::Deg180 => RM67162_MADCTL_MX | RM67162_MADCTL_MY | RM67162_MADCTL_RGB,
        Rotation::Deg270 => RM67162_MADCTL_MX | RM67162_MADCTL_MV | RM67162_MADCTL_RGB,
        Rotation::Deg90 => RM67162_MADCTL_MV | RM67162_MADCTL_MY | RM67162_MADCTL_RGB,
    }
}
