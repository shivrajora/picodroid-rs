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
/* For now, suppress all output to avoid pulling in stdio */
#define CYW43_PRINTF(...) (void)0
#define CYW43_VDEBUG(...) (void)0
#define CYW43_DEBUG(...) (void)0
#define CYW43_INFO(...) (void)0
#define CYW43_WARN(...) (void)0

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
#define CYW43_PIN_WL_IRQ        (24)
#define CYW43_PIN_WL_CS         (25)
#define CYW43_PIN_WL_CLK        (29)
#define CYW43_PIN_WL_SDIO_1     (24)  /* Data pin (alias for SDIO mode compat) */

/* ---- MAC address source ---- */
/* Use OTP-fused MAC address from CYW43 chip */
#define CYW43_USE_OTP_MAC       (1)
/* Interface selectors passed to cyw43_hal_get_mac. Values only need to be
 * distinct — the HAL implementation ignores them and returns the same MAC. */
#define CYW43_HAL_MAC_WLAN0     (0)
#define CYW43_HAL_MAC_WLAN1     (1)
#define CYW43_HAL_MAC_BDADDR    (2)

/* MAC HAL entry points implemented in cyw43_port.c. `idx` is a CYW43_HAL_MAC_*
 * selector; the LAA variant derives a deterministic locally-administered MAC
 * from the RP2350 flash unique id when OTP has no MAC configured. */
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
