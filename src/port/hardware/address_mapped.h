/*
 * Minimal hardware/address_mapped.h stub for FreeRTOS RP2350 port compilation.
 *
 * The real pico-sdk header defines io_rw_32, io_ro_32, etc.  The RP2350
 * portasm.c includes this header but doesn't use any of its types directly.
 */

#ifndef HARDWARE_ADDRESS_MAPPED_H
#define HARDWARE_ADDRESS_MAPPED_H

#include <stdint.h>

/* pico-sdk volatile register access typedefs */
typedef volatile uint32_t io_rw_32;
typedef const volatile uint32_t io_ro_32;
typedef volatile uint32_t io_wo_32;

#endif /* HARDWARE_ADDRESS_MAPPED_H */
