#pragma once
#include <cstdint>
#include "config.h"
#include "driver/spi_master.h"

// RGB565 color constants stored byte-swapped for direct SPI transmission.
// ESP32 is little-endian; display expects big-endian 16-bit pixels.
// __builtin_bswap16(native_rgb565) gives the byte-swapped value to store.
namespace Color {
    static constexpr uint16_t BLACK   = 0x0000;
    static constexpr uint16_t WHITE   = 0xFFFF;
    static constexpr uint16_t RED     = 0x00F8; // native 0xF800
    static constexpr uint16_t GREEN   = 0xE007; // native 0x07E0
    static constexpr uint16_t BLUE    = 0x1F00; // native 0x001F
    static constexpr uint16_t YELLOW  = 0xE0FF; // native 0xFFE0
    static constexpr uint16_t CYAN    = 0xFF07; // native 0x07FF
    static constexpr uint16_t MAGENTA = 0x1FF8; // native 0xF81F
}

// Convert R5/G6/B5 to byte-swapped RGB565 for direct SPI transmission.
static inline constexpr uint16_t rgb565(uint8_t r, uint8_t g, uint8_t b) {
    uint16_t native = (uint16_t)((r & 0x1F) << 11) | ((g & 0x3F) << 5) | (b & 0x1F);
    return __builtin_bswap16(native);
}

struct Display {
    // Initialise hardware: SPI bus, GPIO (DC/RST/PMICEN), RM67162 sequence.
    // Allocates both framebuffers from PSRAM.
    // Returns false on allocation failure.
    bool init();

    // Draw primitives into the back-buffer (call between clear_buffer and flush).
    void clear_buffer();                                    // erase prev-dirty tiles
    void draw_point(int x, int y, uint16_t color);         // single pixel
    void draw_rect3x3(int cx, int cy, uint16_t color);     // 3×3 filled square (particle)
    void draw_line(int x0, int y0, int x1, int y1, uint16_t color); // 2-px Bresenham
    void draw_string(int x, int y, const char *s, uint16_t fg = Color::WHITE,
                     uint16_t bg = Color::BLACK, int scale = 2);

    // Swap back→front and DMA-flush only the dirty tiles to the display.
    void flush();

    // Mark a rectangle of tiles dirty (called internally by draw_* helpers).
    void mark_rect(int x1, int y1, int x2, int y2);

private:
    spi_device_handle_t spi_  = nullptr;
    uint16_t *front_buf_      = nullptr;  // PSRAM
    uint16_t *back_buf_       = nullptr;  // PSRAM
    // Pre-allocated DMA staging buffer (internal SRAM, DMA-capable).
    // Used to linearise sub-width tile rows before the DMA transfer.
    // Max batch: DISPLAY_WIDTH × TILE_SIZE × 2 bytes ≈ 17 KB → 32 KB is ample.
    uint16_t *dma_buf_        = nullptr;
    bool current_tiles_[TOTAL_TILES] = {};
    bool prev_tiles_   [TOTAL_TILES] = {};

    void rm67162_init_sequence();
    void spi_write_cmd (uint8_t cmd);
    void spi_write_data(const void *data, size_t len);
    void spi_write_pixels(const uint16_t *pixels, size_t count);
    void set_window(uint16_t x1, uint16_t y1, uint16_t x2, uint16_t y2);
    void bresenham(int x0, int y0, int x1, int y1, uint16_t color);
};
