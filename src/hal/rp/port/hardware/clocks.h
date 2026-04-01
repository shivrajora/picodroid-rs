/*
 * hardware/clocks.h stub – provides clock_get_hz() declaration.
 * The implementation in pico_shim_rp2040.c / pico_shim_rp2350.c
 * returns the hard-coded CPU frequency (no runtime clock measurement).
 */
#ifndef HARDWARE_CLOCKS_H
#define HARDWARE_CLOCKS_H
#include <stdint.h>

/* Clock identifiers used by port.c (only clk_sys is actually called) */
typedef enum { clk_gpout0 = 0, clk_gpout1, clk_gpout2, clk_gpout3,
               clk_ref = 4, clk_sys = 5 } clock_handle_t;

uint32_t clock_get_hz(uint32_t clk_id);

#endif /* HARDWARE_CLOCKS_H */
