/*
 * FreeRTOS+TCP NetworkInterface driver for CYW43439 WiFi.
 *
 * Bridges raw Ethernet frames between the CYW43 driver and the
 * FreeRTOS+TCP IP stack using the multi-interface API.
 */

#include <stdint.h>
#include <stdbool.h>
#include <string.h>

#include "FreeRTOS.h"
#include "task.h"
#include "FreeRTOS_IP.h"
#include "FreeRTOS_IP_Private.h"
#include "FreeRTOS_Routing.h"
#include "NetworkBufferManagement.h"
#include "NetworkInterface.h"

#include "cyw43.h"

/* ---- Globals ---- */

static BaseType_t xInterfaceUp = pdFALSE;

/* Reference to the global CYW43 driver state (allocated in cyw43.c) */
extern cyw43_t cyw43_state;

/* The interface descriptor registered with FreeRTOS+TCP.  Set by
 * pxPicodroidNetLink_FillInterfaceDescriptor (the storage lives in the
 * shared net_init.c);
 * RX frames must be stamped with THIS descriptor or endpoint lookup
 * returns NULL and every received frame is dropped. */
static NetworkInterface_t *pxRegisteredInterface = NULL;

/* ---- Silent diagnostic counters (read over gdb, NEVER logged) ----
 *
 * Standing instrumentation for the core-1 RX-stall investigation
 * (docs/designs/cyw43-pio-transport.md, Bug B).  Logging in the TX/RX hot
 * path perturbs timing enough to mask the bug, so these are only ever read
 * from a debugger:
 *   printf "tx=%u/%u rx=%u nobuf=%u qfull=%u noif=%u\n", instr_tx_ok, ...
 */
volatile uint32_t instr_tx_ok, instr_tx_fail;
volatile uint32_t instr_rx_ok, instr_rx_drop_nobuf, instr_rx_drop_queue,
    instr_rx_noiface;

/* ---- Interface function pointers ---- */

/*
 * Link-up test used by both pfInitialise and pfGetPhyLinkStatus (NET-2).
 *
 * Two conditions, both required:
 *
 * - xInterfaceUp: set by cyw43_cb_tcpip_set_link_up only once the join has
 *   fully completed (assoc + link + keys). The previous check used
 *   `link_status >= CYW43_LINK_JOIN` alone, but LINK_JOIN maps from the
 *   bare ACTIVE join-state, which is already true while a join is merely
 *   *in progress* — so +TCP brought the interface up during every retry of
 *   a failing join, DHCP started and timed out, and the `net: down` hook
 *   fired dozens of times during a NONET soak.
 *
 * - link_status >= CYW43_LINK_JOIN: catches failure kinds (FAIL / NONET /
 *   BADAUTH / DOWN) that may not deliver an EV_LINK link-down event, which
 *   would otherwise leave xInterfaceUp stale-true.
 *
 * Note: cyw43_tcpip_link_status forwards cyw43_wifi_link_status, whose
 * "associated" value is CYW43_LINK_JOIN — CYW43_LINK_UP is an lwIP-layer
 * state this port never reaches. DHCP now starts at full join rather than
 * 1-2 s earlier during association; join→lease latency re-checked on HW
 * (docs/networking-followups-2026-08.md NET-2).
 */
static BaseType_t xCYW43_LinkIsUp(void) {
    return (xInterfaceUp != pdFALSE &&
            cyw43_tcpip_link_status(&cyw43_state, CYW43_ITF_STA) >= CYW43_LINK_JOIN)
               ? pdTRUE
               : pdFALSE;
}

static BaseType_t xCYW43_Init(NetworkInterface_t *pxInterface) {
    (void)pxInterface;
    /* CYW43 init is handled by the Rust cyw43_task; the IP task retries
     * this every few seconds until it returns pdTRUE, so returning
     * pdFALSE before the association completes is fine. */
    return xCYW43_LinkIsUp();
}

static BaseType_t xCYW43_Output(NetworkInterface_t *pxInterface,
                                 NetworkBufferDescriptor_t *const pxNetworkBuffer,
                                 BaseType_t xReleaseAfterSend) {
    (void)pxInterface;

    if (pxNetworkBuffer == NULL || pxNetworkBuffer->pucEthernetBuffer == NULL) {
        return pdFALSE;
    }

    /* Send the Ethernet frame via CYW43 */
    cyw43_thread_enter();
    int ret = cyw43_send_ethernet(
        &cyw43_state,
        CYW43_ITF_STA,
        pxNetworkBuffer->xDataLength,
        pxNetworkBuffer->pucEthernetBuffer,
        false /* not async */
    );
    cyw43_thread_exit();

    if (ret == 0) {
        instr_tx_ok++;
    } else {
        instr_tx_fail++;
    }

    if (xReleaseAfterSend != pdFALSE) {
        vReleaseNetworkBufferAndDescriptor(pxNetworkBuffer);
    }

    return (ret == 0) ? pdTRUE : pdFALSE;
}

static BaseType_t xCYW43_GetPhyLinkStatus(NetworkInterface_t *pxInterface) {
    (void)pxInterface;
    return xCYW43_LinkIsUp();
}

/* ---- Public: register the CYW43 interface with FreeRTOS+TCP ----
 * This is the one symbol the shared stack glue
 * (picodroid-core/net-freertos-tcp/net_init.c) binds to; every link driver
 * defines it under exactly this name. */

NetworkInterface_t *pxPicodroidNetLink_FillInterfaceDescriptor(
    BaseType_t xEMACIndex,
    NetworkInterface_t *pxInterface) {
    (void)xEMACIndex;

    static char pcName[] = "CYW43";

    memset(pxInterface, 0, sizeof(*pxInterface));
    pxInterface->pcName = pcName;
    pxInterface->pvArgument = (void *)&cyw43_state;
    pxInterface->pfInitialise = xCYW43_Init;
    pxInterface->pfOutput = xCYW43_Output;
    pxInterface->pfGetPhyLinkStatus = xCYW43_GetPhyLinkStatus;

    FreeRTOS_AddNetworkInterface(pxInterface);
    pxRegisteredInterface = pxInterface;

    return pxInterface;
}

/* ---- Global xGetPhyLinkStatus (required by FreeRTOS+TCP) ---- */

BaseType_t xGetPhyLinkStatus(struct xNetworkInterface *pxInterface) {
    (void)pxInterface;
    return xCYW43_GetPhyLinkStatus(pxInterface);
}

/* ---- CYW43 receive callback ---- */

/*
 * Called by the CYW43 driver when a complete Ethernet frame has been received.
 * Context: called from cyw43_poll() in the cyw43_task.
 */
void cyw43_cb_process_ethernet(void *cb_data, int itf, size_t len, const uint8_t *buf) {
    (void)cb_data;

    /* Only process frames from the STA interface */
    if (itf != CYW43_ITF_STA) {
        return;
    }

    /* Frames can arrive before the interface is registered with the stack */
    if (pxRegisteredInterface == NULL) {
        instr_rx_noiface++;
        return;
    }

    /* Allocate a FreeRTOS+TCP network buffer */
    NetworkBufferDescriptor_t *pxBuffer = pxGetNetworkBufferWithDescriptor(len, 0);
    if (pxBuffer == NULL) {
        instr_rx_drop_nobuf++;
        return;
    }

    /* Copy the Ethernet frame into the network buffer */
    memcpy(pxBuffer->pucEthernetBuffer, buf, len);
    pxBuffer->xDataLength = len;
    pxBuffer->pxInterface = pxRegisteredInterface;
    pxBuffer->pxEndPoint = FreeRTOS_FirstEndPoint(pxRegisteredInterface);

    /* Hand the buffer to the IP task */
    IPStackEvent_t xEvent;
    xEvent.eEventType = eNetworkRxEvent;
    xEvent.pvData = pxBuffer;

    if (xSendEventStructToIPTask(&xEvent, 0) != pdPASS) {
        instr_rx_drop_queue++;
        vReleaseNetworkBufferAndDescriptor(pxBuffer);
    } else {
        instr_rx_ok++;
    }
}

/* ---- Link state callbacks ---- */

void cyw43_cb_tcpip_set_link_up(cyw43_t *self, int itf) {
    (void)self;
    if (itf == CYW43_ITF_STA) {
        xInterfaceUp = pdTRUE;
    }
}

void cyw43_cb_tcpip_set_link_down(cyw43_t *self, int itf) {
    (void)self;
    if (itf == CYW43_ITF_STA) {
        xInterfaceUp = pdFALSE;
    }
}

void cyw43_cb_tcpip_init(cyw43_t *self, int itf) {
    (void)self;
    (void)itf;
}

void cyw43_cb_tcpip_deinit(cyw43_t *self, int itf) {
    (void)self;
    (void)itf;
}
