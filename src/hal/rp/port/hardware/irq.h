/*
 * hardware/irq.h stub – declares IRQ management functions used by port.c
 * when configNUMBER_OF_CORES > 1. Implementations are in pico_shim_rp2040.c
 * and pico_shim_rp2350.c.
 */
#ifndef HARDWARE_IRQ_H
#define HARDWARE_IRQ_H
#include <stdint.h>

void irq_set_priority(uint32_t num, uint8_t hardware_priority);
void irq_set_exclusive_handler(uint32_t num, void (*handler)(void));
void irq_set_enabled(uint32_t num, int enabled);

#endif /* HARDWARE_IRQ_H */
