/*
 * pico_shim_rp2040.c – minimal pico-sdk shim for the RP2040 FreeRTOS SMP port.
 *
 * Provides direct-register-access implementations of the pico-sdk C functions
 * needed by ThirdParty/GCC/RP2040/port.c when LIB_PICO_MULTICORE=1 and
 * configNUMBER_OF_CORES=2, without linking against the full pico-sdk.
 *
 * Register addresses verified against:
 *   RP2040 Datasheet (datasheets.raspberrypi.com/rp2040) and
 *   pico-sdk v1.5.x src/rp2_common/pico_multicore/multicore.c
 */

#include "pico_shim.h"

/* ---- RP2040-specific constants ---- */

/* SIO FIFO IRQ: IRQ #15 for core 0, IRQ #16 for core 1 */
#define SIO_IRQ_PROC0  15u

/* RP2040 Power-on State Machine (PSM) – used to hard-reset core 1 */
#define PSM_BASE              0x40010000u
#define PSM_FRCE_OFF_OFFSET   0x0004u
#define PSM_FRCE_OFF_PROC1    (1u << 16)

/* Atomic alias offsets for RP2040 bus fabric (XOR, SET, CLR) */
#define HW_SET_ALIAS_OFFSET   0x2000u
#define HW_CLR_ALIAS_OFFSET   0x3000u

static inline volatile uint32_t *hw_set_alias(volatile uint32_t *reg) {
    return (volatile uint32_t *)((uint32_t)reg | HW_SET_ALIAS_OFFSET);
}
static inline volatile uint32_t *hw_clr_alias(volatile uint32_t *reg) {
    return (volatile uint32_t *)((uint32_t)reg | HW_CLR_ALIAS_OFFSET);
}

/* ---- Core 1 stack ---- */

#define CORE1_STACK_WORDS  512u  /* 2 KiB for core 1's initial scheduler stack */
static uint32_t core1_stack[CORE1_STACK_WORDS];

/* ---- FIFO helpers ---- */

void multicore_fifo_clear_irq(void) {
    /* Write 1 to WOF (bit 2) and ROE (bit 3) to clear sticky error flags,
     * which is what clears the FIFO IRQ pending in the NVIC. */
    sio_hw->fifo_st = SIO_FIFO_ST_WOF | SIO_FIFO_ST_ROE;
}

void multicore_fifo_drain(void) {
    /* Read and discard all words currently in the receive FIFO. */
    while (sio_hw->fifo_st & SIO_FIFO_ST_VLD) {
        (void)sio_hw->fifo_rd;
    }
}

/* ---- Core 1 reset and launch ---- */

void multicore_reset_core1(void) {
    /* Hard-reset core 1 via PSM, then bring it back up.
     * Core 1 will drain its own FIFO then push 0 to signal readiness. */
    volatile uint32_t *frce_off = (volatile uint32_t *)(PSM_BASE + PSM_FRCE_OFF_OFFSET);

    *hw_set_alias(frce_off) = PSM_FRCE_OFF_PROC1;
    /* Fence: wait until the reset is visible */
    while (!(*frce_off & PSM_FRCE_OFF_PROC1)) {
        __asm volatile("" ::: "memory");
    }
    *hw_clr_alias(frce_off) = PSM_FRCE_OFF_PROC1;
}

/* Bootrom FIFO launch handshake (same on RP2040 and RP2350).
 * The bootrom on core 1 executes:
 *   1. Drain its own receive FIFO and echo 0 three times (sync words).
 *   2. Accept VTOR, SP, PC words and configure the core.
 * Protocol: send a word; core 1 echoes it. If echo matches, advance; else restart.
 */
static void fifo_launch_raw(uint32_t vtor, uint32_t sp, uint32_t entry) {
    const uint32_t cmds[6] = {0, 0, 1, vtor, sp, entry};
    int seq = 0;
    do {
        uint32_t cmd = cmds[seq];
        if (!cmd) {
            multicore_fifo_drain();
            __asm volatile("sev");  /* wake core 1 if waiting in WFE */
        }
        /* Wait for TX FIFO space */
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_RDY)) {
            __asm volatile("" ::: "memory");
        }
        sio_hw->fifo_wr = cmd;
        /* Wait for core 1 echo */
        while (!(sio_hw->fifo_st & SIO_FIFO_ST_VLD)) {
            __asm volatile("wfe");
        }
        uint32_t response = sio_hw->fifo_rd;
        seq = (cmd == response) ? seq + 1 : 0;
    } while (seq < 6);
}

void multicore_launch_core1(void (*entry)(void)) {
    uint32_t *sp   = &core1_stack[CORE1_STACK_WORDS]; /* top of stack */
    uint32_t  vtor = SCB_VTOR;

    /* Disable the FIFO IRQ on core 0 during the handshake so we can poll
     * the FIFO directly without racing with an interrupt handler. */
    irq_set_enabled(SIO_IRQ_PROC0, 0);

    fifo_launch_raw(vtor, (uint32_t)sp, (uint32_t)entry);

    /* Do NOT re-enable SIO_IRQ_PROC0 here.  FreeRTOS port.c will install
     * prvFIFOInterruptHandler via irq_set_exclusive_handler and then call
     * irq_set_enabled itself. */
}

/* ---- IRQ management (NVIC) ---- */

/* RAM vector table – required so irq_set_exclusive_handler can write at runtime.
 * We copy the flash vector table on first use and redirect VTOR here.
 * Alignment: VTOR requires alignment to a power-of-2 >= (entries * 4).
 * 256 entries * 4 = 1024 bytes, so aligned(1024) is correct. */
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

/* Returns the RP2040 system clock frequency (fixed at 125 MHz).
 * The argument is ignored; only clk_sys (5) is ever passed by port.c. */
uint32_t clock_get_hz(uint32_t clk_id) {
    (void)clk_id;
    return 125000000UL;
}

/* ---- Doorbell stubs (RP2040 uses FIFO, not doorbells; these are no-ops) ---- */

int8_t multicore_doorbell_claim_unused(uint32_t mask, bool required) {
    (void)mask; (void)required;
    return -1;  /* RP2040 has no doorbells */
}
void multicore_doorbell_clear_current_core(int8_t db_num) { (void)db_num; }
void multicore_doorbell_clear_other_core(int8_t db_num)   { (void)db_num; }
bool multicore_doorbell_is_set_current_core(int8_t db_num) { (void)db_num; return false; }
void multicore_doorbell_set_other_core(int8_t db_num)      { (void)db_num; }
uint32_t multicore_doorbell_irq_num(int8_t db_num) { (void)db_num; return SIO_IRQ_PROC0; }
