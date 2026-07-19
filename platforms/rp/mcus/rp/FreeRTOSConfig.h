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

/* Heap: single-sourced from mcus/rp/<mcu>.toml `heap_kb` (RP2040 128 KB of
 * 256 KB RAM, RP2350 416 KB of 520 KB RAM). build_support/freertos.rs
 * injects -DconfigTOTAL_HEAP_SIZE into this C build, and
 * platforms/rp/build.rs::emit_heap_config generates the same value for the
 * simulator's default heap cap — resize it in the toml and every consumer
 * moves together (docs/parity-audit.md M2).
 *
 * RP2350 history:
 * - 2026-04-22: 256 → 384 KB. The earlier 256 KB budget minus task stacks
 *   and framework init left <88 KB contiguous free, which gcstress's mid-run
 *   JVM `Vec` doublings (capacity 1024 of then-88-byte JvmObject = ~90 KB)
 *   could no longer satisfy — seen as an `alloc 90112 bytes failed` panic.
 * - 2026-05-10: 384 → 416 KB. picoenvmon's HomeActivity hit the same
 *   `90112 bytes failed` panic against a fragmented 384 KB heap, plus a
 *   secondary `49152 bytes failed` once `ObjectHeap`/`ArrayHeap.arrays`
 *   were chunked (jvm/src/chunked_slots.rs landed simultaneously to make
 *   the dominant Vecs grow by 5.6 KB chunks instead of doubling). +32 KB
 *   gives picoenvmon a clean boot. Root cause measured 2026-07-18
 *   (docs/parity-audit.md V4): FreeRTOS overhead — task stacks + TCBs +
 *   queues — is ~85 KB, not the ~25-30 KB this comment once estimated, and
 *   the then-uncapped sim never saw any of it.
 *
 * If you push this higher, re-measure `arm-none-eabi-size` first: total
 * static RAM must stay well below LENGTH(RAM). Measured 2026-07-18 on
 * RP2350: BSS 509.6 KB of 520 KB leaves ~10 KB for the Cortex-M main stack
 * — thinner than the ~28 KB previously claimed here, but still above what
 * cortex-m-rt + ISRs need pre-scheduler.
 */
#ifndef configTOTAL_HEAP_SIZE
#error "configTOTAL_HEAP_SIZE must be injected from mcus/rp/<mcu>.toml heap_kb (build_support/freertos.rs)"
#endif

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

/* Task and run-time stats */
#define configUSE_TRACE_FACILITY                1
#define configGENERATE_RUN_TIME_STATS           1
extern uint32_t picodroid_get_runtime_counter(void);
#define portCONFIGURE_TIMER_FOR_RUN_TIME_STATS()   /* no-op: µs timer always runs */
#define portGET_RUN_TIME_COUNTER_VALUE()           picodroid_get_runtime_counter()

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
#ifdef __ARM_ARCH_8M_MAIN__
/* RP2350: the community FreeRTOS SMP port does not start tasks on core 0
 * when configTICK_CORE=1.  Tick on core 0 means the scheduler freezes
 * while park_for_flash() disables core 0 interrupts; install-time code
 * uses a hardware timer alarm on core 1 to compensate. */
#define configTICK_CORE                         0
#else
/* RP2040: core 1 drives the tick so park_for_flash() can disable core 0
 * interrupts without freezing the scheduler. */
#define configTICK_CORE                         1
#endif
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
