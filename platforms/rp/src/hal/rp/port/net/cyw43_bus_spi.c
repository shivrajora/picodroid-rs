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
#define SIO_GPIO_IN       (*(volatile uint32_t *)(SIO_BASE + 0x008))

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

    /* Pad config: input enable, no pull, drive 4mA */
    volatile uint32_t *pad = (volatile uint32_t *)(PADS_BANK0_BASE + 0x004 + pin * 4);
    uint32_t val = *pad;
    val &= ~(1u << 8);  /* clear ISO bit (RP2350) */
    val |= (1u << 6);   /* IE = input enable */
    val &= ~(1u << 7);  /* OD = 0 (output driver not disabled) */
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

/* Short delay for SPI clock timing (~1 MHz bit rate for bring-up) */
#define SPI_HALF_PERIOD()  delay_cycles(75)

/* ---- SPI bit-bang implementation ---- */

static int spi_polarity = 0;

/* Transfer a single byte (MSB first, CPOL=0/1, CPHA=0).
 * Returns received byte. */
static uint8_t spi_xfer_byte(uint8_t tx) {
    uint8_t rx = 0;
    for (int i = 7; i >= 0; i--) {
        /* Set data output */
        if (tx & (1u << i)) {
            gpio_put_high(PIN_WL_D);
        } else {
            gpio_put_low(PIN_WL_D);
        }

        SPI_HALF_PERIOD();

        /* Rising edge: clock data out, sample data in */
        gpio_put_high(PIN_WL_CLK);
        SPI_HALF_PERIOD();

        /* Sample input */
        if (gpio_get(PIN_WL_D)) {
            rx |= (1u << i);
        }

        /* Falling edge */
        gpio_put_low(PIN_WL_CLK);
    }
    return rx;
}

/* ---- Port-required functions (cyw43_spi.h) ---- */

void cyw43_spi_gpio_setup(void) {
    /* Configure all CYW43 pins for SIO control */
    gpio_init_sio(PIN_WL_ON);
    gpio_init_sio(PIN_WL_D);
    gpio_init_sio(PIN_WL_CS);
    gpio_init_sio(PIN_WL_CLK);

    /* Set directions: ON, CS, CLK are always output; D is bidirectional */
    gpio_set_output(PIN_WL_ON);
    gpio_set_output(PIN_WL_CS);
    gpio_set_output(PIN_WL_CLK);
    gpio_set_output(PIN_WL_D);

    /* Initial states */
    gpio_put_high(PIN_WL_CS);   /* CS deasserted */
    gpio_put_low(PIN_WL_CLK);   /* Clock idle low */
    gpio_put_low(PIN_WL_D);     /* Data low */
}

void cyw43_spi_reset(void) {
    /* Power-cycle the CYW43439 */
    gpio_put_low(PIN_WL_ON);
    cyw43_delay_ms(20);
    gpio_put_high(PIN_WL_ON);
    cyw43_delay_ms(50);
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

void cyw43_spi_set_polarity(cyw43_int_t *self, int pol) {
    (void)self;
    spi_polarity = pol;
    if (pol) {
        gpio_put_high(PIN_WL_CLK);
    } else {
        gpio_put_low(PIN_WL_CLK);
    }
}

int cyw43_spi_transfer(cyw43_int_t *self, const uint8_t *tx, size_t tx_length,
                        uint8_t *rx, size_t rx_length) {
    (void)self;

    /* Assert CS */
    gpio_put_low(PIN_WL_CS);
    SPI_HALF_PERIOD();

    /* Data pin is output for TX phase */
    gpio_set_output(PIN_WL_D);

    /* Transmit phase */
    for (size_t i = 0; i < tx_length; i++) {
        uint8_t received = spi_xfer_byte(tx[i]);
        /* If rx buffer overlaps tx range, capture received data */
        if (rx && i < rx_length) {
            rx[i] = received;
        }
    }

    /* Receive-only phase (beyond tx_length) */
    if (rx && rx_length > tx_length) {
        /* Switch data pin to input for reading */
        gpio_set_input(PIN_WL_D);

        for (size_t i = tx_length; i < rx_length; i++) {
            rx[i] = spi_xfer_byte(0x00);
        }

        /* Restore data pin to output */
        gpio_set_output(PIN_WL_D);
    }

    SPI_HALF_PERIOD();
    /* Deassert CS */
    gpio_put_high(PIN_WL_CS);

    return 0;
}
