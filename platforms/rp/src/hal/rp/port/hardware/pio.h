/*
 * Minimal PIO shim — shadows pico-sdk's hardware/pio.h.
 *
 * Provides the subset of PIO API needed by the CYW43 bus driver.
 * Actual implementations are in pico_shim_rp2350.c.
 */

#ifndef HARDWARE_PIO_H
#define HARDWARE_PIO_H

#include <stdint.h>
#include <stdbool.h>

/* ---- PIO base addresses (RP2350) ---- */
#define PIO0_BASE  0x50200000u
#define PIO1_BASE  0x50300000u

/* ---- PIO hardware register block ---- */
/* Minimal layout — only fields needed by the CYW43 SPI driver */

typedef struct {
    volatile uint32_t ctrl;          /* 0x000 */
    volatile uint32_t fstat;         /* 0x004 */
    volatile uint32_t fdebug;       /* 0x008 */
    volatile uint32_t flevel;       /* 0x00C */
    volatile uint32_t txf[4];       /* 0x010-0x01C — TX FIFOs for SM0-SM3 */
    volatile uint32_t rxf[4];       /* 0x020-0x02C — RX FIFOs for SM0-SM3 */
    volatile uint32_t irq;          /* 0x030 */
    volatile uint32_t irq_force;    /* 0x034 */
    volatile uint32_t input_sync_bypass; /* 0x038 */
    volatile uint32_t dbg_padout;   /* 0x03C */
    volatile uint32_t dbg_padoe;    /* 0x040 */
    volatile uint32_t dbg_cfginfo;  /* 0x044 */
    volatile uint32_t instr_mem[32]; /* 0x048-0x0C4 — instruction memory */
    struct {
        volatile uint32_t clkdiv;    /* SM clock divider */
        volatile uint32_t execctrl;  /* SM execution control */
        volatile uint32_t shiftctrl; /* SM shift control */
        volatile uint32_t addr;      /* SM program counter */
        volatile uint32_t instr;     /* SM instruction register */
        volatile uint32_t pinctrl;   /* SM pin control */
    } sm[4];                         /* 0x0C8+ — state machines 0-3 */
} pio_hw_t;

typedef pio_hw_t *PIO;

#define pio0_hw  ((pio_hw_t *)PIO0_BASE)
#define pio1_hw  ((pio_hw_t *)PIO1_BASE)
#define pio0     pio0_hw
#define pio1     pio1_hw

/* ---- PIO instruction encoding ---- */
typedef struct {
    uint16_t *insn;
    uint8_t length;
    int8_t origin;
} pio_program_t;

/* ---- State machine config ---- */
typedef struct {
    uint32_t clkdiv;
    uint32_t execctrl;
    uint32_t shiftctrl;
    uint32_t pinctrl;
} pio_sm_config;

/* ---- PIO API functions (implemented in pico_shim_rp2350.c) ---- */

/* Claim / release a state machine */
void pio_sm_claim(PIO pio, uint sm);
void pio_sm_unclaim(PIO pio, uint sm);

/* Load PIO program */
bool pio_can_add_program(PIO pio, const pio_program_t *program);
uint pio_add_program(PIO pio, const pio_program_t *program);

/* Configure and control state machines */
void pio_sm_init(PIO pio, uint sm, uint initial_pc, const pio_sm_config *config);
void pio_sm_set_enabled(PIO pio, uint sm, bool enabled);
void pio_sm_set_consecutive_pindirs(PIO pio, uint sm, uint pin, uint count, bool is_out);
void pio_sm_set_pins(PIO pio, uint sm, uint32_t pin_values);
void pio_sm_set_pins_with_mask(PIO pio, uint sm, uint32_t pin_values, uint32_t pin_mask);
void pio_sm_exec(PIO pio, uint sm, uint instr);
void pio_sm_put_blocking(PIO pio, uint sm, uint32_t data);
uint32_t pio_sm_get_blocking(PIO pio, uint sm);
void pio_sm_set_clkdiv(PIO pio, uint sm, float div);

/* GPIO function select for PIO */
void pio_gpio_init(PIO pio, uint pin);

/* SM config helpers */
pio_sm_config pio_get_default_sm_config(void);
void sm_config_set_wrap(pio_sm_config *c, uint wrap_target, uint wrap);
void sm_config_set_in_shift(pio_sm_config *c, bool shift_right, bool autopush, uint push_threshold);
void sm_config_set_out_shift(pio_sm_config *c, bool shift_right, bool autopull, uint pull_threshold);
void sm_config_set_sideset_pins(pio_sm_config *c, uint sideset_base);
void sm_config_set_sideset(pio_sm_config *c, uint bit_count, bool optional, bool pindirs);
void sm_config_set_out_pins(pio_sm_config *c, uint out_base, uint out_count);
void sm_config_set_in_pins(pio_sm_config *c, uint in_base);
void sm_config_set_set_pins(pio_sm_config *c, uint set_base, uint set_count);
void sm_config_set_clkdiv(pio_sm_config *c, float div);
void sm_config_set_clkdiv_int_frac(pio_sm_config *c, uint16_t div_int, uint8_t div_frac);

/* FIFO helpers */
static inline bool pio_sm_is_tx_fifo_full(PIO pio, uint sm) {
    return (pio->fstat & (1u << (16 + sm))) != 0;
}

static inline bool pio_sm_is_rx_fifo_empty(PIO pio, uint sm) {
    return (pio->fstat & (1u << (8 + sm))) != 0;  /* RXEMPTY bit */
}

/* ---- PIO instructions (for inline use) ---- */
/* These are encoded PIO instructions that can be executed via pio_sm_exec */
static inline uint pio_encode_sideset(uint sideset_bit_count, uint value) {
    return value << (13 - sideset_bit_count);
}

static inline uint pio_encode_sideset_opt(uint sideset_bit_count, uint value) {
    return 0x1000 | (value << (12 - sideset_bit_count));
}

#endif /* HARDWARE_PIO_H */
