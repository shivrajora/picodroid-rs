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

/* The network interface descriptor */
static NetworkInterface_t xCYW43Interface;

/* ---- Interface function pointers ---- */

static BaseType_t xCYW43_Init(NetworkInterface_t *pxInterface) {
    (void)pxInterface;
    /* CYW43 init is handled by the Rust cyw43_task.
     * By the time FreeRTOS+TCP starts, WiFi should be associated. */
    if (cyw43_tcpip_link_status(&cyw43_state, CYW43_ITF_STA) == CYW43_LINK_UP) {
        xInterfaceUp = pdTRUE;
    }
    return xInterfaceUp;
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

    if (xReleaseAfterSend != pdFALSE) {
        vReleaseNetworkBufferAndDescriptor(pxNetworkBuffer);
    }

    return (ret == 0) ? pdTRUE : pdFALSE;
}

static BaseType_t xCYW43_GetPhyLinkStatus(NetworkInterface_t *pxInterface) {
    (void)pxInterface;
    return (cyw43_tcpip_link_status(&cyw43_state, CYW43_ITF_STA) == CYW43_LINK_UP)
               ? pdTRUE
               : pdFALSE;
}

/* ---- Public: register the CYW43 interface with FreeRTOS+TCP ---- */

NetworkInterface_t *pxCYW43_FillInterfaceDescriptor(
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

    /* Allocate a FreeRTOS+TCP network buffer */
    NetworkBufferDescriptor_t *pxBuffer = pxGetNetworkBufferWithDescriptor(len, 0);
    if (pxBuffer == NULL) {
        return;
    }

    /* Copy the Ethernet frame into the network buffer */
    memcpy(pxBuffer->pucEthernetBuffer, buf, len);
    pxBuffer->xDataLength = len;
    pxBuffer->pxInterface = &xCYW43Interface;
    pxBuffer->pxEndPoint = FreeRTOS_FirstEndPoint(&xCYW43Interface);

    /* Hand the buffer to the IP task */
    IPStackEvent_t xEvent;
    xEvent.eEventType = eNetworkRxEvent;
    xEvent.pvData = pxBuffer;

    if (xSendEventStructToIPTask(&xEvent, 0) != pdPASS) {
        vReleaseNetworkBufferAndDescriptor(pxBuffer);
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
