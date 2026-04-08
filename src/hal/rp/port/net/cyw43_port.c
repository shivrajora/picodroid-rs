/*
 * CYW43 driver port implementation for picodroid (FreeRTOS on RP2350).
 *
 * Provides the HAL functions, threading primitives, and network callbacks
 * declared in cyw43_configport.h.
 */

#include <stdint.h>
#include <stdbool.h>
#include "cyw43.h"

#include "FreeRTOS.h"
#include "task.h"
#include "semphr.h"

/* ---- Hardware timer (RP2350 TIMER0 at 1 MHz) ---- */

#define TIMER_BASE  0x400B0000u
#define TIMER_TIMELR (*(volatile uint32_t *)(TIMER_BASE + 0x08))
#define TIMER_TIMEHR (*(volatile uint32_t *)(TIMER_BASE + 0x0C))

static uint64_t get_time_us(void) {
    uint32_t lo, hi;
    /* Read high then low then high again to handle rollover */
    do {
        hi = TIMER_TIMEHR;
        lo = TIMER_TIMELR;
    } while (hi != TIMER_TIMEHR);
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
#define SIO_GPIO_IN_ADDR       (*(volatile uint32_t *)(0xD0000000u + 0x008))

void cyw43_hal_pin_config(int pin, int mode, int pull, int alt) {
    (void)pin; (void)mode; (void)pull; (void)alt;
    /* Pin configuration is handled by cyw43_spi_gpio_setup() */
}

void cyw43_hal_pin_config_irq_falling(int pin, int enable) {
    (void)pin; (void)enable;
    /* TODO: Configure IRQ on WL_D for async event notification */
}

int cyw43_hal_pin_read(int pin) {
    return (SIO_GPIO_IN_ADDR & (1u << pin)) ? 1 : 0;
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

void cyw43_yield(void) {
    if (xTaskGetSchedulerState() == taskSCHEDULER_RUNNING) {
        taskYIELD();
    }
}

/* Network callbacks are in NetworkInterface_CYW43.c */
