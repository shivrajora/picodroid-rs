/*
 * pico_shim_rp2350.c – minimal pico-sdk shim for the RP2350 FreeRTOS SMP port.
 *
 * Provides direct-register-access implementations of the pico-sdk C functions
 * needed by ThirdParty/Community-Supported-Ports/GCC/RP2350_ARM_NTZ port.c
 * when LIB_PICO_MULTICORE=1 and configNUMBER_OF_CORES=2, without linking
 * against the full pico-sdk.
 *
 * Register addresses verified against:
 *   RP2350 Datasheet (datasheets.raspberrypi.com/rp2350) and
 *   pico-sdk v2.x src/rp2_common/pico_multicore/multicore.c
 */

#include "pico_shim.h"

/* ---- RP2350-specific constants ---- */

/* RP2350 Power-on State Machine (PSM) – same base as RP2040 */
#define PSM_BASE              0x40010000u
#define PSM_FRCE_OFF_OFFSET   0x0004u
#define PSM_FRCE_OFF_PROC1    (1u << 16)  /* bit 16, same position as RP2040 */

/* Atomic alias offsets for RP2350 bus fabric */
#define HW_SET_ALIAS_OFFSET   0x2000u
#define HW_CLR_ALIAS_OFFSET   0x3000u

static inline volatile uint32_t *hw_set_alias(volatile uint32_t *reg) {
    return (volatile uint32_t *)((uint32_t)reg | HW_SET_ALIAS_OFFSET);
}
static inline volatile uint32_t *hw_clr_alias(volatile uint32_t *reg) {
    return (volatile uint32_t *)((uint32_t)reg | HW_CLR_ALIAS_OFFSET);
}

/* RP2350 SIO doorbell registers.
 *
 * The doorbell block sits AFTER the spinlock bank (0x100-0x17C), at
 * SIO_BASE + 0x180.  The 0x0D0-0x0DC range this shim originally used is
 * INTERP1 lane-pop territory, where writes are silently discarded — every
 * doorbell ring was a no-op, so core 1 (whose scheduler is driven solely
 * by doorbell IPIs; only core 0 has the tick) never woke a ready task.
 *
 * DOORBELL_OUT: core X writes to ring a doorbell on the OTHER core.
 * DOORBELL_IN:  core X reads/clears doorbells pending on THIS core. */
#define SIO_DOORBELL_OUT_SET  (*(volatile uint32_t *)(SIO_BASE + 0x180u))
#define SIO_DOORBELL_OUT_CLR  (*(volatile uint32_t *)(SIO_BASE + 0x184u))
#define SIO_DOORBELL_IN_SET   (*(volatile uint32_t *)(SIO_BASE + 0x188u))
#define SIO_DOORBELL_IN_CLR   (*(volatile uint32_t *)(SIO_BASE + 0x18Cu))

/* RP2350 SIO bell IRQ.  Unlike the RP2040's two distinct FIFO IRQ numbers
 * (SIO_IRQ_PROC0=15 / SIO_IRQ_PROC1=16), the RP2350's SIO interrupt lines
 * are per-core BANKED on a single number: IRQ 26 is SIO_IRQ_BELL on BOTH
 * cores (each core's NVIC sees its own doorbell on 26).  IRQ 27 is
 * SIO_IRQ_FIFO_NS — the Non-Secure FIFO view — and never fires here, so
 * the original "26 + cpuid" computation left core 1 listening on a dead
 * line: pending doorbells were never taken and cross-core yields to
 * core 1 were silently lost (core-1-affinitized tasks wedged in the
 * ready list forever). */
#define SIO_IRQ_BELL   26u   /* doorbell interrupt, per-core banked */

/* ---- Core 1 stack ---- */

#define CORE1_STACK_WORDS  512u  /* 2 KiB for core 1's initial scheduler stack */
static uint32_t core1_stack[CORE1_STACK_WORDS];

/* RAM vector table (defined under "IRQ management" below).  Declared here
 * because multicore_launch_core1 must hand it to core 1 at launch. */
#define VT_ENTRIES  256u
static uint32_t ram_vector_table[VT_ENTRIES] __attribute__((aligned(1024)));
static void ensure_ram_vt(void);

/* ---- FIFO helpers (used only during core 1 launch handshake) ---- */

void multicore_fifo_clear_irq(void) {
    sio_hw->fifo_st = SIO_FIFO_ST_WOF | SIO_FIFO_ST_ROE;
}

void multicore_fifo_drain(void) {
    while (sio_hw->fifo_st & SIO_FIFO_ST_VLD) {
        (void)sio_hw->fifo_rd;
    }
}

/* ---- Core 1 reset and launch ---- */

void multicore_reset_core1(void) {
    volatile uint32_t *frce_off = (volatile uint32_t *)(PSM_BASE + PSM_FRCE_OFF_OFFSET);

    *hw_set_alias(frce_off) = PSM_FRCE_OFF_PROC1;
    while (!(*frce_off & PSM_FRCE_OFF_PROC1)) {
        __asm volatile("" ::: "memory");
    }
    *hw_clr_alias(frce_off) = PSM_FRCE_OFF_PROC1;
}

/* Bootrom FIFO launch handshake – identical protocol on RP2040 and RP2350.
 *
 * In secure boot mode (IMAGE_DEF secure_exe), core 1's bootrom may not
 * respond to the FIFO handshake.  A timeout prevents hanging the scheduler;
 * core 0 proceeds as a single-core system while core 1 stays idle.
 * TODO: investigate why the RP2350 bootrom ignores the FIFO in secure mode. */
static void fifo_launch_raw(uint32_t vtor, uint32_t sp, uint32_t entry) {
    const uint32_t cmds[6] = {0, 0, 1, vtor, sp, entry};
    int seq = 0;
    do {
        uint32_t cmd = cmds[seq];
        if (!cmd) {
            multicore_fifo_drain();
            __asm volatile("sev");
        }
        volatile uint32_t tries = 0;
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_RDY)) {
            if (++tries > 5000000u) return; /* timeout — core 1 not responding */
            __asm volatile("" ::: "memory");
        }
        sio_hw->fifo_wr = cmd;
        tries = 0;
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_VLD)) {
            if (++tries > 5000000u) return; /* timeout — core 1 not responding */
            __asm volatile("sev; wfe" ::: "memory");
        }
        uint32_t response = sio_hw->fifo_rd;
        seq = (cmd == response) ? seq + 1 : 0;
    } while (seq < 6);
}

void multicore_launch_core1(void (*entry)(void)) {
    uint32_t *sp   = &core1_stack[CORE1_STACK_WORDS];

    /* VTOR is per-core, and irq_set_exclusive_handler only writes the RAM
     * table.  Switch core 0 to the RAM table now and launch core 1 pointing
     * at the same table, so the doorbell handlers each core installs
     * afterwards (port.c) are live on BOTH cores.  Launching with whatever
     * core 0's VTOR held at the time left one core on the flash table, where
     * its bell IRQ slot is the cortex-m-rt DefaultHandler infinite loop: the
     * first cross-core yield IPI wedged that core at the lowest IRQ
     * priority, blocking its tick and PendSV forever. */
    ensure_ram_vt();

    /* Disable core 0's bell IRQ during handshake to avoid races. */
    irq_set_enabled(SIO_IRQ_BELL, 0);

    fifo_launch_raw((uint32_t)ram_vector_table, (uint32_t)sp, (uint32_t)entry);

    /* FreeRTOS port.c will install prvDoorbellInterruptHandler via
     * irq_set_exclusive_handler and re-enable the bell IRQ itself. */
}

/* ---- IRQ management (NVIC) ---- */

/* RAM vector table – required so irq_set_exclusive_handler can write at runtime.
 * Storage is declared above multicore_launch_core1, which initialises the
 * table before launching core 1 and passes it as core 1's boot VTOR. */
static int vt_initialized = 0;

static void ensure_ram_vt(void) {
    if (vt_initialized) return;
    const uint32_t *flash_vt = (const uint32_t *)SCB_VTOR;
    for (unsigned i = 0; i < VT_ENTRIES; i++) {
        ram_vector_table[i] = flash_vt[i];
    }
    SCB_VTOR = (uint32_t)ram_vector_table;
    __asm volatile("dsb" ::: "memory");
    vt_initialized = 1;
}

void irq_set_priority(uint32_t num, uint8_t hardware_priority) {
    NVIC_IPR[num] = hardware_priority;
}

void irq_set_exclusive_handler(uint32_t num, void (*handler)(void)) {
    ensure_ram_vt();
    ram_vector_table[num + 16u] = (uint32_t)handler;
    __asm volatile("dsb" ::: "memory");
}

void irq_set_enabled(uint32_t num, int enabled) {
    volatile uint32_t *reg = enabled
        ? &NVIC_ISER[num >> 5]
        : &NVIC_ICER[num >> 5];
    *reg = 1u << (num & 31u);
}

/* ---- Clock ---- */

uint32_t clock_get_hz(uint32_t clk_id) {
    (void)clk_id;
    return 150000000UL;  /* RP2350 system clock: 150 MHz */
}

/* ---- Doorbell API (RP2350 uses hardware doorbells for vYieldCore) ---- */

/* Claim a doorbell from the given availability mask.
 * We only ever call this with mask=0b11 (bits 0 or 1 acceptable).
 * Returns doorbell number 0 (simplest allocation). */
int8_t multicore_doorbell_claim_unused(uint32_t mask, bool required) {
    (void)required;
    /* Bit 0 of mask means "doorbell 0 is acceptable" */
    for (int8_t i = 0; i < 8; i++) {
        if (mask & (1u << i)) {
            return i;
        }
    }
    return -1;
}

/* Clear the doorbell visible on the CURRENT core (clear DOORBELL_IN bit). */
void multicore_doorbell_clear_current_core(int8_t db_num) {
    SIO_DOORBELL_IN_CLR = 1u << (uint32_t)db_num;
    __asm volatile("" ::: "memory");
}

/* Clear the doorbell visible on the OTHER core (clear our DOORBELL_OUT bit). */
void multicore_doorbell_clear_other_core(int8_t db_num) {
    SIO_DOORBELL_OUT_CLR = 1u << (uint32_t)db_num;
    __asm volatile("" ::: "memory");
}

/* Check whether the doorbell bit is set on the current core's IN register. */
bool multicore_doorbell_is_set_current_core(int8_t db_num) {
    return (SIO_DOORBELL_IN_SET & (1u << (uint32_t)db_num)) != 0u;
}

/* Ring a doorbell on the OTHER core (set bit in our OUT register). */
void multicore_doorbell_set_other_core(int8_t db_num) {
    SIO_DOORBELL_OUT_SET = 1u << (uint32_t)db_num;
    __asm volatile("" ::: "memory");
}

/* Return the NVIC IRQ number for the doorbell interrupt on the CURRENT core.
 * Per-core banked: IRQ 26 on both cores (see SIO_IRQ_BELL comment above).
 * The db_num parameter selects the individual doorbell bit within the IRQ;
 * all bits share the one bell IRQ, so it is unused here. */
uint32_t multicore_doorbell_irq_num(int8_t db_num) {
    (void)db_num;
    return SIO_IRQ_BELL;
}

/* ---- Run-time stats counter ---- */

/* RP2350 TIMER0 peripheral – 64-bit µs counter, always running from reset. */
#define TIMER0_BASE       0x400B0000u
#define TIMER0_TIMERAWL   (*(volatile uint32_t *)(TIMER0_BASE + 0x28u))

uint32_t picodroid_get_runtime_counter(void) {
    return TIMER0_TIMERAWL;
}

/* ---- Interrupt priority validation stub ---- */

/* portmacrocommon.h defines portASSERT_IF_INTERRUPT_PRIORITY_INVALID() as
 * vPortValidateInterruptPriority() whenever configASSERT is defined (even
 * as an empty macro via FreeRTOS.h's default).  port.c only compiles the
 * real body when configASSERT_DEFINED==1, which requires the user to
 * explicitly define configASSERT with a non-trivial expansion.  Provide a
 * no-op stub so the linker is satisfied without enabling full asserts. */
void vPortValidateInterruptPriority(void) {}
