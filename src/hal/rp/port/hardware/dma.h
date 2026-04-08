/*
 * Minimal DMA shim — shadows pico-sdk's hardware/dma.h.
 *
 * Provides the subset of DMA API needed by the CYW43 PIO SPI driver.
 * Actual implementations are in pico_shim_rp2350.c.
 */

#ifndef HARDWARE_DMA_H
#define HARDWARE_DMA_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

/* ---- DMA base address (RP2350) ---- */
#define DMA_BASE  0x50000000u

/* ---- DMA channel config ---- */
typedef struct {
    uint32_t ctrl;
} dma_channel_config;

/* ---- DMA transfer sizes ---- */
enum dma_channel_transfer_size {
    DMA_SIZE_8 = 0,
    DMA_SIZE_16 = 1,
    DMA_SIZE_32 = 2,
};

/* ---- DMA DREQ (data request) sources ---- */
/* PIO TX/RX DREQ numbers for RP2350 */
#define DREQ_PIO0_TX0  0
#define DREQ_PIO0_TX1  1
#define DREQ_PIO0_TX2  2
#define DREQ_PIO0_TX3  3
#define DREQ_PIO0_RX0  4
#define DREQ_PIO0_RX1  5
#define DREQ_PIO0_RX2  6
#define DREQ_PIO0_RX3  7
#define DREQ_PIO1_TX0  8
#define DREQ_PIO1_TX1  9
#define DREQ_PIO1_TX2  10
#define DREQ_PIO1_TX3  11
#define DREQ_PIO1_RX0  12
#define DREQ_PIO1_RX1  13
#define DREQ_PIO1_RX2  14
#define DREQ_PIO1_RX3  15

/* ---- DMA API functions (implemented in pico_shim_rp2350.c) ---- */

/* Channel allocation */
int dma_claim_unused_channel(bool required);
void dma_channel_claim(uint channel);
void dma_channel_unclaim(uint channel);

/* Channel configuration */
dma_channel_config dma_channel_get_default_config(uint channel);
void channel_config_set_transfer_data_size(dma_channel_config *c, enum dma_channel_transfer_size size);
void channel_config_set_read_increment(dma_channel_config *c, bool incr);
void channel_config_set_write_increment(dma_channel_config *c, bool incr);
void channel_config_set_dreq(dma_channel_config *c, uint dreq);
void channel_config_set_bswap(dma_channel_config *c, bool bswap);

/* Transfer operations */
void dma_channel_configure(uint channel, const dma_channel_config *config,
                           volatile void *write_addr, const volatile void *read_addr,
                           uint transfer_count, bool trigger);
void dma_channel_transfer_from_buffer_now(uint channel, const volatile void *read_addr, uint32_t transfer_count);
void dma_channel_transfer_to_buffer_now(uint channel, volatile void *write_addr, uint32_t transfer_count);
void dma_channel_set_read_addr(uint channel, const volatile void *read_addr, bool trigger);
void dma_channel_set_write_addr(uint channel, volatile void *write_addr, bool trigger);
void dma_channel_set_trans_count(uint channel, uint32_t trans_count, bool trigger);
void dma_channel_start(uint channel);

/* Synchronization */
void dma_channel_wait_for_finish_blocking(uint channel);
bool dma_channel_is_busy(uint channel);
void dma_channel_abort(uint channel);

/* IRQ */
void dma_channel_set_irq0_enabled(uint channel, bool enabled);
void dma_irqn_acknowledge_channel(uint irq_index, uint channel);

/* Hardware address helpers */
static inline volatile void *dma_channel_hw_addr(uint channel) {
    return (volatile void *)(DMA_BASE + channel * 0x40);
}

#endif /* HARDWARE_DMA_H */
