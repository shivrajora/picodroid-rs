// SPDX-License-Identifier: GPL-3.0-only
//! Bit-faithful Rust port of FreeRTOS `heap_4.c` with 32-bit block
//! arithmetic, backing the simulator's global allocator
//! (docs/parity-audit.md M1).
//!
//! The device allocator is heap_4: first-fit over an address-ordered free
//! list with two-sided coalescing, an 8-byte `BlockLink_t` header per block,
//! 8-byte alignment, and a 16-byte minimum split remainder. The simulator
//! must reproduce that *block-level* behavior — most importantly
//! fragmentation ("enough free bytes, no contiguous block"), which a byte
//! counter cannot model and which is the recorded cause of the 2026
//! sim-passes/device-OOMs incidents.
//!
//! Why a port and not the vendored C: `BlockLink_t` is `{pointer, size_t}`.
//! Compiled for a 64-bit host the header doubles to 16 bytes, the minimum
//! block to 32, and the allocated-flag moves to bit 63 — same code, wrong
//! arithmetic. This port stores headers *inside the arena* as
//! `{next_off: u32, size_and_flag: u32}`, exactly the device's in-memory
//! layout, so every size, offset, split and merge decision matches the
//! 32-bit original. Offsets are relative to the arena base; `u32::MAX`
//! plays NULL.
//!
//! Fidelity is enforced by two test layers below: semantics tests
//! transcribed from heap_4.c, and an exact replay of the V6 hardware oracle
//! trace captured from a real RP2350 (docs/parity-audit.md Appendix A) —
//! every logged `free/min_ever` pair must match bit-for-bit.

/// `xHeapStructSize` on the 32-bit device: `sizeof(BlockLink_t)` (8) already
/// 8-aligned.
pub const HEAP_STRUCT_SIZE: u32 = 8;
/// `heapMINIMUM_BLOCK_SIZE = xHeapStructSize << 1`.
pub const MIN_BLOCK_SIZE: u32 = HEAP_STRUCT_SIZE << 1;
/// `heapBLOCK_ALLOCATED_BITMASK`: top bit of a 32-bit `size_t`.
const ALLOC_BIT: u32 = 1 << 31;
/// Stand-in for a NULL `pxNextFreeBlock`.
const NONE: u32 = u32::MAX;
/// Virtual offset for `&xStart` (a static outside the arena in the C
/// original). Never dereferenced; only an iterator position marker.
const START: u32 = u32::MAX - 1;

/// Mirror of `vPortGetHeapStats` plus the success counters heap_4 keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeapStats {
    pub free_bytes: u32,
    pub min_ever_free_bytes: u32,
    pub largest_free_block: u32,
    pub free_blocks: u32,
    pub successful_allocs: u32,
    pub successful_frees: u32,
}

pub struct Heap4 {
    base: *mut u8,
    /// Usable arena size in bytes (already 8-aligned by the caller).
    size: u32,
    /// `xStart.pxNextFreeBlock` — offset of the first free block.
    start_next: u32,
    /// Offset of the `pxEnd` marker block at the arena top.
    end_off: u32,
    free_bytes: u32,
    min_ever_free: u32,
    allocs: u32,
    frees: u32,
}

// The arena is exclusively owned by the Heap4 instance (itself behind the
// allocator's process-wide lock, the stand-in for vTaskSuspendAll).
unsafe impl Send for Heap4 {}

impl Heap4 {
    /// Port of `prvHeapInit`. `base` must be 8-aligned, valid for `size`
    /// bytes, and exclusively owned by the returned instance.
    ///
    /// # Safety
    /// Caller guarantees the base/size contract above for the lifetime of
    /// the instance.
    pub unsafe fn init(base: *mut u8, size: u32) -> Self {
        assert_eq!(base as usize & 7, 0, "arena base must be 8-aligned");
        assert!(size >= 64, "arena too small to host pxEnd + a free block");
        // pxEnd sits at the last 8-aligned slot that leaves room for its
        // header: (base + size - 8) aligned down to 8. With an aligned base
        // this is offset arithmetic only.
        let end_off = (size - HEAP_STRUCT_SIZE) & !7;
        let mut h = Heap4 {
            base,
            size,
            start_next: 0,
            end_off,
            free_bytes: end_off,
            min_ever_free: end_off,
            allocs: 0,
            frees: 0,
        };
        // pxEnd: size 0, next NULL.
        h.set_bsize(end_off, 0);
        h.set_next(end_off, NONE);
        // Single free block covering the whole usable span.
        h.set_bsize(0, end_off);
        h.set_next(0, end_off);
        h
    }

    #[inline]
    fn word(&self, off: u32, idx: u32) -> *mut u32 {
        // In-arena header: [next_off, size_and_flag] — the device's
        // BlockLink_t field order. No debug_assert here: this runs inside
        // the global allocator, where a panic path (which allocates) can
        // deadlock; corruption is caught by `free`'s Result checks instead.
        unsafe { (self.base.add(off as usize) as *mut u32).add(idx as usize) }
    }
    #[inline]
    fn next(&self, off: u32) -> u32 {
        unsafe { *self.word(off, 0) }
    }
    #[inline]
    fn set_next(&mut self, off: u32, v: u32) {
        unsafe { *self.word(off, 0) = v }
    }
    #[inline]
    fn bsize(&self, off: u32) -> u32 {
        unsafe { *self.word(off, 1) }
    }
    #[inline]
    fn set_bsize(&mut self, off: u32, v: u32) {
        unsafe { *self.word(off, 1) = v }
    }
    /// `pxIterator->pxNextFreeBlock` where `START` models `&xStart`.
    #[inline]
    fn next_of(&self, iter: u32) -> u32 {
        if iter == START {
            self.start_next
        } else {
            self.next(iter)
        }
    }
    #[inline]
    fn set_next_of(&mut self, iter: u32, v: u32) {
        if iter == START {
            self.start_next = v;
        } else {
            self.set_next(iter, v);
        }
    }

    /// Port of `pvPortMalloc`. Takes the raw wanted size in bytes; returns
    /// the payload offset (header + 8) or `None`, exactly where the device
    /// would return NULL — including for a zero-size request.
    pub fn malloc(&mut self, wanted: u32) -> Option<u32> {
        // Size adjustment: +8 header, round up to 8. Overflow → the C sets
        // xWantedSize = 0, which then fails the `> 0` gate below.
        let mut want: u32 = 0;
        if wanted > 0 {
            if let Some(w) = wanted.checked_add(HEAP_STRUCT_SIZE) {
                want = w;
                if want & 7 != 0 {
                    match want.checked_add(8 - (want & 7)) {
                        Some(w) => want = w,
                        None => want = 0,
                    }
                }
            }
        }
        // heapBLOCK_SIZE_IS_VALID: the top bit must be free.
        if want & ALLOC_BIT != 0 {
            return None;
        }
        if want == 0 || want > self.free_bytes {
            return None;
        }
        // First-fit traversal from the lowest address.
        let mut prev = START;
        let mut blk = self.start_next;
        while self.bsize(blk) < want && self.next(blk) != NONE {
            prev = blk;
            blk = self.next(blk);
        }
        if blk == self.end_off {
            return None;
        }
        let ret = blk + HEAP_STRUCT_SIZE;
        // Unlink.
        let blk_next = self.next(blk);
        self.set_next_of(prev, blk_next);
        // Split iff the remainder is STRICTLY greater than the minimum; the
        // remainder is linked directly after `prev` (heap_4 does not go
        // through prvInsertBlockIntoFreeList here — address order is
        // preserved by construction).
        let bs = self.bsize(blk);
        if bs - want > MIN_BLOCK_SIZE {
            let newb = blk + want;
            self.set_bsize(newb, bs - want);
            self.set_bsize(blk, want);
            let prev_next = self.next_of(prev);
            self.set_next(newb, prev_next);
            self.set_next_of(prev, newb);
        }
        let final_size = self.bsize(blk);
        self.free_bytes -= final_size;
        if self.free_bytes < self.min_ever_free {
            self.min_ever_free = self.free_bytes;
        }
        self.set_bsize(blk, final_size | ALLOC_BIT);
        self.set_next(blk, NONE);
        self.allocs += 1;
        Some(ret)
    }

    /// Port of `vPortFree`. Takes the payload offset returned by
    /// [`malloc`](Self::malloc).
    ///
    /// The C asserts the block is marked allocated with a NULL next; a
    /// violation here is heap corruption (double free, foreign pointer).
    /// Returns `Err` instead of panicking: this runs inside the global
    /// allocator holding its lock, where a panic (which itself allocates)
    /// would deadlock — the caller reports and aborts via a no-alloc path.
    pub fn free(&mut self, payload_off: u32) -> Result<(), &'static str> {
        if payload_off < HEAP_STRUCT_SIZE || payload_off >= self.size {
            return Err("sim heap_4: free of an offset outside the arena");
        }
        let blk = payload_off - HEAP_STRUCT_SIZE;
        let bs = self.bsize(blk);
        if bs & ALLOC_BIT == 0 {
            return Err("sim heap_4: freeing a block not marked allocated (double free?)");
        }
        if self.next(blk) != NONE {
            return Err("sim heap_4: freed block has a non-NULL next (header corruption)");
        }
        let sz = bs & !ALLOC_BIT;
        self.set_bsize(blk, sz);
        self.free_bytes += sz;
        self.insert_free(blk);
        self.frees += 1;
        Ok(())
    }

    /// Port of `prvInsertBlockIntoFreeList`: address-ordered insert with
    /// two-sided coalescing; never merges across `pxEnd`.
    fn insert_free(&mut self, off: u32) {
        // Find the block after which to insert (iterate while
        // iterator->next < block; NONE/u32::MAX naturally terminates).
        let mut iter = START;
        while self.next_of(iter) < off {
            iter = self.next_of(iter);
        }
        // Merge with the iterator block if contiguous. The C performs this
        // check against &xStart too, but xStart lives outside the arena with
        // size 0, so it can never match — mirrored here by skipping START.
        let mut ins = off;
        if iter != START && iter + self.bsize(iter) == off {
            let merged = self.bsize(iter) + self.bsize(off);
            self.set_bsize(iter, merged);
            ins = iter;
        }
        // Merge with the following block if contiguous (but never pxEnd).
        let iter_next = self.next_of(iter);
        if ins + self.bsize(ins) == iter_next {
            if iter_next != self.end_off {
                let merged = self.bsize(ins) + self.bsize(iter_next);
                self.set_bsize(ins, merged);
                let after = self.next(iter_next);
                self.set_next(ins, after);
            } else {
                self.set_next(ins, self.end_off);
            }
        } else {
            self.set_next(ins, iter_next);
        }
        if iter != ins {
            self.set_next_of(iter, ins);
        }
    }

    pub fn free_bytes(&self) -> u32 {
        self.free_bytes
    }
    pub fn min_ever_free_bytes(&self) -> u32 {
        self.min_ever_free
    }
    pub fn arena_size(&self) -> u32 {
        self.size
    }

    /// Walk the free list for the `vPortGetHeapStats` mirror.
    pub fn stats(&self) -> HeapStats {
        let mut largest = 0u32;
        let mut count = 0u32;
        let mut blk = self.start_next;
        while blk != self.end_off && blk != NONE {
            let s = self.bsize(blk);
            if s > largest {
                largest = s;
            }
            count += 1;
            blk = self.next(blk);
        }
        HeapStats {
            free_bytes: self.free_bytes,
            min_ever_free_bytes: self.min_ever_free,
            largest_free_block: largest,
            free_blocks: count,
            successful_allocs: self.allocs,
            successful_frees: self.frees,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8-aligned test arena. Leaked on purpose — tests only.
    fn arena(size: u32) -> Heap4 {
        let layout = std::alloc::Layout::from_size_align(size as usize, 8).unwrap();
        let base = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!base.is_null());
        unsafe { Heap4::init(base, size) }
    }

    /// Device cost of a raw request: +8 header, 8-aligned (the min-block
    /// floor applies to split remainders, not to the allocation itself).
    fn cost(raw: u32) -> u32 {
        (raw + HEAP_STRUCT_SIZE + 7) & !7
    }

    #[test]
    fn init_matches_heap4_layout() {
        let h = arena(425_984); // RP2350: 416 KB
                                // pxEnd at (size - 8) & !7; single free block spans up to it.
        assert_eq!(h.free_bytes(), 425_976);
        assert_eq!(h.min_ever_free_bytes(), 425_976);
        let s = h.stats();
        assert_eq!(s.largest_free_block, 425_976);
        assert_eq!(s.free_blocks, 1);
    }

    #[test]
    fn zero_size_returns_null_like_device() {
        let mut h = arena(4096);
        assert_eq!(h.malloc(0), None);
    }

    #[test]
    fn alloc_charges_header_and_alignment() {
        let mut h = arena(4096);
        let before = h.free_bytes();
        let p = h.malloc(1).unwrap();
        assert_eq!(p % 8, 0, "payload must be 8-aligned");
        assert_eq!(before - h.free_bytes(), cost(1)); // 16
        let before = h.free_bytes();
        h.malloc(24).unwrap();
        assert_eq!(before - h.free_bytes(), cost(24)); // 32
    }

    #[test]
    fn split_only_when_remainder_strictly_exceeds_min_block() {
        // Free a block, then re-request slightly less: if the remainder
        // would be <= 16 the WHOLE block is handed out (no split) — the
        // exact heap_4 rule, and a real source of device-only slack.
        let mut h = arena(4096);
        let a = h.malloc(100).unwrap(); // block size 112
        let _guard = h.malloc(100).unwrap(); // pin the region below
        h.free(a).unwrap();
        let before = h.free_bytes();
        // 112 - cost(88)=96 = 16 → NOT > 16 → no split, full 112 charged.
        let b = h.malloc(88).unwrap();
        assert_eq!(a, b, "first-fit must reuse the freed block");
        assert_eq!(
            before - h.free_bytes(),
            112,
            "no-split hands out the whole block"
        );
        h.free(b).unwrap();
        // 112 - cost(80)=88 = 24 → > 16 → split, exactly 88 charged.
        let before = h.free_bytes();
        let c = h.malloc(80).unwrap();
        assert_eq!(before - h.free_bytes(), 88);
        assert_eq!(c, a);
    }

    #[test]
    fn coalescing_both_sides() {
        let mut h = arena(4096);
        let a = h.malloc(56).unwrap();
        let b = h.malloc(56).unwrap();
        let c = h.malloc(56).unwrap();
        let _pin = h.malloc(56).unwrap();
        // Free a and c: two islands.
        h.free(a).unwrap();
        h.free(c).unwrap();
        assert_eq!(h.stats().free_blocks, 3); // a, c, tail
                                              // Free b: must merge a+b+c into one 192-byte block.
        h.free(b).unwrap();
        let s = h.stats();
        assert_eq!(s.free_blocks, 2); // merged island + tail
        let island = h.bsize(a - HEAP_STRUCT_SIZE); // block at a's header
        assert_eq!(island, 3 * 64);
    }

    #[test]
    fn fragmentation_oom_despite_free_bytes() {
        // The thesis case: total free far exceeds the request, but no
        // contiguous block fits → NULL. A byte counter cannot fail here.
        let mut h = arena(2048);
        // Carve the arena into alternating 56-byte allocations.
        let blocks: Vec<u32> = (0..).map_while(|_| h.malloc(56)).collect();
        assert!(h.malloc(56).is_none());
        // Pin the sub-64 B tail remainder so freed holes can't merge into it.
        let _tail_pin = h.malloc(48).unwrap();
        // Free every second block → ~half the arena free, all 64-byte holes.
        let mut freed = 0u32;
        for (i, &b) in blocks.iter().enumerate() {
            if i % 2 == 0 {
                h.free(b).unwrap();
                freed += 64;
            }
        }
        assert!(h.free_bytes() >= freed);
        // Plenty of free bytes, but no 200-byte contiguous block exists.
        assert!(h.free_bytes() > 256);
        assert_eq!(
            h.malloc(200),
            None,
            "fragmented heap must OOM on contiguity"
        );
        assert_eq!(h.stats().largest_free_block, 64);
    }

    #[test]
    fn min_ever_free_is_a_high_water_mark() {
        let mut h = arena(4096);
        let a = h.malloc(1000).unwrap();
        let low = h.free_bytes();
        h.free(a).unwrap();
        assert!(h.free_bytes() > low);
        assert_eq!(h.min_ever_free_bytes(), low);
    }

    /// Exact replay of the V6 hardware oracle (docs/parity-audit.md
    /// Appendix A): a deterministic 400-op LCG alloc/free trace captured on
    /// a real RP2350 over RTT, with `xPortGetFreeHeapSize` /
    /// `xPortGetMinimumEverFreeHeapSize` logged every 40 ops. The port must
    /// reproduce every pair bit-for-bit, and return to the exact starting
    /// free figure after the balanced teardown.
    #[test]
    fn replays_rp2350_hardware_oracle_trace() {
        const ARENA: u32 = 425_984; // configTOTAL_HEAP_SIZE, RP2350
        const TRACE_START_FREE: u32 = 413_384; // measured pre-trace on HW
        const EXPECTED: [(u32, u32); 10] = [
            (402_392, 400_344),
            (395_216, 391_288),
            (392_584, 391_288),
            (391_840, 390_784),
            (383_880, 383_880),
            (386_704, 381_416),
            (392_104, 381_416),
            (393_912, 381_416),
            (388_472, 381_416),
            (391_960, 381_416),
        ];

        let mut h = arena(ARENA);
        // The device had 12,592 B of never-freed boot allocations at the
        // arena base before the trace ran. Their internal layout is
        // irrelevant to the trace (nothing below the trace's blocks is
        // freed), so model them as one block whose heap_4 cost matches.
        let consumed = h.free_bytes() - TRACE_START_FREE;
        let pad = h.malloc(consumed - HEAP_STRUCT_SIZE).unwrap();
        assert_eq!(h.free_bytes(), TRACE_START_FREE);
        // Trace min_ever starts clamped at the trace start, as on HW where
        // boot had already set the low-water mark to the pre-trace level.
        assert!(h.min_ever_free_bytes() == TRACE_START_FREE);

        let mut slots = [None::<u32>; 64];
        let mut lcg: u32 = 0x1234_5678;
        let mut next = move || {
            lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            lcg
        };
        let mut checkpoints = Vec::new();
        for op in 0u32..400 {
            let idx = (next() >> 8) as usize % 64;
            match slots[idx] {
                None => {
                    let sz = 16 + (next() % 512) * ((next() % 4) + 1);
                    let p = h.malloc(sz).expect("oracle trace never OOMs on device");
                    slots[idx] = Some(p);
                }
                Some(p) => {
                    h.free(p).unwrap();
                    slots[idx] = None;
                }
            }
            if op % 40 == 39 {
                checkpoints.push((h.free_bytes(), h.min_ever_free_bytes()));
            }
        }
        assert_eq!(
            checkpoints.as_slice(),
            &EXPECTED,
            "free/min_ever curve must match HW"
        );
        for s in slots.iter_mut() {
            if let Some(p) = s.take() {
                h.free(p).unwrap();
            }
        }
        assert_eq!(h.free_bytes(), TRACE_START_FREE, "balanced teardown");
        assert_eq!(h.min_ever_free_bytes(), 381_416, "global low-water mark");
        h.free(pad).unwrap();
    }
}
