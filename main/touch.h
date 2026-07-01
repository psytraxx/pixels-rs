#pragma once
#include <cstdint>
#include "driver/i2c_master.h"

struct TouchEvent {
    int16_t x, y;
    enum class Type : uint8_t { Down = 0, Up = 1, Contact = 2 } type;
};

struct TouchController {
    // Initialise I2C bus and CST816x touch controller.
    bool init();

    // Poll for a touch event.  Returns false when no event is pending.
    // The CST816x pulls PIN_TOUCH_INT low when data is ready.
    bool read(TouchEvent &out);

private:
    i2c_master_bus_handle_t bus_ = nullptr;
    i2c_master_dev_handle_t dev_ = nullptr;

    bool write_reg(uint8_t reg, uint8_t val);
    bool read_regs(uint8_t reg, uint8_t *buf, size_t len);
};
