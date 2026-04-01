/*
 * hardware/exception.h stub.
 *
 * With PICO_NO_RAM_VECTOR_TABLE=1, rp2040_config.h sets
 * configUSE_DYNAMIC_EXCEPTION_HANDLERS=0, so port.c never calls
 * exception_set_exclusive_handler(). We only need the exception number
 * enum to be defined so the header parses cleanly.
 */
#ifndef HARDWARE_EXCEPTION_H
#define HARDWARE_EXCEPTION_H
#include <stdint.h>

/* Cortex-M exception numbers (negative) */
typedef int8_t exception_number;
enum {
    NMI_EXCEPTION       = -14,
    HARDFAULT_EXCEPTION = -13,
    SVCALL_EXCEPTION    =  -5,
    PENDSV_EXCEPTION    =  -2,
    SYSTICK_EXCEPTION   =  -1,
};

typedef void (*exception_handler_t)(void);

/* Only called when configUSE_DYNAMIC_EXCEPTION_HANDLERS=1; stub is sufficient */
static inline void exception_set_exclusive_handler(exception_number num,
                                                   exception_handler_t fn) {
    (void)num; (void)fn;
}

#endif /* HARDWARE_EXCEPTION_H */
