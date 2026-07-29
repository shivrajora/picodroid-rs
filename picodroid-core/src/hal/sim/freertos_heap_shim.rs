// SPDX-License-Identifier: GPL-3.0-only
//! `pvPortMalloc` / `vPortFree` for the hosted FreeRTOS kernel.
//!
//! The simulator's kernel is compiled without a `heap_N.c` (see
//! `build_support/freertos_host.rs`); the kernel's allocator *is* this file.
//! Every call routes to the host allocator inside an [`allocator::bypass`]
//! region, so kernel objects are uncounted and uncapped — the same treatment
//! host `std::thread` internals already get.
//!
//! # Why the kernel's own bytes must not reach the arena
//!
//! On a 64-bit host every FreeRTOS object is host-sized: `StackType_t` is
//! `unsigned long` (8 B against the device's 4), TCBs carry 64-bit pointers,
//! and a real `heap_4.c` would double its own block headers. Charging those
//! to the modeled arena would silently inflate every figure the parity
//! harness compares against hardware — the ±2 KB HIL boot-budget assertion
//! first (docs/parity-audit.md MEM-04/M4).
//!
//! Device bytes for kernel objects still enter the arena, but through the
//! *model*: `boot_budget.rs` charges each task the stack words and TCB
//! estimate the device would have allocated, at the moment the task is really
//! created. So the arena sees device numbers while the host sees host ones,
//! and neither is a guess about the other
//! (docs/designs/freertos-host-sim.md §1.1).
//!
//! [`xPortGetFreeHeapSize`] deliberately reports the *modeled* arena's
//! free bytes rather than this shim's: anything kernel-side that asks how much
//! heap is left should hear the device's answer, not the host's ~unbounded
//! one.

#![allow(non_snake_case)] // C ABI symbol names: pvPortMalloc, not pv_port_malloc

use std::alloc::Layout;
use std::ffi::c_void;

use super::allocator;

/// Bytes reserved before each block for the layout `vPortFree` must
/// reconstruct. 16 rather than 8 so the returned pointer keeps 16-byte
/// alignment — `portBYTE_ALIGNMENT` is 8, but nothing in the kernel is harmed
/// by more, and a `HashMap` side table (the alternative) would itself have to
/// allocate under bypass on every kernel malloc.
const HEADER: usize = 16;

fn layout_for(total: usize) -> Layout {
    // align 16: see HEADER.
    Layout::from_size_align(total, 16).expect("kernel allocation layout")
}

/// # Safety
///
/// C ABI entry point. The returned pointer is owned by the kernel until it
/// passes it back to [`vPortFree`].
#[no_mangle]
pub unsafe extern "C" fn pvPortMalloc(size: usize) -> *mut c_void {
    if size == 0 {
        return std::ptr::null_mut();
    }
    let Some(total) = size.checked_add(HEADER) else {
        return std::ptr::null_mut();
    };
    let _bypass = allocator::bypass();
    let p = std::alloc::alloc(layout_for(total));
    if p.is_null() {
        return std::ptr::null_mut();
    }
    p.cast::<usize>().write(total);
    p.add(HEADER).cast()
}

/// # Safety
///
/// `pv` must be null or a pointer previously returned by [`pvPortMalloc`].
#[no_mangle]
pub unsafe extern "C" fn vPortFree(pv: *mut c_void) {
    if pv.is_null() {
        return;
    }
    let base = pv.cast::<u8>().sub(HEADER);
    let total = base.cast::<usize>().read();
    let _bypass = allocator::bypass();
    std::alloc::dealloc(base, layout_for(total));
}

/// Free bytes in the *modeled* device arena — see the module docs.
#[no_mangle]
pub extern "C" fn xPortGetFreeHeapSize() -> usize {
    match allocator::heap4_stats() {
        Some(h) => h.free_bytes as usize,
        None => {
            let (cur, _peak, limit) = allocator::heap_stats();
            limit.saturating_sub(cur)
        }
    }
}

/// Low-water mark of the modeled arena, for the same reason as
/// [`xPortGetFreeHeapSize`].
#[no_mangle]
pub extern "C" fn xPortGetMinimumEverFreeHeapSize() -> usize {
    match allocator::heap4_stats() {
        Some(h) => h.min_ever_free_bytes as usize,
        None => xPortGetFreeHeapSize(),
    }
}

/// Part of `portable.h`'s contract and a no-op for a host-backed heap: there
/// are no blocks of ours to re-initialise.
#[no_mangle]
pub extern "C" fn vPortInitialiseBlocks() {}
