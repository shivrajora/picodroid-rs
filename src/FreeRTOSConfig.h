#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

/* Chip-specific clock and port settings.
 * ARM_CM33_NTZ defines __ARM_ARCH_8M_MAIN__; ARM_CM0 defines __ARM_ARCH_6M__. */
#ifdef __ARM_ARCH_8M_MAIN__
/* RP2350 Cortex-M33 @ 150 MHz */
#define configCPU_CLOCK_HZ                      150000000UL
#define configENABLE_FPU                        1
#define configENABLE_MPU                        0
#define configENABLE_TRUSTZONE                  0
#define configRUN_FREERTOS_SECURE_ONLY          1
/* RP2350 NVIC implements 4 priority bits (16 levels).
 * Allow FreeRTOS to manage priorities 1-15; level 0 stays unmasked. */
#define configMAX_SYSCALL_INTERRUPT_PRIORITY    ( 1 << ( 8 - 4 ) )
#else
/* RP2040 Cortex-M0+ @ 125 MHz */
#define configCPU_CLOCK_HZ                      125000000UL
/* ARM_CM0 port v11 requires this; RP2040 CM0+ has no MPU */
#define configENABLE_MPU                        0
#endif

#define configTICK_RATE_HZ                      1000

/* Task priorities and stack */
#define configMAX_PRIORITIES                    32
#define configMINIMAL_STACK_SIZE                128
#define configMAX_TASK_NAME_LEN                 16
#define configSTACK_DEPTH_TYPE                  uint32_t

/* Heap: 128 KB (RP2040 has 256 KB, RP2350 has 520 KB — conservative default) */
#define configTOTAL_HEAP_SIZE                   ( 128 * 1024 )

/* Scheduler behaviour */
#define configUSE_PREEMPTION                    1
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 0
#define configUSE_TICKLESS_IDLE                 0
#define configUSE_16_BIT_TICKS                  0

/* Hook functions */
#define configUSE_IDLE_HOOK                     0
#define configUSE_TICK_HOOK                     0
#define configUSE_MALLOC_FAILED_HOOK            1
#define configCHECK_FOR_STACK_OVERFLOW          2

/* Allocation */
#define configSUPPORT_DYNAMIC_ALLOCATION        1
#define configSUPPORT_STATIC_ALLOCATION         0

/* Synchronisation primitives */
#define configUSE_MUTEXES                       1
#define configUSE_RECURSIVE_MUTEXES             1
#define configUSE_COUNTING_SEMAPHORES           1
#define configUSE_QUEUE_SETS                    0
#define configUSE_TASK_NOTIFICATIONS            1

/* Software timers */
#define configUSE_TIMERS                        1
#define configTIMER_TASK_PRIORITY               ( configMAX_PRIORITIES - 1 )
#define configTIMER_QUEUE_LENGTH                10
#define configTIMER_TASK_STACK_DEPTH            configMINIMAL_STACK_SIZE

/* Message buffers */
#define configMESSAGE_BUFFER_LENGTH_TYPE        size_t

/* SMP – dual-core scheduler */
#define configNUMBER_OF_CORES                   2
#define configUSE_PASSIVE_IDLE_HOOK             0   /* required by FreeRTOS SMP kernel */
/* Enable pico-sync interop so prvFIFOInterruptHandler (RP2040 port) compiles.
 * In SMP mode the handler just calls portYIELD_FROM_ISR; the full interop
 * code paths are excluded by configNUMBER_OF_CORES != 1 guards. */
#define configSUPPORT_PICO_SYNC_INTEROP         1
#define configTICK_CORE                         1   /* core 1 drives the tick so park_for_flash() can disable core 0 interrupts without freezing the scheduler */
#define configUSE_CORE_AFFINITY                 1
/* Hardware spinlock IDs reserved for FreeRTOS (spinlocks 26 and 27 = PICO_SPINLOCK_ID_OS1/OS2) */
#define configSMP_SPINLOCK_0                    26
#define configSMP_SPINLOCK_1                    27
/* Disable the runtime vector-table check; we use linker-alias approach instead */
#define configCHECK_HANDLER_INSTALLATION        0

/* Optional API functions */
#define INCLUDE_vTaskDelay                      1
#define INCLUDE_vTaskDelayUntil                 1
#define INCLUDE_vTaskDelete                     1
#define INCLUDE_vTaskSuspend                    1
#define INCLUDE_xTaskAbortDelay                 1
#define INCLUDE_xTaskGetCurrentTaskHandle       1
#define INCLUDE_uxTaskGetStackHighWaterMark     1
#define INCLUDE_xTaskGetSchedulerState          1
#define INCLUDE_xTimerPendFunctionCall          1   /* needed by xEventGroupSetBitsFromISR */

#endif /* FREERTOS_CONFIG_H */
