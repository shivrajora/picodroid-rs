---
title: "RP2350 FreeRTOS SMP Port Bugs"
description: "Documented bugs in the RP2350 FreeRTOS SMP port and their workarounds."
---

Tracked issues with the community FreeRTOS SMP port for RP2350 (Cortex-M33)
that affect picodroid. These are upstream bugs in
`third_party/FreeRTOS-Kernel/portable/ThirdParty/Community-Supported-Ports/GCC/RP2350_ARM_NTZ/`.

## 1. configTICK_CORE=1 breaks task scheduling on core 0

**Status:** Worked around (configTICK_CORE=0 + hardware timer alarm)

When `configTICK_CORE=1` (tick on core 1, matching the RP2040 configuration),
core 0 never starts its first task. The PDB task on core 1 runs normally but
the JVM task on core 0 is never scheduled.

**Root cause:** The RP2350 port uses the generic ARM_CM33_NTZ `vPortStartFirstTask`
which triggers SVC to load the first task context. Unlike the RP2040 port (which
directly reads `pxCurrentTCBs[get_core_num()]` from assembly), the SVC-based
approach does not properly select a task for core 0 when it is not the tick core.

**Workaround:** Use `configTICK_CORE=0` (tick on core 0). This means the tick
freezes when `park_for_flash()` disables interrupts on core 0 — see bug #2.

## 2. Tick freeze during park_for_flash stalls core 1

**Status:** Worked around (TIMER0 hardware alarm on core 1)

With `configTICK_CORE=0`, parking core 0 (`cpsid i`) freezes the FreeRTOS tick.
Any tick-dependent operation on core 1 (timeouts, delays, queue receives) hangs
permanently because the tick counter never advances.

**Workaround:** A TIMER0 alarm ISR on core 1 (`src/hal/rp/timer_alarm.rs`) fires
every 1 ms independently of FreeRTOS. When it detects `CORE0_PARKED`, it sends
to a FreeRTOS queue from ISR context, waking the PDB task. The alarm is only
armed during PDB install.

## 3. Cross-core FreeRTOS IPC is unreliable

**Status:** Avoided (not used on RP2350 install path)

`xTaskNotify` and `xQueueSend` from core 0 to a blocked task on core 1 silently
fail to wake the target task. The notification/data is delivered (the value is
set) but the doorbell interrupt that triggers PendSV on core 1 either isn't sent
or isn't processed. This was tested with both task notifications and queues.

Calling `xTaskNotify` from core 1 to core 0 also causes the FreeRTOS SMP
scheduler to deadlock — `vTaskDelay` on core 0 stops completing entirely.

**Workaround:** The install path on RP2350 avoids all cross-core FreeRTOS API
calls. Core 1 signals core 0 via atomic flags + `notify_jvm()` (which works
because the JVM task checks `STOP_JVM` at opcode boundaries). Core 0 signals
core 1 via the hardware timer alarm ISR (bug #2 workaround).

## 4. Tight busy-wait on core 1 starves core 0's scheduler

**Status:** Avoided (core 1 blocks in FreeRTOS during install wait)

When core 1 runs a tight loop (even with NOP gaps or hardware timer delays),
`vTaskDelay` on core 0 never completes. This occurs regardless of whether the
loop accesses shared memory or peripherals. The FreeRTOS SMP scheduler appears
to require both cores to periodically enter FreeRTOS-managed blocking states
for tick processing to work correctly.

**Workaround:** Core 1 blocks on a FreeRTOS queue receive (500 ms timeout)
during the install wait, keeping it in the scheduler. The TIMER0 alarm (bug #2)
handles waking core 1 after the tick freezes.

---

## Ideal fix

All four bugs stem from `configTICK_CORE=0` being forced by bug #1. If the
RP2350 port's `vPortStartFirstTask` / SVC handler were fixed to properly select
tasks for non-tick cores, `configTICK_CORE=1` would work and bugs #2–#4 would
not apply (the tick would run on core 1, unaffected by core 0 parking).

The RP2040 port handles this correctly by reading
`pxCurrentTCBs[get_core_num()]` directly in assembly rather than relying on SVC.
Porting that approach to the Cortex-M33 SVC handler (or using a similar direct
context-load) would be the root fix.
