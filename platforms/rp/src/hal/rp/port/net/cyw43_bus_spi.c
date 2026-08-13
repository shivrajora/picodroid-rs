/*
 * CYW43 SPI bus transport for Pico 2 W (RP2350).
 *
 * Implements the port-required SPI bus functions (cyw43_spi.h) using
 * software bit-bang GPIO.  This is a bring-up implementation — a
 * PIO+DMA version will replace it for production throughput.
 *
 * Pico 2 W CYW43439 pin wiring:
 *   GP23 — WL_ON  (power enable, active high)
 *   GP24 — WL_D   (bidirectional data)
 *   GP25 — WL_CS  (chip select, active low)
 *   GP29 — WL_CLK (SPI clock)
 */

#include <stdint.h>
#include <stdbool.h>
#include <string.h>
#include "cyw43.h"
#include "cyw43_internal.h"
#include "cyw43_spi.h"

/* ---- GPIO register access (direct, no pico-sdk) ---- */

#define IO_BANK0_BASE     0x40028000u
#define PADS_BANK0_BASE   0x40038000u
#define SIO_BASE          0xD0000000u

/* SIO registers for direct GPIO control */
#define SIO_GPIO_OUT_SET  (*(volatile uint32_t *)(SIO_BASE + 0x018))
#define SIO_GPIO_OUT_CLR  (*(volatile uint32_t *)(SIO_BASE + 0x020))
#define SIO_GPIO_OUT_XOR  (*(volatile uint32_t *)(SIO_BASE + 0x028))
#define SIO_GPIO_OE_SET   (*(volatile uint32_t *)(SIO_BASE + 0x038))
#define SIO_GPIO_OE_CLR   (*(volatile uint32_t *)(SIO_BASE + 0x040))
/* RP2350 GPIO_IN is at +0x004; +0x008 is GPIO_HI_IN (GPIOs 32-47), where
 * bit 24 is a nonexistent GPIO and reads as constant zero.  The original
 * 0x008 here made every input read return 0 — TX worked, RX was blind. */
#define SIO_GPIO_IN       (*(volatile uint32_t *)(SIO_BASE + 0x004))

/* Pin numbers */
#define PIN_WL_ON   23
#define PIN_WL_D    24
#define PIN_WL_CS   25
#define PIN_WL_CLK  29

/* Bit masks */
#define MASK_WL_ON  (1u << PIN_WL_ON)
#define MASK_WL_D   (1u << PIN_WL_D)
#define MASK_WL_CS  (1u << PIN_WL_CS)
#define MASK_WL_CLK (1u << PIN_WL_CLK)

/* Configure a GPIO pin for SIO (software) control */
static void gpio_init_sio(unsigned int pin) {
    /* Function select = SIO (5) */
    volatile uint32_t *ctrl = (volatile uint32_t *)(IO_BANK0_BASE + 0x004 + pin * 8);
    *ctrl = 5;

    /* Pad config: input enable, schmitt, drive 4mA */
    volatile uint32_t *pad = (volatile uint32_t *)(PADS_BANK0_BASE + 0x004 + pin * 4);
    uint32_t val = *pad;
    val &= ~(1u << 8);  /* clear ISO bit (RP2350) */
    val |= (1u << 6);   /* IE = input enable */
    val &= ~(1u << 7);  /* OD = 0 (output driver not disabled) */
    val |= (1u << 1);   /* SCHMITT = input hysteresis */
    if (pin == PIN_WL_D) {
        /* Shared data/IRQ line idles as input and doubles as the ACTIVE-HIGH
         * host-wake interrupt (INTERRUPT_POLARITY_HIGH in the bus config).
         * Pull it DOWN so an undriven line reads "no work pending"; a pull-up
         * here fakes a permanently-asserted IRQ. */
        val &= ~(1u << 3);  /* PUE off */
        val |= (1u << 2);   /* PDE */
    }
    *pad = val;
}

static inline void gpio_set_output(unsigned int pin) {
    SIO_GPIO_OE_SET = (1u << pin);
}

static inline void gpio_set_input(unsigned int pin) {
    SIO_GPIO_OE_CLR = (1u << pin);
}

static inline void gpio_put_high(unsigned int pin) {
    SIO_GPIO_OUT_SET = (1u << pin);
}

static inline void gpio_put_low(unsigned int pin) {
    SIO_GPIO_OUT_CLR = (1u << pin);
}

static inline bool gpio_get(unsigned int pin) {
    return (SIO_GPIO_IN & (1u << pin)) != 0;
}

/* ---- Delay (busy-wait using cycle counter) ---- */

static inline void delay_cycles(uint32_t cycles) {
    volatile uint32_t count = cycles;
    while (count--) {
        __asm volatile("nop");
    }
}

/* Short delay for SPI clock timing.  Keep the bit clock in the low-MHz
 * range: every known-good transport runs this chip at 10-33 MHz, and at
 * sub-MHz rates the gSPI interface has been observed to stop decoding
 * commands entirely (periodic junk instead of responses). */
#define SPI_HALF_PERIOD()  delay_cycles(8)

/* ---- SPI bit-bang implementation ---- */

static int spi_polarity = 0;

/*
 * gSPI wire format, matching the pico-sdk PIO transport exactly: buffer
 * bytes travel in order, MSB-first within each byte (pico-sdk DMA uses
 * bswap into a shift-left/MSB-first PIO, which is equivalent to a
 * big-endian load of each 4-byte group).  The driver pre-swizzles boot
 * transfers with cyw43_put_swap32 and steady-state ones with put_le32;
 * the transport itself is oblivious to both.
 */

/* Clock out one 32-bit word, MSB (bit 31) first.  WL_D must be output. */
static void spi_write_word(uint32_t w) {
    for (int i = 31; i >= 0; i--) {
        if (w & (1u << i)) {
            gpio_put_high(PIN_WL_D);
        } else {
            gpio_put_low(PIN_WL_D);
        }
        SPI_HALF_PERIOD();
        gpio_put_high(PIN_WL_CLK);   /* chip latches on rising edge */
        SPI_HALF_PERIOD();
        gpio_put_low(PIN_WL_CLK);
    }
}

/* Clock in one 32-bit word, MSB first.  WL_D must be input.
 *
 * gSPI read discipline (PicoWi: "the data is read immediately, then the
 * clock is toggled high and low"): the chip presents each response bit
 * BEFORE the clock pulse — bit 0 is already valid during the turnaround
 * after the command word — so sample first, then pulse the clock to
 * advance the chip's shifter. */
static uint32_t spi_read_word(void) {
    uint32_t w = 0;
    for (int i = 31; i >= 0; i--) {
        if (gpio_get(PIN_WL_D)) {
            w |= (1u << i);
        }
        gpio_put_high(PIN_WL_CLK);
        SPI_HALF_PERIOD();
        gpio_put_low(PIN_WL_CLK);
        SPI_HALF_PERIOD();
    }
    return w;
}

/* Load/store a 4-byte group as a big-endian u32 (buffer byte 0 = first
 * on the wire, MSB-first).  Byte access only: the swap-register helpers
 * pass stack arrays with no alignment guarantee. */
static uint32_t load_be32(const uint8_t *p) {
    return ((uint32_t)p[0] << 24) | ((uint32_t)p[1] << 16) |
           ((uint32_t)p[2] << 8) | (uint32_t)p[3];
}

static void store_be32(uint8_t *p, uint32_t w) {
    p[0] = (uint8_t)(w >> 24);
    p[1] = (uint8_t)(w >> 16);
    p[2] = (uint8_t)(w >> 8);
    p[3] = (uint8_t)w;
}

/* ---- Port-required functions (cyw43_spi.h) ---- */

void cyw43_spi_gpio_setup(void) {
    /* Configure all CYW43 pins for SIO control */
    gpio_init_sio(PIN_WL_ON);
    gpio_init_sio(PIN_WL_D);
    gpio_init_sio(PIN_WL_CS);
    gpio_init_sio(PIN_WL_CLK);

    /* Set directions: ON, CS, CLK are always output.  D is bidirectional
     * and must IDLE AS INPUT — GP24 doubles as the active-high gSPI host-wake
     * line, so holding it as an output between transfers would fight the
     * chip and corrupt cyw43_ll_has_work(). */
    gpio_set_output(PIN_WL_ON);
    gpio_set_output(PIN_WL_CS);
    gpio_set_output(PIN_WL_CLK);
    gpio_set_input(PIN_WL_D);

    /* Initial states */
    gpio_put_high(PIN_WL_CS);   /* CS deasserted */
    gpio_put_low(PIN_WL_CLK);   /* Clock idle low */
    gpio_put_low(PIN_WL_D);     /* Data-out latch low (for when driven) */
}

void cyw43_spi_reset(void) {
    /* Power-cycle the CYW43439.  Hold the DATA line driven LOW across the
     * WL_REG_ON rise (pico-sdk does the same via its DATA_OUT-low init),
     * and give the chip the full 250 ms post-power-on settle pico-sdk
     * uses — 50 ms is not reliably enough for first bus access. */
    gpio_put_low(PIN_WL_D);
    gpio_set_output(PIN_WL_D);
    gpio_put_low(PIN_WL_ON);
    cyw43_delay_ms(20);
    gpio_put_high(PIN_WL_ON);
    cyw43_delay_ms(250);
    gpio_set_input(PIN_WL_D);
}

int cyw43_spi_init(cyw43_int_t *self) {
    (void)self;
    cyw43_spi_gpio_setup();
    cyw43_spi_reset();
    return 0;
}

void cyw43_spi_deinit(cyw43_int_t *self) {
    (void)self;
    gpio_put_low(PIN_WL_ON);
}

/* The driver only ever requests polarity 0 (cyw43_spi.c:84, defensive
 * re-sync after the bus-control write); CPOL=1 support is deliberately
 * not implemented in the bit-bang loops. */
void cyw43_spi_set_polarity(cyw43_int_t *self, int pol) {
    (void)self;
    spi_polarity = pol;
    if (pol) {
        gpio_put_high(PIN_WL_CLK);
    } else {
        gpio_put_low(PIN_WL_CLK);
    }
}

/*
 * gSPI transaction.  The driver uses exactly two shapes:
 *   write:  (tx, N, NULL, 0)   — host drives all N bytes (4-byte command
 *                                word plus payload).
 *   read:   (buf, N, buf, N)   — the command is exactly the first 4 bytes;
 *                                the chip drives the shared data line for
 *                                bytes [4..N).  rx[0..4) is never inspected
 *                                by the caller.  (F1 reads insert their
 *                                response-delay pad inside N; no dummy
 *                                clocks are added here.)
 * All lengths are multiples of 4.
 */
int cyw43_spi_transfer(cyw43_int_t *self, const uint8_t *tx, size_t tx_length,
                        uint8_t *rx, size_t rx_length) {
    (void)self;

    if (tx == NULL || tx_length < 4) {
        return -1;
    }

    /* The transfer must be atomic on this core: a preemption (doorbell IRQ
     * → context switch) mid-frame parks CS low with the clock frozen for
     * milliseconds, and the chip drops the half-delivered frame.  Only
     * >1 KB frames (CLM download chunks, large TX) are slow enough to
     * reliably straddle a scheduling quantum, which made exactly those
     * fail intermittently.  Worst case IRQ-off window: ~7 ms for a
     * 1036-byte frame at ~1.25 MHz — the tick runs on core 0, so only
     * this core's doorbell handling is delayed. */
    uint32_t primask;
    __asm volatile ("mrs %0, PRIMASK\n\tcpsid i" : "=r"(primask));

    /* Assert CS */
    gpio_put_low(PIN_WL_CS);
    SPI_HALF_PERIOD();

    /* Drive the data line for the command phase */
    gpio_set_output(PIN_WL_D);

    if (rx == NULL) {
        /* Pure write: host drives everything */
        for (size_t i = 0; i < tx_length; i += 4) {
            spi_write_word(load_be32(&tx[i]));
        }
    } else {
        /* Read: drive the 32-bit command word, then turn the line around
         * (clock is low after the final falling edge) and sample the
         * response the chip drives. */
        spi_write_word(load_be32(&tx[0]));
        gpio_set_input(PIN_WL_D);

        for (size_t i = 4; i < rx_length; i += 4) {
            store_be32(&rx[i], spi_read_word());
        }
    }

    /* Idle the data line as input: GP24 doubles as the active-low gSPI
     * IRQ line, which the driver reads between transactions. */
    gpio_set_input(PIN_WL_D);

    SPI_HALF_PERIOD();
    /* Deassert CS */
    gpio_put_high(PIN_WL_CS);

    __asm volatile ("msr PRIMASK, %0" :: "r"(primask));

    return 0;
}
