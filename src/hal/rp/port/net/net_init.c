/*
 * FreeRTOS+TCP stack initialisation for picodroid (CYW43439 WiFi).
 *
 * Called once from the Rust cyw43_task after the CYW43 driver has been
 * initialised and the MAC address is available.  Registers the CYW43
 * network interface, creates a DHCP-enabled IPv4 endpoint, and starts
 * the FreeRTOS+TCP IP task.
 */

#include <stdint.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"
#include "FreeRTOS_IP.h"
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

/*
 * Called by FreeRTOS+TCP when the network goes up or down.
 * Required when ipconfigUSE_NETWORK_EVENT_HOOK == 1.
 */
void vApplicationIPNetworkEventHook_Multi(
    eIPCallbackEvent_t eNetworkEvent,
    struct xNetworkEndPoint *pxEndPoint)
{
    (void)pxEndPoint;
    (void)eNetworkEvent;
    /* TODO: log assigned IP address on eNetworkUp for debugging. */
}

/*
 * Provide a random number for TCP sequence numbers, DHCP transaction IDs, etc.
 * Uses the RP2350 hardware TRNG (TRNG block at 0x400F0000).
 */
BaseType_t xApplicationGetRandomNumber(uint32_t *pulNumber) {
    /* RP2350 TRNG: read from TRNG_RND_SOURCE_ENABLE and TRNG_RND_OUTPUT
     * registers.  For bring-up, fall back to a simple timer-based PRNG. */
    static uint32_t ulState = 0x12345678;

    /* Mix in the hardware timer for entropy. */
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
