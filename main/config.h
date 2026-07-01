#pragma once
#include <cstdint>

// Display resolution (logical, after Deg270 rotation)
static constexpr uint16_t DISPLAY_WIDTH  = 536;
static constexpr uint16_t DISPLAY_HEIGHT = 240;

// SPI (display) pin assignments
static constexpr int PIN_SCK  = 47;
static constexpr int PIN_MOSI = 18;
static constexpr int PIN_CS   =  6;
static constexpr int PIN_DC   =  7;
static constexpr int PIN_RST  = 17;

// I2C (touch controller) pin assignments
static constexpr int PIN_SDA  =  3;
static constexpr int PIN_SCL  =  2;

// Touch interrupt (active-low, pulled down by CST816x on touch event)
static constexpr int PIN_TOUCH_INT = 21;

// Power management IC enable (drive HIGH to power on display)
static constexpr int PIN_PMICEN = 38;

// SPI bus frequency for the RM67162 display
static constexpr int SPI_FREQ_HZ = 80 * 1000 * 1000;

// DMA transfer ceiling (bytes): each tile-row batch fits comfortably in 32 KB
static constexpr int SPI_DMA_MAX_BYTES = 32768;

// Tile dimensions for dirty-tracking
static constexpr uint16_t TILE_SIZE  = 16;
static constexpr uint16_t TILES_X    = (DISPLAY_WIDTH  + TILE_SIZE - 1) / TILE_SIZE; // 34
static constexpr uint16_t TILES_Y    = (DISPLAY_HEIGHT + TILE_SIZE - 1) / TILE_SIZE; // 15
static constexpr uint16_t TOTAL_TILES = TILES_X * TILES_Y;                            // 510

// Framebuffer size in pixels
static constexpr uint32_t FB_PIXELS = (uint32_t)DISPLAY_WIDTH * DISPLAY_HEIGHT;
