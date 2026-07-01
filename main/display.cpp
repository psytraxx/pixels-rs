#include "display.h"
#include "font8x8.h"
#include "driver/gpio.h"
#include "driver/spi_master.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include <cstring>
#include <cstdlib>
#include <algorithm>

static const char *TAG = "display";

// DC pin ISR-safe setter used by the SPI pre-transfer callback.
static void IRAM_ATTR spi_pre_cb(spi_transaction_t *t)
{
    gpio_set_level(static_cast<gpio_num_t>(PIN_DC), (int)(intptr_t)t->user);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

bool Display::init()
{
    // --- PMICEN: power-management IC enable -----------------------------------
    gpio_config_t pwr = {};
    pwr.pin_bit_mask = (1ULL << PIN_PMICEN);
    pwr.mode         = GPIO_MODE_OUTPUT;
    gpio_config(&pwr);
    gpio_set_level(static_cast<gpio_num_t>(PIN_PMICEN), 1);

    // --- DC / RST: output, not managed by SPI driver --------------------------
    gpio_config_t io = {};
    io.pin_bit_mask = (1ULL << PIN_DC) | (1ULL << PIN_RST);
    io.mode         = GPIO_MODE_OUTPUT;
    gpio_config(&io);
    gpio_set_level(static_cast<gpio_num_t>(PIN_DC),  1);
    gpio_set_level(static_cast<gpio_num_t>(PIN_RST), 1);

    // --- SPI bus --------------------------------------------------------------
    spi_bus_config_t bus = {};
    bus.mosi_io_num     = PIN_MOSI;
    bus.miso_io_num     = -1;
    bus.sclk_io_num     = PIN_SCK;
    bus.quadwp_io_num   = -1;
    bus.quadhd_io_num   = -1;
    bus.max_transfer_sz = SPI_DMA_MAX_BYTES;
    ESP_ERROR_CHECK(spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO));

    spi_device_interface_config_t dev = {};
    dev.mode           = 0;
    dev.clock_speed_hz = SPI_FREQ_HZ;
    dev.spics_io_num   = PIN_CS;
    dev.queue_size     = 7;
    dev.pre_cb         = spi_pre_cb;
    dev.flags          = SPI_DEVICE_NO_DUMMY;
    ESP_ERROR_CHECK(spi_bus_add_device(SPI2_HOST, &dev, &spi_));

    // --- PSRAM framebuffers ---------------------------------------------------
    front_buf_ = static_cast<uint16_t*>(
        heap_caps_malloc(FB_PIXELS * sizeof(uint16_t), MALLOC_CAP_SPIRAM));
    back_buf_  = static_cast<uint16_t*>(
        heap_caps_malloc(FB_PIXELS * sizeof(uint16_t), MALLOC_CAP_SPIRAM));
    // DMA staging buffer in internal SRAM: max tile-row batch = W × TILE_SIZE × 2 bytes.
    dma_buf_ = static_cast<uint16_t*>(
        heap_caps_malloc(SPI_DMA_MAX_BYTES, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL));
    if (!front_buf_ || !back_buf_ || !dma_buf_) {
        ESP_LOGE(TAG, "Framebuffer allocation failed");
        return false;
    }
    memset(front_buf_, 0, FB_PIXELS * sizeof(uint16_t));
    memset(back_buf_,  0, FB_PIXELS * sizeof(uint16_t));

    // --- RM67162 hardware reset + init sequence -------------------------------
    gpio_set_level(static_cast<gpio_num_t>(PIN_RST), 0);
    vTaskDelay(pdMS_TO_TICKS(10));
    gpio_set_level(static_cast<gpio_num_t>(PIN_RST), 1);
    vTaskDelay(pdMS_TO_TICKS(120));

    rm67162_init_sequence();

    ESP_LOGI(TAG, "Display ready (%u x %u)", DISPLAY_WIDTH, DISPLAY_HEIGHT);
    return true;
}

// ---------------------------------------------------------------------------
// RM67162 init sequence (ported from mipidsi Rust driver)
// ---------------------------------------------------------------------------

void Display::rm67162_init_sequence()
{
    // Extended register pages
    spi_write_cmd(0xFE); spi_write_data("\x04", 1);
    spi_write_cmd(0x6A); spi_write_data("\x00", 1);
    spi_write_cmd(0xFE); spi_write_data("\x05", 1);
    spi_write_cmd(0xFE); spi_write_data("\x07", 1);
    spi_write_cmd(0x07); spi_write_data("\x4F", 1);
    spi_write_cmd(0xFE); spi_write_data("\x01", 1);
    spi_write_cmd(0x2A); spi_write_data("\x02", 1);  // in extended page 1, not CASET
    spi_write_cmd(0x2B); spi_write_data("\x73", 1);
    spi_write_cmd(0xFE); spi_write_data("\x0A", 1);
    spi_write_cmd(0x29); spi_write_data("\x10", 1);
    // Back to user register page 0
    spi_write_cmd(0xFE); spi_write_data("\x00", 1);

    // Brightness + display control
    spi_write_cmd(0x51); spi_write_data("\xAF", 1); // display brightness
    spi_write_cmd(0x53); spi_write_data("\x20", 1); // write CTRL display
    spi_write_cmd(0x35); spi_write_data("\x00", 1); // tearing effect ON

    // Pixel format: RGB565 (0x55 = 16-bit on both MCU & RGB interface)
    spi_write_cmd(0x3A); spi_write_data("\x55", 1);

    // Enable SRAM access via SPI
    spi_write_cmd(0xC4); spi_write_data("\x80", 1);

    // MADCTL: Deg270, RGB order
    // MY=1 (bit7), MX=0 (bit6), MV=1 (bit5) → 0xA0
    spi_write_cmd(0x36); spi_write_data("\xA0", 1);

    // Display inversion off (INVOFF)
    spi_write_cmd(0x20);

    // Exit sleep
    spi_write_cmd(0x11);
    vTaskDelay(pdMS_TO_TICKS(120));

    // Display on
    spi_write_cmd(0x29);
    vTaskDelay(pdMS_TO_TICKS(20));
}

// ---------------------------------------------------------------------------
// SPI helpers
// ---------------------------------------------------------------------------

void Display::spi_write_cmd(uint8_t cmd)
{
    spi_transaction_t t = {};
    t.length    = 8;
    t.tx_buffer = &cmd;
    t.user      = reinterpret_cast<void*>(0); // DC=0 (command)
    spi_device_polling_transmit(spi_, &t);
}

void Display::spi_write_data(const void *data, size_t len)
{
    if (!len) return;
    spi_transaction_t t = {};
    t.length    = len * 8;
    t.tx_buffer = data;
    t.user      = reinterpret_cast<void*>(1); // DC=1 (data)
    spi_device_polling_transmit(spi_, &t);
}

void Display::spi_write_pixels(const uint16_t *pixels, size_t count)
{
    // DC=1, chunked DMA (ESP-IDF automatically uses DMA for the SPI2 host).
    const uint8_t *ptr = reinterpret_cast<const uint8_t*>(pixels);
    size_t remaining   = count * sizeof(uint16_t);

    // Acquire the bus for the full batch to keep CS asserted between chunks.
    spi_device_acquire_bus(spi_, portMAX_DELAY);

    while (remaining) {
        size_t chunk = std::min<size_t>(remaining, SPI_DMA_MAX_BYTES);
        spi_transaction_t t = {};
        t.length    = chunk * 8;
        t.tx_buffer = ptr;
        t.user      = reinterpret_cast<void*>(1); // DC=1
        spi_device_transmit(spi_, &t);             // blocking DMA
        ptr       += chunk;
        remaining -= chunk;
    }

    spi_device_release_bus(spi_);
}

void Display::set_window(uint16_t x1, uint16_t y1, uint16_t x2, uint16_t y2)
{
    uint8_t col[4] = { uint8_t(x1 >> 8), uint8_t(x1), uint8_t(x2 >> 8), uint8_t(x2) };
    uint8_t row[4] = { uint8_t(y1 >> 8), uint8_t(y1), uint8_t(y2 >> 8), uint8_t(y2) };
    spi_write_cmd(0x2A); spi_write_data(col, 4);
    spi_write_cmd(0x2B); spi_write_data(row, 4);
    spi_write_cmd(0x2C); // RAMWR – data bytes follow (no DC toggle between cmd and data here)
}

// ---------------------------------------------------------------------------
// Tile tracking
// ---------------------------------------------------------------------------

void Display::mark_rect(int x1, int y1, int x2, int y2)
{
    int tx1 = std::max(x1, 0) / TILE_SIZE;
    int ty1 = std::max(y1, 0) / TILE_SIZE;
    int tx2 = std::min(x2, (int)DISPLAY_WIDTH  - 1) / TILE_SIZE;
    int ty2 = std::min(y2, (int)DISPLAY_HEIGHT - 1) / TILE_SIZE;
    if (tx2 >= TILES_X) tx2 = TILES_X - 1;
    if (ty2 >= TILES_Y) ty2 = TILES_Y - 1;

    for (int ty = ty1; ty <= ty2; ty++)
        for (int tx = tx1; tx <= tx2; tx++)
            current_tiles_[ty * TILES_X + tx] = true;
}

// ---------------------------------------------------------------------------
// Frame operations
// ---------------------------------------------------------------------------

void Display::clear_buffer()
{
    // Clear only the tiles that were dirty two frames ago (stored in prev_tiles_).
    for (int idx = 0; idx < TOTAL_TILES; idx++) {
        if (!prev_tiles_[idx]) continue;
        int tx = idx % TILES_X;
        int ty = idx / TILES_X;
        int xs = tx * TILE_SIZE;
        int ys = ty * TILE_SIZE;
        int xe = std::min(xs + TILE_SIZE, (int)DISPLAY_WIDTH);
        int ye = std::min(ys + TILE_SIZE, (int)DISPLAY_HEIGHT);
        for (int y = ys; y < ye; y++)
            memset(&back_buf_[y * DISPLAY_WIDTH + xs], 0,
                   (xe - xs) * sizeof(uint16_t));
    }
}

void Display::flush()
{
    // Swap back → front.
    std::swap(front_buf_, back_buf_);

    // For each tile row: batch horizontally-adjacent dirty tiles into one DMA transfer.
    for (int ty = 0; ty < TILES_Y; ty++) {
        int batch_start = -1;

        for (int tx = 0; tx <= TILES_X; tx++) {
            int idx     = ty * TILES_X + tx;
            bool dirty  = (tx < TILES_X) &&
                          (current_tiles_[idx] || prev_tiles_[idx]);

            if (dirty) {
                if (batch_start < 0) batch_start = tx;
            } else if (batch_start >= 0) {
                // Flush [batch_start, tx)
                uint16_t x1 = batch_start * TILE_SIZE;
                uint16_t x2 = std::min((int)(tx * TILE_SIZE), (int)DISPLAY_WIDTH) - 1;
                uint16_t y1 = ty * TILE_SIZE;
                uint16_t y2 = std::min((int)((ty + 1) * TILE_SIZE), (int)DISPLAY_HEIGHT) - 1;
                uint16_t w  = x2 - x1 + 1;
                uint16_t h  = y2 - y1 + 1;

                set_window(x1, y1, x2, y2);

                // Copy tile-row batch into the pre-allocated DMA staging buffer
                // (internal SRAM, DMA-capable) to avoid per-frame heap operations.
                size_t pixels = (size_t)w * h;
                for (uint16_t row = 0; row < h; row++) {
                    const uint16_t *src = &front_buf_[(y1 + row) * DISPLAY_WIDTH + x1];
                    memcpy(&dma_buf_[row * w], src, w * sizeof(uint16_t));
                }
                spi_write_pixels(dma_buf_, pixels);

                batch_start = -1;
            }
        }
    }

    // Rotate tile sets.
    memcpy(prev_tiles_, current_tiles_, sizeof(current_tiles_));
    memset(current_tiles_, 0, sizeof(current_tiles_));
}

// ---------------------------------------------------------------------------
// Drawing primitives
// ---------------------------------------------------------------------------

void Display::draw_point(int x, int y, uint16_t color)
{
    if (x < 0 || x >= DISPLAY_WIDTH || y < 0 || y >= DISPLAY_HEIGHT) return;
    back_buf_[y * DISPLAY_WIDTH + x] = color;
    mark_rect(x, y, x, y);
}

void Display::draw_rect3x3(int cx, int cy, uint16_t color)
{
    int x1 = std::max(cx - 1, 0);
    int y1 = std::max(cy - 1, 0);
    int x2 = std::min(cx + 1, (int)DISPLAY_WIDTH  - 1);
    int y2 = std::min(cy + 1, (int)DISPLAY_HEIGHT - 1);
    for (int y = y1; y <= y2; y++)
        for (int x = x1; x <= x2; x++)
            back_buf_[y * DISPLAY_WIDTH + x] = color;
    mark_rect(x1, y1, x2, y2);
}

void Display::bresenham(int x0, int y0, int x1, int y1, uint16_t color)
{
    int dx =  abs(x1 - x0), sx = x0 < x1 ? 1 : -1;
    int dy = -abs(y1 - y0), sy = y0 < y1 ? 1 : -1;
    int err = dx + dy;
    while (true) {
        if (x0 >= 0 && x0 < DISPLAY_WIDTH && y0 >= 0 && y0 < DISPLAY_HEIGHT)
            back_buf_[y0 * DISPLAY_WIDTH + x0] = color;
        if (x0 == x1 && y0 == y1) break;
        int e2 = 2 * err;
        if (e2 >= dy) { err += dy; x0 += sx; }
        if (e2 <= dx) { err += dx; y0 += sy; }
    }
}

void Display::draw_line(int x0, int y0, int x1, int y1, uint16_t color)
{
    // Two Bresenham passes offset perpendicular to the line direction → ~2px stroke.
    bresenham(x0, y0, x1, y1, color);
    int dx = abs(x1 - x0), dy = abs(y1 - y0);
    if (dx >= dy)
        bresenham(x0, y0 + 1, x1, y1 + 1, color); // more horizontal → offset vertically
    else
        bresenham(x0 + 1, y0, x1 + 1, y1, color); // more vertical   → offset horizontally

    int minx = std::max(std::min(x0, x1) - 1, 0);
    int miny = std::max(std::min(y0, y1) - 1, 0);
    int maxx = std::min(std::max(x0, x1) + 2, (int)DISPLAY_WIDTH  - 1);
    int maxy = std::min(std::max(y0, y1) + 2, (int)DISPLAY_HEIGHT - 1);
    mark_rect(minx, miny, maxx, maxy);
}

void Display::draw_string(int x, int y, const char *s, uint16_t fg, uint16_t bg, int scale)
{
    font_draw_string(back_buf_, x, y, s, fg, bg, scale,
                     DISPLAY_WIDTH, DISPLAY_HEIGHT);
    int w = strlen(s) * 8 * scale;
    int h = 8 * scale;
    mark_rect(x, y, x + w - 1, y + h - 1);
}
