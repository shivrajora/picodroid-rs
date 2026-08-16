/*
 * FreeRTOS+TCP configuration for picodroid (RP2350 + CYW43439 WiFi).
 *
 * All buffers come out of the shared FreeRTOS heap_4 arena (heap_kb in the
 * MCU toml, injected as configTOTAL_HEAP_SIZE — 416 KB on RP2350), which the
 * IP stack shares with the JVM and every task stack. The #ifndef-wrapped
 * values below are testbench defaults; heap-constrained boards override
 * them per-board via `net_*` keys in board.toml (see
 * build_support/network.rs::net_config_overrides).
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
/* Parse IP-literal "hostnames" (e.g. "192.168.1.215") inside
 * FreeRTOS_gethostbyname instead of sending them to a DNS server.  This
 * option has NO default in FreeRTOSIPConfigDefaults.h — leaving it unset
 * compiles the literal-parse path out entirely and every URL that uses a
 * raw IP fails to "resolve". */
#define ipconfigINCLUDE_FULL_INET_ADDR          (1)
#define ipconfigDNS_CACHE_ENTRIES               (4)
#define ipconfigDNS_REQUEST_ATTEMPTS            (4)

/* ---- Network buffers ---- */
#ifndef ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS
#define ipconfigNUM_NETWORK_BUFFER_DESCRIPTORS  (16)
#endif
#define ipconfigNETWORK_MTU                     (1500)
#define ipconfigTCP_MSS                         (1460)

/* ---- TCP socket buffers ---- */
#ifndef ipconfigTCP_RX_BUFFER_LENGTH
#define ipconfigTCP_RX_BUFFER_LENGTH            (4096)
#endif
#ifndef ipconfigTCP_TX_BUFFER_LENGTH
#define ipconfigTCP_TX_BUFFER_LENGTH            (4096)
#endif

/* ---- IP task ---- */
#define ipconfigIP_TASK_PRIORITY                (7)
#define ipconfigIP_TASK_STACK_SIZE_WORDS         (512)  /* 2 KB */

/* ---- ARP ---- */
#define ipconfigARP_CACHE_ENTRIES               (8)
#define ipconfigARP_STORES_REMOTE_ADDRESSES     (1)
#define ipconfigMAX_ARP_RETRANSMISSIONS         (5)
#define ipconfigMAX_ARP_AGE                     (150)

/* ---- Buffer allocation ---- */
/* Use BufferAllocation_2.c (heap-based, works with FreeRTOS heap_4).
 *
 * Do NOT override ipconfigBUFFER_PADDING: the default is
 * 8 + ipconfigPACKET_FILLER_SIZE = 10, and every byte matters.  Each
 * buffer's first 4 bytes hold the NetworkBufferDescriptor_t* stamp and
 * the stack ALSO writes an IPv4/IPv6 discriminator byte at
 * payload - 48 = ethbuf - 6.  With padding 10 that byte lands after the
 * stamp; with padding 8 it lands INSIDE the stamp (byte 2), corrupting
 * the descriptor pointer and hard-faulting the first zero-copy send
 * (DHCP discover).  Padding 10 also keeps the IP header 4-byte aligned
 * (10 + 14-byte Ethernet header = 24). */
#define ipconfigPACKET_FILLER_SIZE              (2)

/* ---- TCP window ---- */
#define ipconfigUSE_TCP_WIN                     (1)
#ifndef ipconfigTCP_WIN_SEG_COUNT
#define ipconfigTCP_WIN_SEG_COUNT               (16)
#endif

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
