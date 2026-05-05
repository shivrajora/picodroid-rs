/*
 * FreeRTOS+TCP configuration for picodroid (RP2350 + CYW43439 WiFi).
 *
 * Tuned for 256 KB FreeRTOS heap — balances buffer count against
 * JVM heap and LVGL memory requirements.
 */

#ifndef FREERTOS_IP_CONFIG_H
#define FREERTOS_IP_CONFIG_H

/* ---- Core protocol support ---- */
#define ipconfigUSE_IPv4                        (1)
#define ipconfigUSE_IPv6                        (0)
#define ipconfigUSE_TCP                         (1)
#define ipconfigUSE_UDP                         (1)

/* ---- DHCP / DNS ---- */
#define ipconfigUSE_DHCP                        (1)
#define ipconfigDHCP_REGISTER_HOSTNAME          (1)
#define ipconfigUSE_DNS                         (1)
#define ipconfigUSE_DNS_CACHE                   (1)
#define ipconfigDNS_CACHE_ENTRIES               (4)
#define ipconfigDNS_REQUEST_ATTEMPTS            (4)

/* ---- Network buffers ---- */
#define ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS  (16)
#define ipconfigNETWORK_MTU                     (1500)
#define ipconfigTCP_MSS                         (1460)

/* ---- TCP socket buffers ---- */
#define ipconfigTCP_RX_BUFFER_LENGTH            (4096)
#define ipconfigTCP_TX_BUFFER_LENGTH            (4096)

/* ---- IP task ---- */
#define ipconfigIP_TASK_PRIORITY                (7)
#define ipconfigIP_TASK_STACK_SIZE_WORDS         (512)  /* 2 KB */

/* ---- ARP ---- */
#define ipconfigARP_CACHE_ENTRIES               (8)
#define ipconfigARP_STORES_REMOTE_ADDRESSES     (1)
#define ipconfigMAX_ARP_RETRANSMISSIONS         (5)
#define ipconfigMAX_ARP_AGE                     (150)

/* ---- Buffer allocation ---- */
/* Use BufferAllocation_2.c (heap-based, works with FreeRTOS heap_4) */
#define ipconfigBUFFER_PADDING                  (8)
#define ipconfigPACKET_FILLER_SIZE              (2)

/* ---- TCP window ---- */
#define ipconfigUSE_TCP_WIN                     (1)
#define ipconfigTCP_WIN_SEG_COUNT               (16)

/* ---- Misc ---- */
#define ipconfigETHERNET_DRIVER_FILTERS_FRAME_TYPES  (0)
#define ipconfigDRIVER_INCLUDED_TX_IP_CHECKSUM  (0)
#define ipconfigDRIVER_INCLUDED_RX_IP_CHECKSUM  (0)
#define ipconfigZERO_COPY_TX_DRIVER             (0)
#define ipconfigZERO_COPY_RX_DRIVER             (0)

/* Byte order — ARM Cortex-M is little-endian */
#define ipconfigBYTE_ORDER                      pdFREERTOS_LITTLE_ENDIAN

/* ---- Callbacks / hooks ---- */
#define ipconfigUSE_NETWORK_EVENT_HOOK          (1)

/* ---- Sockets ---- */
#define ipconfigALLOW_SOCKET_SEND_WITHOUT_BIND  (1)
#define ipconfigSUPPORT_SELECT_FUNCTION         (0)

/* ---- Logging (minimal for now) ---- */
#define ipconfigHAS_DEBUG_PRINTF                (0)
#define ipconfigHAS_PRINTF                      (0)

/* ---- Multi-interface ---- */
#define ipconfigCOMPATIBLE_WITH_SINGLE           (0)
#define ipconfigUSE_LINKED_RX_MESSAGES           (0)

#endif /* FREERTOS_IP_CONFIG_H */
