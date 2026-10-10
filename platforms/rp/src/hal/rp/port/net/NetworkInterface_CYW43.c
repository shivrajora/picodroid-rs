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
/*
 * The driver's join-state word carries a verdict in its low nibble (ACTIVE,
 * FAIL, NONET, BADAUTH) and the join's progress in bits 9..11 (AUTH, LINK,
 * KEYED). A good AUTH resets the verdict only after BADAUTH, so when the
 * chip's firmware joins by itself after a NONET verdict — it keeps trying
 * the SSID it was given — the word reads 0xe03: authenticated, linked and
 * keyed, "no such network". The driver never collapses that to link-up,
 * so the station was on the network and nothing here knew (NET-12, bench
 * cycle 116: 45 s with the AP out of reach, then a self-join the driver
 * kept filing as NONET). A verdict can also land mid-join and wipe the
 * progress already made (every verdict is an assignment), after which the
 * chip finishes the handshake anyway: PSK_SUP status 8 filed as BADAUTH,
 * then KEYED, reads 0x804 (run 4, cycle 8). KEYED is only ever set by the
 * supplicant reporting the 4-way handshake complete, which needs the
 * association and the link, and it cannot be the open-network preset
 * once a verdict has overwritten the word; so KEYED over a failure
 * verdict means joined whatever the other bits say, and
 * picodroid_cyw43_sta_status performs the collapse the driver skipped —
 * the word back to bare ACTIVE and the interface up, exactly its own
 * WIFI_JOIN_STATE_ALL step — so a later link-down reads as one.
 */
#define STA_JOIN_KIND_MASK (0x000fu)
#define STA_JOIN_ACTIVE    (0x0001u)
#define STA_JOIN_KEYED     (0x0800u)

static void vCollapseSelfJoin(void) {
    uint32_t w = cyw43_state.wifi_join_state;
    uint32_t kind = w & STA_JOIN_KIND_MASK;
    if ((w & STA_JOIN_KEYED) != 0 && kind != 0 && kind != STA_JOIN_ACTIVE) {
        cyw43_state.wifi_join_state = STA_JOIN_ACTIVE;
        xInterfaceUp = pdTRUE;
    }
}

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

/* ---- picodroid WifiManager helpers (hal/wifi.rs via cyw43/link.rs) ---- */

/*
 * The STA's state for the Java WifiManager, folded from the driver's join
 * state and this port's interface-up flag: the driver's failure kinds
 * (CYW43_LINK_BADAUTH -3, NONET -2, FAIL -1), 0 down, 1 associating (a
 * join was issued and no EV_LINK has arrived), 2 associated (link up).
 */
int picodroid_cyw43_sta_status(void) {
    /* Called on the link task, which is where the driver's event handling
     * writes the word too (cyw43_poll and the join's ioctls run there). */
    vCollapseSelfJoin();
    int s = cyw43_wifi_link_status(&cyw43_state, CYW43_ITF_STA);
    if (s < 0) {
        return s;
    }
    if (s < CYW43_LINK_JOIN) {
        return 0;
    }
    return xInterfaceUp != pdFALSE ? 2 : 1;
}

/* cyw43_wifi_scan_active is a static inline over the driver struct, which
 * the Rust side keeps opaque. */
int picodroid_cyw43_scan_active(void) {
    return cyw43_wifi_scan_active(&cyw43_state) ? 1 : 0;
}

/* The driver's raw join-state word (WIFI_JOIN_STATE_* in cyw43_ctrl.c),
 * logged by the join supervisor when it retries: tells a join that never
 * got its SET_SSID verdict (0x0001) from one that authenticated and lost
 * the link (0x0201) or keyed and never linked (0x0a01). */
uint32_t picodroid_cyw43_join_state(void) {
    return cyw43_state.wifi_join_state;
}

/* Log every async event the chip sends (cyw43_dump_async_event). */
void picodroid_cyw43_trace_events(int on) {
    if (on) {
        cyw43_state.trace_flags |= CYW43_TRACE_ASYNC_EV;
    } else {
        cyw43_state.trace_flags &= ~(uint32_t)CYW43_TRACE_ASYNC_EV;
    }
}

/* Start an active scan of every channel; results reach `cb` from inside
 * cyw43_poll, on the link task. The options struct's layout stays here. */
int picodroid_cyw43_scan_start(int (*cb)(void *, const cyw43_ev_scan_result_t *)) {
    cyw43_wifi_scan_options_t opts;
    memset(&opts, 0, sizeof(opts));
    return cyw43_wifi_scan(&cyw43_state, &opts, NULL, cb);
}

/* ---- Link state callbacks ---- */

void cyw43_cb_tcpip_set_link_up(cyw43_t *self, int itf) {
    (void)self;
    if (itf == CYW43_ITF_STA) {
        xInterfaceUp = pdTRUE;
    }
}

/*
 * The driver reports the link gone (EV_LINK down, EV_DISASSOC): a deauth
 * from the AP, the AP rebooting, the station out of range. The stack
 * only learns of a lost link from the driver, so tell it once per loss:
 * the IP task then runs its network-down path (the `net: down` hook,
 * every socket's send failing fast instead of timing out) and retries
 * pfInitialise every 3 s, which brings DHCP back the moment the join
 * supervisor (hal/rp/cyw43/link.rs) has the station re-associated. A
 * link-down during a join (never up) is not a loss to the stack, which
 * never saw it up. Called inside cyw43_poll on the link task, which may
 * post to the IP task's queue.
 */
void cyw43_cb_tcpip_set_link_down(cyw43_t *self, int itf) {
    (void)self;
    if (itf == CYW43_ITF_STA) {
        BaseType_t xWasUp = xInterfaceUp;
        xInterfaceUp = pdFALSE;
        if (xWasUp != pdFALSE && pxRegisteredInterface != NULL) {
            FreeRTOS_NetworkDown(pxRegisteredInterface);
        }
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
