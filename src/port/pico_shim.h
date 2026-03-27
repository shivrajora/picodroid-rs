/*
 * Minimal pico-sdk shim for FreeRTOS RP2040/RP2350 SMP ports.
 *
 * Provides direct-register-access implementations of pico-sdk functions
 * required by the RP-specific FreeRTOS ports, without linking against
 * the full pico-sdk C library.
 *
 * Included via src/port/ being on the include path, shadowing pico-sdk headers
 * when building with freertos-cargo-build's get_cc().include("src/port").
 */

#ifndef PICO_SHIM_H
#define PICO_SHIM_H

#include <stdint.h>
#include <stdbool.h>

/* ---- SIO hardware layout ---- */

#define SIO_BASE  0xD0000000u

/* Only the fields we actually use are named; the rest are padding.
 * Offsets verified against RP2040 TRM Table 2.3.1 "SIO Register Summary"
 * and confirmed identical for RP2350 at these offsets. */
typedef struct {
    volatile uint32_t cpuid;        /* 0x000 – reads 0 on core 0, 1 on core 1 */
    volatile uint32_t _unused[19];  /* 0x004–0x04C – GPIO and related registers */
    volatile uint32_t fifo_st;      /* 0x050 – FIFO status */
    volatile uint32_t fifo_wr;      /* 0x054 – FIFO write (signals other core) */
    volatile uint32_t fifo_rd;      /* 0x058 – FIFO read */
    volatile uint32_t _pad[41];     /* 0x05C–0x0FC – divider, interpolator, etc. */
    volatile uint32_t spinlock[32]; /* 0x100–0x17C – hardware spinlocks */
} sio_hw_t;

#define sio_hw ((sio_hw_t *)SIO_BASE)

/* FIFO_ST bit fields */
#define SIO_FIFO_ST_VLD  (1u << 0)  /* RX FIFO has data */
#define SIO_FIFO_ST_RDY  (1u << 1)  /* TX FIFO has space */
#define SIO_FIFO_ST_WOF  (1u << 2)  /* TX overflow (sticky, write-1-to-clear) */
#define SIO_FIFO_ST_ROE  (1u << 3)  /* RX overflow (sticky, write-1-to-clear) */

/* ---- Spinlock type and inline operations ---- */

typedef volatile uint32_t spin_lock_t;

/* Spinlock IDs reserved for FreeRTOS SMP (matches pico-sdk PICO_SPINLOCK_ID_OS1/OS2) */
#define PICO_SPINLOCK_ID_OS1  26
#define PICO_SPINLOCK_ID_OS2  27

static inline spin_lock_t *spin_lock_instance(uint32_t lock_num) {
    return &sio_hw->spinlock[lock_num];
}

static inline bool spin_try_lock_unsafe(spin_lock_t *lock) {
    /* A read of the spinlock address returns non-zero only if we acquired it */
    return *lock != 0u;
}

static inline void spin_lock_unsafe_blocking(spin_lock_t *lock) {
    while (*lock == 0u) {
        __asm volatile("" ::: "memory");
    }
    __asm volatile("dmb" ::: "memory");
}

static inline void spin_unlock_unsafe(spin_lock_t *lock) {
    __asm volatile("dmb" ::: "memory");
    *lock = 0u; /* any write to the spinlock address releases it */
}

/* No-ops: we use two fixed IDs, no need for runtime claiming */
static inline void spin_lock_claim(uint32_t n)   { (void)n; }
static inline void spin_lock_unclaim(uint32_t n) { (void)n; }

/* ---- Core ID ---- */

static inline uint32_t get_core_num(void) {
    return sio_hw->cpuid;
}

/* ---- Cortex-M NVIC helpers (same on all Cortex-M cores) ---- */

#define SCB_VTOR  (*(volatile uint32_t *)0xE000ED08u) /* Vector Table Offset Register */
#define NVIC_ISER ((volatile uint32_t *)0xE000E100u)  /* Interrupt Set-Enable Registers */
#define NVIC_ICER ((volatile uint32_t *)0xE000E180u)  /* Interrupt Clear-Enable Registers */
#define NVIC_IPR  ((volatile uint8_t  *)0xE000E400u)  /* Interrupt Priority Registers */

/* ---- SIO FIFO IRQ numbers (same on RP2040 and RP2350 legacy FIFO path) ---- */
#define SIO_IRQ_PROC0  15u
#define SIO_IRQ_PROC1  16u

/* ---- Additional spinlock ops needed by configSUPPORT_PICO_SYNC_INTEROP ---- */

/* spin_lock_blocking: disable interrupts, acquire spinlock, return saved PRIMASK. */
static inline uint32_t spin_lock_blocking(spin_lock_t *lock) {
    uint32_t save;
    __asm volatile("mrs %0, PRIMASK" : "=r"(save));
    __asm volatile("cpsid i" ::: "memory");
    spin_lock_unsafe_blocking(lock);
    return save;
}

/* spin_unlock: release spinlock, restore saved interrupt state. */
static inline void spin_unlock(spin_lock_t *lock, uint32_t saved_irq_state) {
    spin_unlock_unsafe(lock);
    __asm volatile("msr PRIMASK, %0" :: "r"(saved_irq_state) : "memory");
}

/* next_striped_spin_lock_num: allocate spinlock IDs for pico-sync interop.
 * We use IDs 28–31 (clear of the FreeRTOS OS1/OS2 locks at 26/27). */
static inline uint32_t next_striped_spin_lock_num(void) {
    static uint32_t next_id = 28u;
    return next_id++;
}

/* ---- CMSIS-like helpers ---- */

/* __get_current_exception: returns current exception number from IPSR (0 = thread mode). */
static inline uint32_t __get_current_exception(void) {
    uint32_t ipsr;
    __asm volatile("mrs %0, IPSR" : "=r"(ipsr));
    return ipsr & 0x1FFu;
}

/* __sev: generate a Send Event instruction (wakes cores in WFE). */
#ifndef __sev
#define __sev()  __asm volatile("sev" ::: "memory")
#endif

/* ---- Absolute time stubs (pico/time.h stand-in for sync interop) ---- */

/* Microsecond timestamp (monotonic; we stub as zero since interop is unused). */
typedef uint64_t absolute_time_t;

static inline absolute_time_t get_absolute_time(void) {
    return 0u;
}

static inline int64_t absolute_time_diff_us(absolute_time_t from, absolute_time_t to) {
    return (int64_t)(to - from);
}

/* best_effort_wfe_or_timeout: WFE until deadline or return false.
 * Stub always returns false (caller re-checks condition). */
static inline bool best_effort_wfe_or_timeout(absolute_time_t target) {
    (void)target;
    __asm volatile("wfe" ::: "memory");
    return false;
}

/* time_reached: true if deadline has passed.
 * Stub always returns true so callers exit immediately rather than spinning. */
static inline bool time_reached(absolute_time_t target) {
    (void)target;
    return true;
}

/* spin_lock_get_num: return the index of a spinlock in the SIO spinlock array. */
static inline uint32_t spin_lock_get_num(const spin_lock_t *lock) {
    return (uint32_t)(lock - &sio_hw->spinlock[0]);
}

/* __wfe: generate a Wait For Event instruction. */
#ifndef __wfe
#define __wfe()  __asm volatile("wfe" ::: "memory")
#endif

/* ---- Function declarations (implemented in pico_shim_rp2040.c / pico_shim_rp2350.c) ---- */

/* Multicore */
void multicore_reset_core1(void);
void multicore_launch_core1(void (*entry)(void));
void multicore_fifo_clear_irq(void);
void multicore_fifo_drain(void);

/* IRQ */
void irq_set_priority(uint32_t num, uint8_t hardware_priority);
void irq_set_exclusive_handler(uint32_t num, void (*handler)(void));
void irq_set_enabled(uint32_t num, int enabled);

/* Clock */
uint32_t clock_get_hz(uint32_t clk_id);

#endif /* PICO_SHIM_H */
