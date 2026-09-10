// PicoTouchKit board — 52Pi EP-0172 carrier (ST7796 + GT911) with a Pimoroni
// Pico Plus 2 W (RP2350B + Raspberry Pi RM2) in its socket.
//
// All display, touch and button config is in board.toml. WiFi pin numbers live
// in src/hal/rp/port/cyw43_configport.h (CYW43_PIN_WL_*), consumed by the
// vendored C driver — they are not duplicated here, and the RM2 sits on the
// same pins a Pico 2 W does.
