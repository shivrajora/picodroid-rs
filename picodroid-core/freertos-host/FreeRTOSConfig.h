/* SPDX-License-Identifier: GPL-3.0-only
 *
 * FreeRTOS configuration for the *host simulator* (POSIX port).
 *
 * Mirrors platforms/rp/mcus/rp/FreeRTOSConfig.h wherever the setting is
 * meaningful off-chip, so a behaviour difference between the simulator and a
 * board is a real difference and not a config divergence. The three places it
 * deliberately parts company, each forced rather than chosen:
 *
 *  1. configNUMBER_OF_CORES 1. The POSIX port is single-core; the device runs
 *     the SMP kernel with 2. Cross-core interleavings therefore stay
 *     HIL-only (docs/parity-audit.md THR-04/X1). Single-core is the
 *     conservative side for JVM heap safety, so this understates the device's
 *     concurrency rather than overstating it.
 *
 *  2. No configTOTAL_HEAP_SIZE. No heap_N.c is compiled at all: pvPortMalloc
 *     and friends are Rust shims routing to the host allocator under
 *     `allocator::bypass()`. On a 64-bit host every kernel object is
 *     host-sized (StackType_t is `unsigned long`, TCBs carry 64-bit
 *     pointers), so charging them to the modeled arena would wreck the
 *     calibrated device-byte accounting. Device bytes for kernel objects
 *     enter through boot_budget.rs instead
 *     (docs/designs/freertos-host-sim.md §1.1).
 *
 *  3. Run-time stats and the stack-overflow / malloc-failed hooks are off.
 *     The first needs a chip timer; the second is meaningless when the port
 *     runs tasks on pthread stacks sized by pthread, not by us.
 *
 * This file lives in picodroid-core rather than under platforms/<family>/mcus/
 * because it is not MCU config: the simulator is family-neutral and lives in
 * this crate.
 */

#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

#include <assert.h>

/* The POSIX port paces its tick with setitimer(), not a CPU clock; this is
 * only here because the kernel's config checks want it defined. */
#define configCPU_CLOCK_HZ                      1000000000UL
#define configTICK_RATE_HZ                      1000

/* Task priorities and stack.
 *
 * configMINIMAL_STACK_SIZE is a *model* number here. The POSIX port stores
 * only its Thread_t in the FreeRTOS "stack" allocation and gives each task a
 * default-sized pthread stack, so these words never bound real recursion
 * depth — they exist so device and host agree on what a task costs. */
#define configMAX_PRIORITIES                    32
#define configMINIMAL_STACK_SIZE                128
#define configMAX_TASK_NAME_LEN                 16
#define configSTACK_DEPTH_TYPE                  uint32_t

/* Scheduler behaviour */
#define configUSE_PREEMPTION                    1
/* Time slicing MUST stay off — same shared-JVM-heap contract as the device
 * config (mcus/rp/FreeRTOSConfig.h): equal-priority JVM tasks must switch
 * only at blocking yield points, never at the tick. */
#define configUSE_TIME_SLICING                  0
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 0
#define configUSE_TICKLESS_IDLE                 0
#define configUSE_16_BIT_TICKS                  0
#define configIDLE_SHOULD_YIELD                 1

/* Hook functions — see note 3 in the header comment. */
#define configUSE_IDLE_HOOK                     0
#define configUSE_TICK_HOOK                     0
#define configUSE_MALLOC_FAILED_HOOK            0
#define configCHECK_FOR_STACK_OVERFLOW          0

/* Task stats. uxTaskGetSystemState (used by the freertos-rust shim) needs the
 * trace facility; run-time stats need a chip timer we do not have. */
#define configUSE_TRACE_FACILITY                1
#define configGENERATE_RUN_TIME_STATS           0

/* Allocation — see note 2. */
#define configSUPPORT_DYNAMIC_ALLOCATION        1
#define configSUPPORT_STATIC_ALLOCATION         0

/* Synchronisation primitives. Recursive mutexes are not optional: Java
 * monitors re-enter (picodroid-core/src/rtos.rs). */
#define configUSE_MUTEXES                       1
#define configUSE_RECURSIVE_MUTEXES             1
#define configUSE_COUNTING_SEMAPHORES           1
#define configUSE_QUEUE_SETS                    0
#define configUSE_TASK_NOTIFICATIONS            1

/* Software timers. The simulator's LVGL tick is one of these in FreeRTOS
 * mode, exactly as it is on device. */
#define configUSE_TIMERS                        1
#define configTIMER_TASK_PRIORITY               ( configMAX_PRIORITIES - 1 )
#define configTIMER_QUEUE_LENGTH                10
#define configTIMER_TASK_STACK_DEPTH            configMINIMAL_STACK_SIZE

/* Message buffers */
#define configMESSAGE_BUFFER_LENGTH_TYPE        size_t

/* Single core — see note 1. configUSE_CORE_AFFINITY stays 0 so the
 * freertos-rust shim compiles its no-op affinity wrappers. */
#define configNUMBER_OF_CORES                   1
#define configUSE_CORE_AFFINITY                 0

/* A failed assertion in the kernel is a simulator bug worth stopping on, and
 * unlike the device build there is a console to say so on. */
#define configASSERT( x )                       assert( x )

/* Optional API functions. Same set the device enables, minus the two the
 * single-core POSIX port cannot serve. */
#define INCLUDE_vTaskDelay                      1
#define INCLUDE_vTaskDelayUntil                 1
#define INCLUDE_vTaskDelete                     1
#define INCLUDE_vTaskSuspend                    1
#define INCLUDE_xTaskAbortDelay                 1
#define INCLUDE_xTaskGetCurrentTaskHandle       1
#define INCLUDE_uxTaskGetStackHighWaterMark     1
#define INCLUDE_xTaskGetSchedulerState          1
#define INCLUDE_xTimerPendFunctionCall          1

#endif /* FREERTOS_CONFIG_H */
