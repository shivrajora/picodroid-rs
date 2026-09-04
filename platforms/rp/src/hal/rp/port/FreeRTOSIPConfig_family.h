/*
 * The RP family's part of the FreeRTOS+TCP configuration. Included first by
 * the shared picodroid-core/net-freertos-tcp/FreeRTOSIPConfig.h; only what
 * is the family's to decide lives here (docs/designs/network-seam-2026-09.md D7).
 */

#ifndef FREERTOS_IP_CONFIG_FAMILY_H
#define FREERTOS_IP_CONFIG_FAMILY_H

/* Either core, deliberately — the kernel default spelled out.  The IP task
 * never touches JVM state: sockets are driven synchronously from the calling
 * Java task (picodroid-core hal/freertos_tcp) and the stack's application
 * hooks (net_init.c) touch only their own statics, so it need not join the
 * core-0 pin that every JVM-adjacent task carries, and a busy Java thread
 * (priority 15 > 7, no time slicing) cannot starve TCP.  The affinity guard
 * (platforms/rp/src/task_affinity.rs) requires the choice to be written
 * down; 0 would mean "whatever the kernel does", which is the same today but
 * not a decision. */
#define ipconfigIP_TASK_AFFINITY                ( ( 1 << 0 ) | ( 1 << 1 ) )

#endif /* FREERTOS_IP_CONFIG_FAMILY_H */
