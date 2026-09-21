// PicoDisplay2-W board — RP2350 + Pimoroni Pico Display Pack 2.0 on a
// Raspberry Pi Pico 2 W (CYW43439 WiFi).
// All display/button config is in board.toml. The pack's RGB LED (GP6/GP7/GP8)
// is app-driven through picodroid.pio PWM. WiFi pin numbers live in
// src/hal/rp/port/cyw43_configport.h (CYW43_PIN_WL_*), consumed by the
// vendored C driver — they are not duplicated here.
