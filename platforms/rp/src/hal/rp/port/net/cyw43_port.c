/*
 * CYW43 driver port implementation for picodroid (FreeRTOS on RP2350).
 *
 * Provides the HAL functions, threading primitives, and network callbacks
 * declared in cyw43_configport.h.
 */

#include <stdint.h>
#include <stdbool.h>
#include <stdarg.h>
#include "cyw43.h"

#include "FreeRTOS.h"
#include "task.h"
#include "semphr.h"

/* ---- Hardware timer (RP2350 TIMER0 at 1 MHz) ---- */

#define TIMER_BASE  0x400B0000u
/* Use the RAW (unlatched) counter registers: safe for concurrent readers on
 * both cores, unlike the TIMELR/TIMEHR latched pair. */
#define TIMER_TIMERAWH (*(volatile uint32_t *)(TIMER_BASE + 0x24))
#define TIMER_TIMERAWL (*(volatile uint32_t *)(TIMER_BASE + 0x28))

static uint64_t get_time_us(void) {
    uint32_t lo, hi;
    /* Read high then low then high again to handle rollover */
    do {
        hi = TIMER_TIMERAWH;
        lo = TIMER_TIMERAWL;
    } while (hi != TIMER_TIMERAWH);
    return ((uint64_t)hi << 32) | lo;
}

/* ---- Timing functions ---- */

void cyw43_delay_us(uint32_t us) {
    if (us == 0) return;
    uint64_t target = get_time_us() + us;
    while (get_time_us() < target) {
        __asm volatile("nop");
    }
}

void cyw43_delay_ms(uint32_t ms) {
    /* For longer delays, yield to FreeRTOS scheduler */
    if (ms >= 2 && xTaskGetSchedulerState() == taskSCHEDULER_RUNNING) {
        vTaskDelay(pdMS_TO_TICKS(ms));
    } else {
        cyw43_delay_us(ms * 1000);
    }
}

uint32_t cyw43_hal_ticks_us(void) {
    return (uint32_t)get_time_us();
}

uint32_t cyw43_hal_ticks_ms(void) {
    return (uint32_t)(get_time_us() / 1000);
}

/* ---- GPIO control for WL_ON ---- */

#define SIO_GPIO_OUT_SET_ADDR  (*(volatile uint32_t *)(0xD0000000u + 0x018))
#define SIO_GPIO_OUT_CLR_ADDR  (*(volatile uint32_t *)(0xD0000000u + 0x020))
/* RP2350 GPIO_IN lives at +0x004 (+0x008 is GPIO_HI_IN for GPIOs 32-47). */
#define SIO_GPIO_IN_ADDR       (*(volatile uint32_t *)(0xD0000000u + 0x004))

void cyw43_hal_pin_config(int pin, int mode, int pull, int alt) {
    (void)pin; (void)mode; (void)pull; (void)alt;
    /* Pin configuration is handled by cyw43_spi_gpio_setup() */
}

void cyw43_hal_pin_config_irq_falling(int pin, int enable) {
    (void)pin; (void)enable;
    /* Unused in the gSPI configuration: the driver only calls this on the
     * !CYW43_USE_SPI (SDIO) path. The host-wake IRQ (NET-5) is instead
     * armed by the Rust side (hal/rp/gpio.rs::hostwake, set up from
     * wifi_task.rs) and re-armed via CYW43_POST_POLL_HOOK. The name says
     * "falling" but the wake line is ACTIVE-HIGH (INTERRUPT_POLARITY_HIGH)
     * — see cyw43_configport.h. */
}

/*
 * Note: cyw43_cb_read_host_interrupt_pin is provided by the vendored
 * driver (cyw43_ctrl.c) via the CYW43_PIN_WL_HOST_WAKE path (active-high,
 * matching INTERRUPT_POLARITY_HIGH); it level-reads
 * WL_D/GP24 through cyw43_hal_pin_read below.  The SPI transport leaves
 * that pin as input between transactions so the read sees the
 * chip-driven level, not our own output latch.
 */

/* Silent host-wake counters (gdb-read only, never logged — see Bug B in
 * docs/designs/cyw43-pio-transport.md).  Separates "host-wake never
 * asserts" from "poll never runs" without perturbing hot-path timing. */
volatile uint32_t instr_hostwake_reads, instr_hostwake_high;

int cyw43_hal_pin_read(int pin) {
    int level = (SIO_GPIO_IN_ADDR & (1u << pin)) ? 1 : 0;
    instr_hostwake_reads++;
    instr_hostwake_high += level;
    return level;
}

/* ---- Driver log formatting ---- */

/* Rust defmt shim: logs the (already formatted) NUL-terminated string. */
extern void picodroid_cyw43_log_str(const char *msg);

/* Minimal printf subset for CYW43_PRINTF/DEBUG/INFO/WARN: handles
 * %% %c %s %d %i %u %x %X with optional '-', '0', width, and 'l'
 * modifiers — everything the vendored driver's messages use.  No
 * floating point, no heap. */
static void fmt_putc(char *buf, size_t cap, size_t *pos, char c) {
    if (*pos + 1 < cap) {
        buf[*pos] = c;
    }
    (*pos)++;
}

static void fmt_number(char *buf, size_t cap, size_t *pos, unsigned long v,
                       int base, int is_signed, int upper, int width,
                       int zero_pad, int left) {
    char tmp[16];
    int n = 0, neg = 0;
    if (is_signed && (long)v < 0) {
        neg = 1;
        v = (unsigned long)(-(long)v);
    }
    do {
        int d = (int)(v % (unsigned)base);
        tmp[n++] = (char)(d < 10 ? '0' + d : (upper ? 'A' : 'a') + d - 10);
        v /= (unsigned)base;
    } while (v != 0 && n < (int)sizeof(tmp));
    if (neg) {
        tmp[n++] = '-';
    }
    int pad = width - n;
    if (!left) {
        while (pad-- > 0) {
            fmt_putc(buf, cap, pos, zero_pad ? '0' : ' ');
        }
    }
    while (n > 0) {
        fmt_putc(buf, cap, pos, tmp[--n]);
    }
    if (left) {
        while (pad-- > 0) {
            fmt_putc(buf, cap, pos, ' ');
        }
    }
}

void picodroid_cyw43_log_fmt(const char *fmt, ...) {
    char buf[160];
    size_t pos = 0;
    va_list ap;
    va_start(ap, fmt);
    for (const char *p = fmt; *p != '\0'; p++) {
        if (*p != '%') {
            /* defmt adds its own newline; drop trailing ones */
            if (*p != '\n') {
                fmt_putc(buf, sizeof(buf), &pos, *p);
            }
            continue;
        }
        p++;
        if (*p == '%') {
            fmt_putc(buf, sizeof(buf), &pos, '%');
            continue;
        }
        int left = 0, zero_pad = 0, width = 0, longs = 0;
        while (*p == '-' || *p == '0') {
            if (*p == '-') left = 1; else zero_pad = 1;
            p++;
        }
        while (*p >= '0' && *p <= '9') {
            width = width * 10 + (*p - '0');
            p++;
        }
        while (*p == 'l') {
            longs++;
            p++;
        }
        switch (*p) {
            case 'c':
                fmt_putc(buf, sizeof(buf), &pos, (char)va_arg(ap, int));
                break;
            case 's': {
                const char *s = va_arg(ap, const char *);
                if (s == NULL) s = "(null)";
                int len = (int)strlen(s);
                int pad = width - len;
                if (!left) while (pad-- > 0) fmt_putc(buf, sizeof(buf), &pos, ' ');
                /* Map newlines to '|' so multiline strings (e.g. clmver)
                 * survive the single-line defmt log. */
                for (; *s; s++) fmt_putc(buf, sizeof(buf), &pos, (*s == '\n' || *s == '\r') ? '|' : *s);
                if (left) while (pad-- > 0) fmt_putc(buf, sizeof(buf), &pos, ' ');
                break;
            }
            case 'd':
            case 'i':
                fmt_number(buf, sizeof(buf), &pos,
                           longs ? (unsigned long)va_arg(ap, long)
                                 : (unsigned long)(long)va_arg(ap, int),
                           10, 1, 0, width, zero_pad, left);
                break;
            case 'u':
                fmt_number(buf, sizeof(buf), &pos,
                           longs ? va_arg(ap, unsigned long)
                                 : (unsigned long)va_arg(ap, unsigned int),
                           10, 0, 0, width, zero_pad, left);
                break;
            case 'x':
            case 'X':
                fmt_number(buf, sizeof(buf), &pos,
                           longs ? va_arg(ap, unsigned long)
                                 : (unsigned long)va_arg(ap, unsigned int),
                           16, 0, (*p == 'X'), width, zero_pad, left);
                break;
            case 'p':
                fmt_number(buf, sizeof(buf), &pos,
                           (unsigned long)(uintptr_t)va_arg(ap, void *),
                           16, 0, 0, 8, 1, 0);
                break;
            case '\0':
                p--; /* trailing '%': stop cleanly */
                break;
            default:
                /* Unknown specifier: emit it raw so nothing is lost */
                fmt_putc(buf, sizeof(buf), &pos, '%');
                fmt_putc(buf, sizeof(buf), &pos, *p);
                break;
        }
    }
    va_end(ap);
    buf[(pos < sizeof(buf) - 1) ? pos : sizeof(buf) - 1] = '\0';
    picodroid_cyw43_log_str(buf);
}

void cyw43_hal_pin_low(int pin) {
    SIO_GPIO_OUT_CLR_ADDR = (1u << pin);
}

void cyw43_hal_pin_high(int pin) {
    SIO_GPIO_OUT_SET_ADDR = (1u << pin);
}

/* ---- Threading (FreeRTOS recursive mutex) ---- */

static SemaphoreHandle_t cyw43_mutex = NULL;

static void ensure_mutex(void) {
    if (cyw43_mutex == NULL) {
        cyw43_mutex = xSemaphoreCreateRecursiveMutex();
        configASSERT(cyw43_mutex != NULL);
    }
}

void cyw43_thread_enter(void) {
    ensure_mutex();
    xSemaphoreTakeRecursive(cyw43_mutex, portMAX_DELAY);
}

void cyw43_thread_exit(void) {
    xSemaphoreGiveRecursive(cyw43_mutex);
}

void cyw43_thread_lock_check(void) {
    /* In debug builds, assert we hold the lock */
    (void)0;
}

/* ---- Event scheduling ---- */

/* Task handle for the CYW43 poll task (set by Rust init code) */
static TaskHandle_t cyw43_poll_task = NULL;

void cyw43_set_poll_task(TaskHandle_t task) {
    cyw43_poll_task = task;
}

void cyw43_schedule_internal_poll_dispatch(void (*func)(void)) {
    (void)func;
    /* Notify the CYW43 poll task to run cyw43_poll() */
    if (cyw43_poll_task != NULL) {
        BaseType_t xHigherPriorityTaskWoken = pdFALSE;
        if (__get_current_exception() != 0) {
            /* Called from ISR context */
            vTaskNotifyGiveFromISR(cyw43_poll_task, &xHigherPriorityTaskWoken);
            portYIELD_FROM_ISR(xHigherPriorityTaskWoken);
        } else {
            xTaskNotifyGive(cyw43_poll_task);
        }
    }
}

/* Host-wake ISR path (NET-5): called from the Rust IO_IRQ_BANK0 handler
 * (core 1 — PROC1 routing) when the GP24 level-high interrupt fires; wakes
 * the core-1-pinned cyw43 poll task. Silent counter alongside the others
 * above. */
volatile uint32_t instr_hostwake_irqs;

void picodroid_cyw43_hostwake_notify_from_isr(void) {
    instr_hostwake_irqs++;
    if (cyw43_poll_task != NULL) {
        BaseType_t xHigherPriorityTaskWoken = pdFALSE;
        vTaskNotifyGiveFromISR(cyw43_poll_task, &xHigherPriorityTaskWoken);
        portYIELD_FROM_ISR(xHigherPriorityTaskWoken);
    }
}

void cyw43_yield(void) {
    if (xTaskGetSchedulerState() == taskSCHEDULER_RUNNING) {
        taskYIELD();
    }
}

/* ---- MAC address (LAA fallback when OTP is empty) ---- */

/*
 * During cyw43_ensure_up the driver reads the OTP MAC into
 * cyw43_state.mac (cyw43_ctrl.c, CYW43_USE_OTP_MAC path) and then
 * queries it back through this hook — "cyw43_hal_get_mac can get this
 * from cyw43_state.mac" per the driver comment.  Surface the OTP value
 * once populated; fall back to a fixed locally-administered MAC before
 * set-up or on the rare dev board with blank OTP.
 */
static const uint8_t placeholder_mac[6] = { 0x02, 'P', 'I', 'C', 'O', 'W' };

void cyw43_hal_get_mac(int idx, uint8_t mac[6]) {
    (void)idx;
    const uint8_t *otp = cyw43_state.mac;
    for (int i = 0; i < 6; i++) {
        if (otp[i] != 0) {
            memcpy(mac, otp, 6);
            return;
        }
    }
    memcpy(mac, placeholder_mac, 6);
}

void cyw43_hal_generate_laa_mac(int idx, uint8_t mac[6]) {
    (void)idx;
    memcpy(mac, placeholder_mac, 6);
}

/* ---- Link status (FreeRTOS+TCP replacement for cyw43_lwip.c) ---- */

/*
 * FreeRTOS+TCP has no per-netif link state, so we mirror what
 * cyw43_lwip.c does without lwIP: report LINK_UP only once the driver
 * itself says the association is up. DHCP/IP state is tracked by
 * FreeRTOS+TCP separately, so callers querying this treat it as the
 * PHY-level link indicator.
 */
int cyw43_tcpip_link_status(cyw43_t *self, int itf) {
    return cyw43_wifi_link_status(self, itf);
}

/* ---- pbuf stub (CYW43_LWIP=0: buf is always a flat buffer) ---- */

/*
 * The driver forward-declares this in cyw43_ll.c:56 and only calls it
 * on the `is_pbuf == true` branch of cyw43_ll_send_ethernet. Our
 * NetworkInterface always sends with is_pbuf=false, so this is link-only
 * bait; keep it as a panic stub to catch any future misuse.
 */
struct pbuf;
uint16_t pbuf_copy_partial(const struct pbuf *p, void *dataptr,
                            uint16_t len, uint16_t offset) {
    (void)p; (void)dataptr; (void)len; (void)offset;
    configASSERT(!"pbuf_copy_partial called under CYW43_LWIP=0");
    return 0;
}

/* Network callbacks are in NetworkInterface_CYW43.c */
