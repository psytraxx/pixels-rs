#include "touch.h"
#include "config.h"
#include "driver/gpio.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static const char *TAG = "touch";

static constexpr uint8_t  CST816X_ADDR = 0x15;
static constexpr uint8_t  REG_GESTURE  = 0x01;
// IrqControl default: EN_TOUCH(6)|EN_CHANGE(5)|EN_MOTION(4) = 0x70
static constexpr uint8_t  IRQ_CTRL_VAL = (1<<6)|(1<<5)|(1<<4);
// MotionMask default: DOUBLE_CLICK(0) = 0x01
static constexpr uint8_t  MOTION_MASK  = 0x01;

bool TouchController::init()
{
    // Touch interrupt pin – input, no pull (CST816x drives it actively low).
    gpio_config_t cfg = {};
    cfg.pin_bit_mask = (1ULL << PIN_TOUCH_INT);
    cfg.mode         = GPIO_MODE_INPUT;
    gpio_config(&cfg);

    // I2C master bus
    i2c_master_bus_config_t bus_cfg = {};
    bus_cfg.clk_source        = I2C_CLK_SRC_DEFAULT;
    bus_cfg.i2c_port          = I2C_NUM_0;
    bus_cfg.scl_io_num        = static_cast<gpio_num_t>(PIN_SCL);
    bus_cfg.sda_io_num        = static_cast<gpio_num_t>(PIN_SDA);
    bus_cfg.glitch_ignore_cnt = 7;
    bus_cfg.flags.enable_internal_pullup = true;
    if (i2c_new_master_bus(&bus_cfg, &bus_) != ESP_OK) {
        ESP_LOGE(TAG, "I2C bus init failed");
        return false;
    }

    // CST816x device
    i2c_device_config_t dev_cfg = {};
    dev_cfg.dev_addr_length = I2C_ADDR_BIT_LEN_7;
    dev_cfg.device_address  = CST816X_ADDR;
    dev_cfg.scl_speed_hz    = 400000;
    if (i2c_master_bus_add_device(bus_, &dev_cfg, &dev_) != ESP_OK) {
        ESP_LOGE(TAG, "CST816x device add failed");
        return false;
    }

    // Brief settle time after power-on
    vTaskDelay(pdMS_TO_TICKS(50));

    // Configure IRQ control and motion mask
    write_reg(0xFA, IRQ_CTRL_VAL);
    write_reg(0xEC, MOTION_MASK);

    ESP_LOGI(TAG, "CST816x ready");
    return true;
}

bool TouchController::read(TouchEvent &out)
{
    // INT pin is active-low; skip the read if no event is pending.
    if (gpio_get_level(static_cast<gpio_num_t>(PIN_TOUCH_INT)) != 0)
        return false;

    uint8_t buf[7] = {};
    if (!read_regs(REG_GESTURE, buf, 7))
        return false;

    // buf[0] = gesture, buf[1] = touch count
    // buf[2] = event_flag(7:6) | x_high(3:0)
    // buf[3] = x_low
    // buf[4] = y_high(3:0)
    // buf[5] = y_low
    uint8_t event_flag = buf[2] >> 6;
    uint16_t x = (uint16_t)((buf[2] & 0x0F) << 8) | buf[3];
    uint16_t y = (uint16_t)((buf[4] & 0x0F) << 8) | buf[5];

    out.x = static_cast<int16_t>(x);
    out.y = static_cast<int16_t>(y);
    out.type = (event_flag == 1) ? TouchEvent::Type::Up
             : (event_flag == 2) ? TouchEvent::Type::Contact
             :                     TouchEvent::Type::Down;
    return true;
}

bool TouchController::write_reg(uint8_t reg, uint8_t val)
{
    uint8_t buf[2] = { reg, val };
    return i2c_master_transmit(dev_, buf, 2, 100) == ESP_OK;
}

bool TouchController::read_regs(uint8_t reg, uint8_t *buf, size_t len)
{
    return i2c_master_transmit_receive(dev_, &reg, 1, buf, len, 100) == ESP_OK;
}
