/*
 * pico/multicore.h stub – declares multicore functions used by port.c.
 * Implementations are in pico_shim_rp2040.c and pico_shim_rp2350.c.
 */
#ifndef PICO_MULTICORE_H
#define PICO_MULTICORE_H
#include <stdint.h>
#include <stdbool.h>
#include "../pico_shim.h"

/* ---- Common multicore (both RP2040 and RP2350) ---- */

void multicore_reset_core1(void);
void multicore_launch_core1(void (*entry)(void));
void multicore_fifo_clear_irq(void);
void multicore_fifo_drain(void);

/* ---- RP2350-only: doorbell-based cross-core signalling ---- */
/* Used by the RP2350 community FreeRTOS port in place of the FIFO. */

int8_t multicore_doorbell_claim_unused(uint32_t mask, bool required);
void   multicore_doorbell_clear_current_core(int8_t db_num);
void   multicore_doorbell_clear_other_core(int8_t db_num);
bool   multicore_doorbell_is_set_current_core(int8_t db_num);
void   multicore_doorbell_set_other_core(int8_t db_num);
uint32_t multicore_doorbell_irq_num(int8_t db_num);

#endif /* PICO_MULTICORE_H */
