/*
 * FreeRTOS+TCP stack initialisation for picodroid — shared by every family
 * that runs FreeRTOS+TCP (docs/designs/network-seam-2026-09.md).
 *
 * Called once from the Rust link task (`run_link_task`) after the link
 * driver is initialised and its MAC address is known. Registers the link
 * driver's network interface, creates a DHCP-enabled IPv4 endpoint, and
 * starts the FreeRTOS+TCP IP task. Also provides the application hooks the
 * stack requires. Nothing here names a chip or a family: the two things
 * that vary arrive through two link-time symbols (below).
 */

#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"
#include "FreeRTOS_IP.h"
#include "FreeRTOS_DHCP.h"
#include "FreeRTOS_Routing.h"
#include "NetworkInterface.h"

/* The Rust socket layer (picodroid-core hal/freertos_tcp) passes
 * milliseconds straight through as FreeRTOS ticks for SO_RCVTIMEO. */
_Static_assert(configTICK_RATE_HZ == 1000,
               "FreeRtosTcpNet passes milliseconds as ticks: configTICK_RATE_HZ must be 1000");

/* ---- Seam 1: the link driver ----
 * Defined by the board's NetworkInterface_<X>.c (the RP family's WiFi driver
 * in platforms/rp/src/hal/rp/port/net/ is the reference). It fills FreeRTOS+TCP's own
 * NetworkInterface_t and calls FreeRTOS_AddNetworkInterface. One link per
 * board; a missing driver is a link error. */
NetworkInterface_t *pxPicodroidNetLink_FillInterfaceDescriptor(
    BaseType_t xEMACIndex,
    NetworkInterface_t *pxInterface);

/* ---- Seam 2: entropy ----
 * Defined by the family (platforms/rp/src/hal/rp/entropy.rs). Returns one
 * random word; the family owns the hardware-or-fallback choice. Called only from
 * the IP task (both hooks below). */
extern uint32_t picodroid_port_entropy32(void);

/* Defined in Rust: logs the up/down transition. `ip_nbo` is the endpoint
 * address in network byte order within a little-endian u32, so the first
 * octet is the low byte. */
extern void picodroid_net_ip_event(uint32_t up, uint32_t ip_nbo);

/* ---- Static storage for the interface and endpoint ---- */

static NetworkInterface_t xInterface;
static NetworkEndPoint_t  xEndPoint;

/* ---- Public: called from the Rust link task ---- */

void picodroid_net_stack_init(const uint8_t mac[6]) {
    /* Register the board's link driver with FreeRTOS+TCP. */
    pxPicodroidNetLink_FillInterfaceDescriptor(0, &xInterface);

    /* All-zero addresses — DHCP will fill them in. */
    static const uint8_t ucZero[4] = { 0, 0, 0, 0 };

    FreeRTOS_FillEndPoint(
        &xInterface,
        &xEndPoint,
        ucZero,  /* IP address   (DHCP overrides) */
        ucZero,  /* Netmask      (DHCP overrides) */
        ucZero,  /* Gateway      (DHCP overrides) */
        ucZero,  /* DNS server   (DHCP overrides) */
        mac
    );

    /* Request a DHCP lease for this endpoint. */
    xEndPoint.bits.bWantDHCP = pdTRUE;

    /* Start the IP task (creates an internal FreeRTOS task at
     * ipconfigIP_TASK_PRIORITY).  This also kicks off DHCP discovery. */
    FreeRTOS_IPInit_Multi();
}

/* ---- Required callbacks ---- */

/*
 * Called by FreeRTOS+TCP when the network goes up or down.
 * Required when ipconfigUSE_NETWORK_EVENT_HOOK == 1.
 */
void vApplicationIPNetworkEventHook_Multi(
    eIPCallbackEvent_t eNetworkEvent,
    struct xNetworkEndPoint *pxEndPoint)
{
    uint32_t ip = 0;
    if (pxEndPoint != NULL) {
        ip = pxEndPoint->ipv4_settings.ulIPAddress;
    }
    picodroid_net_ip_event((eNetworkEvent == eNetworkUp) ? 1u : 0u, ip);
}

/*
 * Provide a random number for TCP sequence numbers, DHCP transaction IDs, etc.
 * The family answers (seam 2); this hook never fails.
 */
BaseType_t xApplicationGetRandomNumber(uint32_t *pulNumber) {
    *pulNumber = picodroid_port_entropy32();
    return pdTRUE;
}

/*
 * Generate the next TCP sequence number.
 * Required by FreeRTOS+TCP for new TCP connections.
 */
uint32_t ulApplicationGetNextSequenceNumber(
    uint32_t ulSourceAddress,
    uint16_t usSourcePort,
    uint32_t ulDestinationAddress,
    uint16_t usDestinationPort)
{
    (void)ulSourceAddress;
    (void)usSourcePort;
    (void)ulDestinationAddress;
    (void)usDestinationPort;

    return picodroid_port_entropy32();
}

/*
 * DHCP client hostname sent in DHCPDISCOVER. Appears in the router's
 * DHCP lease table; purely cosmetic.
 */
const char *pcApplicationHostnameHook(void) {
    return "picodroid";
}

/*
 * DHCP phase hook (ipconfigUSE_DHCP_HOOK defaults to 1). Default behavior:
 * let the stack discover and request normally. Apps can override later to
 * pin a static IP if DHCP fails.
 */
eDHCPCallbackAnswer_t xApplicationDHCPHook_Multi(
    eDHCPCallbackPhase_t eDHCPPhase,
    struct xNetworkEndPoint *pxEndPoint,
    IP_Address_t *pxIPAddress)
{
    (void)eDHCPPhase;
    (void)pxEndPoint;
    (void)pxIPAddress;
    return eDHCPContinue;
}
