/*
 * FreeRTOS+TCP stack initialisation for picodroid (CYW43439 WiFi).
 *
 * Called once from the Rust cyw43_task after the CYW43 driver has been
 * initialised and the MAC address is available.  Registers the CYW43
 * network interface, creates a DHCP-enabled IPv4 endpoint, and starts
 * the FreeRTOS+TCP IP task.
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

/* Forward declaration — defined in NetworkInterface_CYW43.c */
NetworkInterface_t *pxCYW43_FillInterfaceDescriptor(
    BaseType_t xEMACIndex,
    NetworkInterface_t *pxInterface);

/* ---- Static storage for the endpoint ---- */

static NetworkInterface_t xInterface;
static NetworkEndPoint_t  xEndPoint;

/* ---- Public: called from Rust wifi_task ---- */

void picodroid_net_stack_init(const uint8_t mac[6]) {
    /* Register the CYW43 network interface with FreeRTOS+TCP. */
    pxCYW43_FillInterfaceDescriptor(0, &xInterface);

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

/* Rust defmt shim (platforms/rp wifi_task.rs).  ip is in network byte
 * order within a little-endian u32: first octet = low byte. */
extern void picodroid_net_ip_event(uint32_t up, uint32_t ip_nbo);

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

/* RP2350 hardware TRNG (hal/rp/trng.rs, NET-6). Non-blocking: false while
 * the current collection round is still sampling. */
extern bool picodroid_trng_random_u32(uint32_t *out);

/*
 * Provide a random number for TCP sequence numbers, DHCP transaction IDs, etc.
 * Hardware TRNG when a word is buffered (NET-6); while the TRNG warms up
 * (first harvest lands tens of ms after first use) a timer-seeded LCG fills
 * in, and every TRNG word additionally XOR-mixes into the LCG state so the
 * fallback stream stops being predictable after the first harvest.
 */
BaseType_t xApplicationGetRandomNumber(uint32_t *pulNumber) {
    static uint32_t ulState = 0x12345678;
    uint32_t ulHw;

    if (picodroid_trng_random_u32(&ulHw)) {
        ulState ^= ulHw;
        *pulNumber = ulHw;
        return pdTRUE;
    }

    /* Fallback: mix in the hardware timer for entropy. */
    volatile uint32_t *pTimerLow = (volatile uint32_t *)0x400B000C;
    ulState ^= *pTimerLow;
    ulState = ulState * 1664525u + 1013904223u; /* LCG */

    *pulNumber = ulState;
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
    uint32_t ulRandom;
    (void)ulSourceAddress;
    (void)usSourcePort;
    (void)ulDestinationAddress;
    (void)usDestinationPort;

    xApplicationGetRandomNumber(&ulRandom);
    return ulRandom;
}

/*
 * DHCP client hostname sent in DHCPDISCOVER. Appears in the router's
 * DHCP lease table; purely cosmetic.
 */
const char *pcApplicationHostnameHook(void) {
    return "picodroid";
}

/*
 * DHCP phase hook (ipconfigUSE_DHCP_HOOK=1). Default behavior: let the
 * stack discover and request normally. Apps can override later to pin
 * a static IP if DHCP fails.
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
