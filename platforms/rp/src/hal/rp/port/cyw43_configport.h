/*
 * CYW43 driver configuration port for picodroid.
 *
 * This header is included by cyw43_config.h via CYW43_CONFIG_FILE.
 * It provides the platform-specific configuration, HAL functions,
 * and threading primitives needed by the CYW43 driver.
 */

#ifndef CYW43_CONFIGPORT_H
#define CYW43_CONFIGPORT_H

#include <stdint.h>
#include <stdbool.h>
#include <string.h>

/* C11 _Static_assert — the driver uses C23-style `static_assert` */
#ifndef static_assert
#define static_assert _Static_assert
#endif

/* Element count of a fixed-size array (driver uses this in event tables). */
#ifndef CYW43_ARRAY_SIZE
#define CYW43_ARRAY_SIZE(a) (sizeof(a) / sizeof((a)[0]))
#endif

/* ---- Bus configuration ---- */

/* Use SPI bus (not SDIO) — Pico W uses gSPI over PIO */
#define CYW43_USE_SPI (1)

/* ---- Disable lwIP — we use FreeRTOS+TCP ---- */
#define CYW43_LWIP (0)
#define CYW43_NETUTILS (0)

/* ---- Disable Bluetooth (not used in picodroid) ---- */
#define CYW43_ENABLE_BLUETOOTH (0)

/* ---- Logging — route to defmt via Rust shim ---- */
/* No stdio on this target: cyw43_port.c formats with a minimal printf
 * subset (%s %c %d %i %u %x %X, width/zero-pad, 'l' modifier) into a
 * stack buffer and forwards the result to the Rust defmt shim.  VDEBUG
 * stays muted: it is per-packet chatty. */
void picodroid_cyw43_log_fmt(const char *fmt, ...);
#define CYW43_PRINTF(...) picodroid_cyw43_log_fmt(__VA_ARGS__)
#define CYW43_VDEBUG(...) (void)0
#define CYW43_DEBUG(...) picodroid_cyw43_log_fmt(__VA_ARGS__)
#define CYW43_INFO(...) picodroid_cyw43_log_fmt(__VA_ARGS__)
#define CYW43_WARN(...) picodroid_cyw43_log_fmt(__VA_ARGS__)

/* ---- Timing ---- */

/* Provided by our port (implemented in Rust, exposed via extern "C") */
void cyw43_delay_us(uint32_t us);
void cyw43_delay_ms(uint32_t ms);
uint32_t cyw43_hal_ticks_us(void);
uint32_t cyw43_hal_ticks_ms(void);

#define CYW43_HAL_PIN_ON  (1)
#define CYW43_HAL_PIN_OFF (0)

/* GPIO pin mode / pull constants */
#define CYW43_HAL_PIN_MODE_INPUT   (0)
#define CYW43_HAL_PIN_MODE_OUTPUT  (1)
#define CYW43_HAL_PIN_PULL_NONE    (0)
#define CYW43_HAL_PIN_PULL_UP      (1)
#define CYW43_HAL_PIN_PULL_DOWN    (2)

/* GPIO control for WL_ON (power enable) */
void cyw43_hal_pin_config(int pin, int mode, int pull, int alt);
void cyw43_hal_pin_config_irq_falling(int pin, int enable);
int cyw43_hal_pin_read(int pin);
void cyw43_hal_pin_low(int pin);
void cyw43_hal_pin_high(int pin);

/* ---- Pin definitions (Pico 2 W CYW43439 wiring) ---- */
#define CYW43_PIN_WL_REG_ON     (23)
#define CYW43_PIN_WL_DATA_OUT   (24)
#define CYW43_PIN_WL_DATA_IN    (24)
/* IMPORTANT: this must be WL_HOST_WAKE, not WL_IRQ.  The driver's gSPI bus
 * config sets INTERRUPT_POLARITY_HIGH (chip drives DATA/IRQ high when a
 * packet is pending), and cyw43_ll.c treats WL_HOST_WAKE as active-high but
 * WL_IRQ as active-low.  Defining WL_IRQ inverts the RX poll gate: the
 * driver skips polling exactly when the chip has work (pico-sdk also uses
 * WL_HOST_WAKE for this pin). */
#define CYW43_PIN_WL_HOST_WAKE  (24)
#define CYW43_PIN_WL_CS         (25)
#define CYW43_PIN_WL_CLK        (29)
#define CYW43_PIN_WL_SDIO_1     (24)  /* Data pin (alias for SDIO mode compat) */

/* Host-wake level IRQ re-arm (NET-5): the IO_IRQ_BANK0 handler masks the
 * GP24 level-high interrupt when it fires (a level interrupt cannot be
 * acked while the line is high); re-arm after every poll has serviced the
 * chip. Implemented in Rust — hal/rp/gpio.rs::hostwake. */
extern void picodroid_cyw43_hostwake_rearm(void);
#define CYW43_POST_POLL_HOOK picodroid_cyw43_hostwake_rearm();

/* Keep the driver's default ioctl timeout (500 ms).  Do NOT shorten it —
 * some ioctls (e.g. CLM finalization) legitimately take hundreds of ms;
 * a shorter timeout silently breaks the country/regulatory setup and
 * every join then fails with NONET. */

/* ---- MAC address source ---- */
/* Use OTP-fused MAC address from CYW43 chip */
#define CYW43_USE_OTP_MAC       (1)
/* Interface selectors passed to cyw43_hal_get_mac. Values only need to be
 * distinct — the HAL implementation ignores them and returns the same MAC. */
#define CYW43_HAL_MAC_WLAN0     (0)
#define CYW43_HAL_MAC_WLAN1     (1)
#define CYW43_HAL_MAC_BDADDR    (2)

/* MAC HAL entry points implemented in cyw43_port.c. `idx` is a CYW43_HAL_MAC_*
 * selector. get_mac surfaces the OTP MAC (read into cyw43_state.mac during
 * set-up) once available; both fall back to a fixed locally-administered
 * placeholder before set-up or on a blank-OTP board. */
void cyw43_hal_get_mac(int idx, uint8_t mac[6]);
void cyw43_hal_generate_laa_mac(int idx, uint8_t mac[6]);

/* ---- Error codes ---- */
#ifndef CYW43_EPERM
#define CYW43_EPERM (1)
#endif
#ifndef CYW43_EIO
#define CYW43_EIO   (5)
#endif
#ifndef CYW43_EINVAL
#define CYW43_EINVAL (22)
#endif
#ifndef CYW43_ETIMEDOUT
#define CYW43_ETIMEDOUT (110)
#endif

/* ---- Threading / locking (FreeRTOS) ---- */

/* These are called by the driver to protect shared state.
 * We implement them using FreeRTOS recursive mutexes. */
void cyw43_thread_enter(void);
void cyw43_thread_exit(void);
void cyw43_thread_lock_check(void);

/* Macro forms used by the driver in addition to the function calls */
#define CYW43_THREAD_ENTER      cyw43_thread_enter()
#define CYW43_THREAD_EXIT       cyw43_thread_exit()
#define CYW43_THREAD_LOCK_CHECK cyw43_thread_lock_check()

/* Schedule a poll of the CYW43 driver (called from ISR context) */
void cyw43_schedule_internal_poll_dispatch(void (*func)(void));

/* ---- Event / wait hooks ---- */
#define CYW43_EVENT_POLL_HOOK cyw43_yield()
void cyw43_yield(void);

/* Wait hooks — called during long-running operations (IOCTL, SDPCM send) */
#define CYW43_DO_IOCTL_WAIT         cyw43_delay_ms(1)
#define CYW43_SDPCM_SEND_COMMON_WAIT cyw43_delay_ms(1)

/* ---- Firmware storage ---- */
/* Firmware is compiled into the driver via include headers (default paths in cyw43_config.h) */

/* ---- Network callbacks (provided by our FreeRTOS+TCP NetworkInterface) ---- */
/* Note: cyw43_t is not yet defined here (we're included from cyw43_config.h
 * before cyw43.h defines it).  The actual prototypes with cyw43_t* are
 * declared in cyw43.h — we just need the implementations to match those. */

#endif /* CYW43_CONFIGPORT_H */
