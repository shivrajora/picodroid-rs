/*
 * Minimal pico/lock_core.h stub for FreeRTOS sync interop compilation.
 *
 * The real pico-sdk lock_core.h defines the generic lock_core struct used by
 * mutex_t, semaphore_t, etc.  We only need the struct layout since port.c
 * accesses the spin_lock field to release the associated spinlock.
 */

#ifndef PICO_LOCK_CORE_H
#define PICO_LOCK_CORE_H

#include "../pico_shim.h"

/* Pull in timers.h so xEventGroupSetBitsFromISR (which uses
 * xTimerPendFunctionCallFromISR) can be compiled from event_groups.h
 * which port.c includes right after this header. */
#include "timers.h"

/* Minimal lock_core layout: only spin_lock is accessed by port.c. */
struct lock_core {
    spin_lock_t *spin_lock;
};
typedef struct lock_core lock_core_t;

#endif /* PICO_LOCK_CORE_H */
