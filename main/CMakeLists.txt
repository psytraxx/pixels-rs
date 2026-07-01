idf_component_register(
    SRCS
        "app_main.cpp"
        "display.cpp"
        "touch.cpp"
    INCLUDE_DIRS "."
    REQUIRES
        driver
        esp_timer
        freertos
        log
        spi_flash
)
