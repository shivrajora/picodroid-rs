// SPDX-License-Identifier: GPL-3.0-only
//! What the network stack costs the heap arena, as the simulator models it
//! (docs/parity-audit.md M9).
//!
//! On a network board every FreeRTOS+TCP structure comes out of the same
//! `heap_4` arena the JVM lives in (`configSUPPORT_DYNAMIC_ALLOCATION 1`):
//! the IP task's stack and TCB and its event queue when the link comes up,
//! then per TCP socket the socket struct, its event group and — on first
//! use — its receive and send streams, plus one window-segment pool for
//! the whole stack the first time a window is created. The simulator's
//! sockets are host sockets, so until M9 none of that reached the modelled
//! arena and the board's `nused` sat ~10 KB above the simulator's on the
//! same screen.
//!
//! The figures are `sizeof` on the Cortex-M build of the vendored stack with
//! this repo's `FreeRTOSIPConfig.h` (IPv4 only, `ipconfigUSE_TCP_WIN 1`, no
//! select, no callbacks) and `FreeRTOSConfig.h` (32-bit ticks, trace
//! facility on, mini list items). The device build pins them with
//! `_Static_assert`s in `net-freertos-tcp/net_init.c`, so a stack or config
//! change that moves a struct fails the firmware build rather than drifting
//! the model. The kernel's `Queue_t` and `EventGroup_t` are private to
//! their `.c` files and stay calibrated estimates, absorbed by the
//! calibration test's tolerance.
//!
//! The per-board numbers (`net_tcp_rx_bytes`, `net_buffer_descriptors`, …)
//! come from `board_cfg::net`, generated from the same board.toml keys the
//! C build takes its `-D` overrides from, so the model and the stack are
//! sized from one source.

use crate::board_cfg::net;

/// `ipconfigIP_TASK_STACK_SIZE_WORDS` (512) in bytes.
pub const IP_TASK_STACK_BYTES: u32 = 512 * 4;
/// `sizeof(IPStackEvent_t)`: an enum and a `void *`.
pub const IP_EVENT_BYTES: u32 = 8;
/// `sizeof(Queue_t)` under the RP `FreeRTOSConfig.h` (trace facility on,
/// no queue sets): two pointers, the union, two lists, three counts, two
/// bytes, the trace number and type — 77 B padded to 80. An estimate: the
/// struct is private to `queue.c`.
pub const KERNEL_QUEUE_BYTES: u32 = 80;
/// `sizeof(EventGroup_t)`: the bits, a list and the trace number. An
/// estimate: private to `event_groups.c`. One per socket.
pub const EVENT_GROUP_BYTES: u32 = 28;
/// `sizeof(FreeRTOS_Socket_t)` for a TCP socket: the common head (event
/// bits, event-group pointer, options, bound-list item, two timeouts, the
/// 16 B local address union, port, end-point and id pointers — 68 B) plus
/// the TCP union member (`IPTCPSocket_t`: addresses, sequence state, the
/// last-packet copy, the 192 B `TCPWindow_t` — 376 B). Read off
/// arm-none-eabi-gcc, not summed by hand.
pub const TCP_SOCKET_STRUCT_BYTES: u32 = 444;
/// `sizeof(StreamBuffer_t)` before its flexible array: five counters and
/// the one-byte array rounded to a word.
pub const STREAM_HEADER_BYTES: u32 = 24;
/// `sizeof(TCPSegment_t)`: four sequence words, a timer, the union and two
/// list items.
pub const TCP_SEGMENT_BYTES: u32 = 64;

/// Bytes `prvTCPCreateStream` allocates for a stream of `len` payload
/// bytes: `sizeof(StreamBuffer_t) + ((len + sizeof(size_t)) & ~3) -
/// sizeof(ucArray)` (`FreeRTOS_Sockets.c`).
pub const fn stream_bytes(len: u32) -> u32 {
    STREAM_HEADER_BYTES + ((len + 4) & !3) - 4
}

/// The kernel objects `FreeRTOS_IPInit_Multi` and the buffer allocator
/// create for `descriptors` network buffers: the IP event queue
/// (`ipconfigEVENT_QUEUE_LENGTH = descriptors + 5` events) and the
/// buffer-counting semaphore (a `Queue_t` with no storage).
pub const fn ip_task_queue_bytes(descriptors: u32) -> u32 {
    KERNEL_QUEUE_BYTES + (descriptors + 5) * IP_EVENT_BYTES + KERNEL_QUEUE_BYTES
}

/// What bringing the stack up costs at boot: the IP task's stack and TCB
/// (`tcb_bytes` is the family's estimate) and its queues.
pub const fn ip_task_boot_bytes(tcb_bytes: u32) -> u32 {
    IP_TASK_STACK_BYTES + tcb_bytes + ip_task_queue_bytes(net::BUFFER_DESCRIPTORS)
}

/// The blocks a connected TCP socket holds, each as the device allocates
/// it: the socket struct, its event group, the receive stream and the send
/// stream. The streams are created lazily on the device (the first receive
/// and the first send); a socket Java opens reaches both, so the model
/// charges them at connect.
pub const fn socket_blocks() -> [u32; 4] {
    [
        TCP_SOCKET_STRUCT_BYTES,
        EVENT_GROUP_BYTES,
        stream_bytes(net::TCP_RX_BYTES),
        stream_bytes(net::TCP_TX_BYTES),
    ]
}

/// The sum of [`socket_blocks`].
pub const fn socket_bytes() -> u32 {
    let b = socket_blocks();
    b[0] + b[1] + b[2] + b[3]
}

/// The TCP window-segment pool: `ipconfigTCP_WIN_SEG_COUNT` segments,
/// allocated once when the first window is created and never freed
/// (`FreeRTOS_TCP_WIN.c`), so it is charged with the first connection and
/// stays.
pub const fn win_seg_pool_bytes() -> u32 {
    net::WIN_SEGS * TCP_SEGMENT_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arithmetic behind the model, with the pico_display2_w keys
    /// (8 descriptors, 2048 B streams, 8 segments) spelled as literals so
    /// the test does not depend on which board the test build resolves.
    #[test]
    fn the_model_arithmetic_for_a_2048_byte_stream_board() {
        assert_eq!(stream_bytes(2048), 2072);
        assert_eq!(stream_bytes(4096), 4120);
        assert_eq!(ip_task_queue_bytes(8), 80 + 13 * 8 + 80);
        assert_eq!(IP_TASK_STACK_BYTES + 120 + ip_task_queue_bytes(8), 2432);
        assert_eq!(
            TCP_SOCKET_STRUCT_BYTES + EVENT_GROUP_BYTES + 2 * stream_bytes(2048),
            4616
        );
        assert_eq!(8 * TCP_SEGMENT_BYTES, 512);
    }

    /// The board this build resolved to: every block is word-aligned and
    /// the sum is what the sim charges.
    #[test]
    fn the_resolved_board_blocks_sum_and_align() {
        let blocks = socket_blocks();
        assert!(blocks.iter().all(|b| b % 4 == 0));
        assert_eq!(blocks.iter().sum::<u32>(), socket_bytes());
        assert_eq!(win_seg_pool_bytes() % TCP_SEGMENT_BYTES, 0);
    }

    /// The device's own figures, to pin the model against once measured.
    ///
    /// Recipe (docs/parity-audit.md M9): `netdemo` on pico_display2_w with
    /// a mem-diag firmware (`PICODROID_EXTRA_FEATURES=mem-diag
    /// PICODROID_NET_TEST_HOST=<lan ip> ./scripts/flash.sh -b
    /// pico_display2_w -a netdemo`), a scratch copy of the app sleeping
    /// 3 s between its phases, and the `memmon: … nused=` line over RTT at
    /// four points: after `memdiag: ACTIVE` before link init; after the
    /// network-up event; after `Received 5 bytes`; after `Done`. The three
    /// deltas are what the constants below hold.
    #[test]
    #[ignore = "calibration pending: pico_display2_w not yet measured — docs/parity-audit.md M9"]
    fn the_model_is_within_2_kb_of_the_measured_board() {
        /// `nused` after the network-up event minus before link init.
        const DEVICE_IP_INIT_DELTA_B: u32 = 0;
        /// `nused` after the first exchange minus after network-up.
        const DEVICE_FIRST_CONNECT_DELTA_B: u32 = 0;
        /// `nused` after close minus after the first exchange (negative:
        /// the streams, event group and socket struct go back; the
        /// window pool stays).
        const DEVICE_CLOSE_DELTA_B: i32 = 0;
        let within = |model: u32, measured: u32| model.abs_diff(measured) <= 2048;
        assert!(within(ip_task_boot_bytes(120), DEVICE_IP_INIT_DELTA_B));
        assert!(within(
            socket_bytes() + win_seg_pool_bytes(),
            DEVICE_FIRST_CONNECT_DELTA_B
        ));
        assert!(within(socket_bytes(), DEVICE_CLOSE_DELTA_B.unsigned_abs()));
    }
}
