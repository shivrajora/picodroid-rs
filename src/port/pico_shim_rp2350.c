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
 * Verified against RP2350 TRM section 3.1.3 "SIO Register Summary"
 * and pico-sdk v2 src/rp2350/hardware/regs/sio.h.
 *
 * DOORBELL_OUT: core X writes to ring a doorbell on the OTHER core.
 * DOORBELL_IN:  core X reads/clears doorbells pending on THIS core. */
#define SIO_DOORBELL_OUT_SET  (*(volatile uint32_t *)(SIO_BASE + 0x0D0u))
#define SIO_DOORBELL_OUT_CLR  (*(volatile uint32_t *)(SIO_BASE + 0x0D4u))
#define SIO_DOORBELL_IN_SET   (*(volatile uint32_t *)(SIO_BASE + 0x0D8u))
#define SIO_DOORBELL_IN_CLR   (*(volatile uint32_t *)(SIO_BASE + 0x0DCu))

/* RP2350 SIO bell IRQs (one per core, separate from RP2040-style FIFO IRQs) */
#define SIO_IRQ_BELL0  26u   /* doorbell interrupt for core 0 */
#define SIO_IRQ_BELL1  27u   /* doorbell interrupt for core 1 */

/* ---- Core 1 stack ---- */

#define CORE1_STACK_WORDS  512u  /* 2 KiB for core 1's initial scheduler stack */
static uint32_t core1_stack[CORE1_STACK_WORDS];

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

/* Bootrom FIFO launch handshake – identical protocol on RP2040 and RP2350. */
static void fifo_launch_raw(uint32_t vtor, uint32_t sp, uint32_t entry) {
    const uint32_t cmds[6] = {0, 0, 1, vtor, sp, entry};
    int seq = 0;
    do {
        uint32_t cmd = cmds[seq];
        if (!cmd) {
            multicore_fifo_drain();
            __asm volatile("sev");
        }
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_RDY)) {
            __asm volatile("" ::: "memory");
        }
        sio_hw->fifo_wr = cmd;
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_VLD)) {
            __asm volatile("wfe");
        }
        uint32_t response = sio_hw->fifo_rd;
        seq = (cmd == response) ? seq + 1 : 0;
    } while (seq < 6);
}

void multicore_launch_core1(void (*entry)(void)) {
    uint32_t *sp   = &core1_stack[CORE1_STACK_WORDS];
    uint32_t  vtor = SCB_VTOR;

    /* Disable core 0's bell IRQ during handshake to avoid races. */
    irq_set_enabled(SIO_IRQ_BELL0, 0);

    fifo_launch_raw(vtor, (uint32_t)sp, (uint32_t)entry);

    /* FreeRTOS port.c will install prvDoorbellInterruptHandler via
     * irq_set_exclusive_handler and re-enable the bell IRQ itself. */
}

/* ---- IRQ management (NVIC) ---- */

/* RAM vector table – required so irq_set_exclusive_handler can write at runtime. */
#define VT_ENTRIES  256u
static uint32_t ram_vector_table[VT_ENTRIES] __attribute__((aligned(1024)));
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
 * RP2350 has two bell IRQs: SIO_IRQ_BELL0=26 (core 0), SIO_IRQ_BELL1=27 (core 1).
 * The db_num parameter selects the individual doorbell bit within one IRQ; all
 * bits share the same per-core IRQ on RP2350, so db_num is unused here. */
uint32_t multicore_doorbell_irq_num(int8_t db_num) {
    (void)db_num;
    return SIO_IRQ_BELL0 + sio_hw->cpuid;
}

/* ---- Interrupt priority validation stub ---- */

/* portmacrocommon.h defines portASSERT_IF_INTERRUPT_PRIORITY_INVALID() as
 * vPortValidateInterruptPriority() whenever configASSERT is defined (even
 * as an empty macro via FreeRTOS.h's default).  port.c only compiles the
 * real body when configASSERT_DEFINED==1, which requires the user to
 * explicitly define configASSERT with a non-trivial expansion.  Provide a
 * no-op stub so the linker is satisfied without enabling full asserts. */
void vPortValidateInterruptPriority(void) {}
