// SPDX-License-Identifier: GPL-3.0-only
//! Allocation-site ledger for the simulator arena (`mem-diag`, opt-in).
//!
//! The `[memmon]` line says how much of the arena is used; the heap census
//! says how much of that is Java objects, arrays, strings and parsed class
//! metadata. Everything else — task stacks, executor state, resolution
//! tables, the file system, native side tables — was invisible: `nused`
//! minus the census was a number with no owner. This ledger gives every
//! live arena block an owner: the call stack that allocated it.
//!
//! Enabled by `PICODROID_MEMDIAG_SITES=1`. Every arena allocation of at least
//! `PICODROID_MEMDIAG_SITES_MIN` bytes (default 0: all of them) records its
//! size and raw call stack, keyed by address, in a host-side map that the
//! arena never sees (the ledger runs under the allocator's bypass). A free
//! removes the entry. `heapcensus` then prints the live set aggregated three
//! ways: by size class (the heap_4 header-and-alignment tax of many small
//! blocks), by the innermost frame inside this code base (the site), and by
//! full call stack (the path that reached it) — the first
//! `PICODROID_MEMDIAG_SITES_TOP` rows of each (default 24).
//!
//! Symbolisation happens only at census time, and only for the stacks that
//! make the cut, so recording costs one unwind per allocation (a few
//! microseconds) and nothing else. Frames from `alloc`, `core`, `std`,
//! hashbrown and the allocator itself are skipped when a stack is printed;
//! everything from `picodroid_core`, `pico_jvm`, the platform crate and the
//! LVGL bindings is a candidate.
//!
//! Host-only by construction: the device has no unwinder and no symbols,
//! and the ledger would cost more RAM than the heap it describes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

use super::allocator;

/// Deepest call stack kept per block. Java-call-heavy paths run deep; the
/// interesting frames sit near the leaf.
const MAX_FRAMES: usize = 40;

struct Entry {
    size: u32,
    frames: Box<[usize]>,
}

/// Live arena blocks by address. `None` until the first record.
static LEDGER: Mutex<Option<HashMap<usize, Entry>>> = Mutex::new(None);

/// 0 = env unread, 1 = off, 2 = on (the `histo_enabled` pattern).
static ENABLED: AtomicU8 = AtomicU8::new(0);
/// Minimum block size traced; read with `ENABLED`.
static MIN_BYTES: AtomicU32 = AtomicU32::new(0);
/// Blocks below the minimum, so the census can say how much it did not see.
static SKIPPED_BYTES: AtomicU32 = AtomicU32::new(0);
static SKIPPED_COUNT: AtomicU32 = AtomicU32::new(0);

/// Read an environment variable without allocating: this runs inside the
/// global allocator (the `heap_limit` discipline).
fn getenv_u32(name: &[u8]) -> Option<u32> {
    // SAFETY: `name` is NUL-terminated by every caller.
    let p = unsafe { libc::getenv(name.as_ptr() as *const libc::c_char) };
    if p.is_null() {
        return None;
    }
    let mut v: u32 = 0;
    let mut i = 0;
    loop {
        // SAFETY: getenv returns a NUL-terminated string.
        let c = unsafe { *p.add(i) } as u8;
        if c == 0 {
            break;
        }
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.saturating_mul(10).saturating_add((c - b'0') as u32);
        i += 1;
    }
    Some(v)
}

pub fn enabled() -> bool {
    match ENABLED.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = getenv_u32(b"PICODROID_MEMDIAG_SITES\0").unwrap_or(0) != 0;
            if on {
                MIN_BYTES.store(
                    getenv_u32(b"PICODROID_MEMDIAG_SITES_MIN\0").unwrap_or(0),
                    Ordering::Relaxed,
                );
            }
            ENABLED.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

fn lock() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Entry>>> {
    LEDGER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Record a block the arena just handed out. Called by the allocator with
/// the arena lock released; the ledger's own memory is host memory.
pub fn record(ptr: *mut u8, size: usize) {
    if !enabled() {
        return;
    }
    let size = size as u32;
    if size < MIN_BYTES.load(Ordering::Relaxed) {
        SKIPPED_BYTES.fetch_add(size, Ordering::Relaxed);
        SKIPPED_COUNT.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let _b = allocator::bypass();
    let mut ips = [0usize; MAX_FRAMES];
    let mut n = 0;
    backtrace::trace(|f| {
        ips[n] = f.ip() as usize;
        n += 1;
        n < MAX_FRAMES
    });
    let mut g = lock();
    g.get_or_insert_with(HashMap::new).insert(
        ptr as usize,
        Entry {
            size,
            frames: ips[..n].into(),
        },
    );
}

/// Forget a block the arena is about to free.
pub fn forget(ptr: *mut u8) {
    if !enabled() {
        return;
    }
    let _b = allocator::bypass();
    if let Some(m) = lock().as_mut() {
        m.remove(&(ptr as usize));
    }
}

/// heap_4's cost for a block of `size` bytes: an 8-byte header, then the
/// whole thing rounded up to 8.
fn heap4_block_bytes(size: u32) -> u32 {
    (size + 8 + 7) & !7
}

/// A resolved frame: `crate::path::function (file:line)`, or `None` when the
/// frame is runtime plumbing the report should skip.
fn resolve(ip: usize, cache: &mut HashMap<usize, Option<String>>) -> Option<String> {
    if let Some(hit) = cache.get(&ip) {
        return hit.clone();
    }
    // The unwinder ends a stack with a zero frame.
    if ip == 0 {
        return None;
    }
    let mut out: Option<String> = None;
    // The return address points past the call; one byte back lands inside
    // the call instruction, which is what the line table knows about.
    backtrace::resolve((ip - 1) as *mut core::ffi::c_void, |sym| {
        if out.is_some() {
            return;
        }
        let Some(name) = sym.name() else {
            return;
        };
        let name = format!("{name:#}");
        let ours = name.starts_with("picodroid")
            || name.starts_with("pico_jvm")
            || name.starts_with("pd_")
            || name.contains("picodroid_core::")
            || name.contains("pico_jvm::");
        let plumbing = name.contains("hal::sim::allocator")
            || name.contains("hal::sim::alloc_ledger")
            || name.contains("hal::sim::freertos_heap_shim");
        if !ours || plumbing {
            return;
        }
        let mut s = name;
        if let (Some(file), Some(line)) = (sym.filename(), sym.lineno()) {
            let base = file
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            s.push_str(&format!(" ({base}:{line})"));
        }
        out = Some(s);
    });
    cache.insert(ip, out.clone());
    out
}

/// Print the live-block attribution. Under the allocator bypass, like the
/// rest of the census.
pub fn print_census() {
    if !enabled() {
        println!(
            "[memmon] census native: sites off (PICODROID_MEMDIAG_SITES=1 to attribute the arena)"
        );
        return;
    }
    let _b = allocator::bypass();
    let top_n = getenv_u32(b"PICODROID_MEMDIAG_SITES_TOP\0").unwrap_or(24) as usize;
    let g = lock();
    let Some(map) = g.as_ref() else {
        println!("[memmon] census native: no live blocks recorded");
        return;
    };

    // Totals and the size-class histogram: payload vs what heap_4 charges.
    const EDGES: [u32; 9] = [16, 32, 64, 128, 256, 512, 1024, 4096, u32::MAX];
    let mut bucket_n = [0u32; 9];
    let mut bucket_b = [0u64; 9];
    let (mut payload, mut charged, mut count) = (0u64, 0u64, 0u32);
    for e in map.values() {
        payload += e.size as u64;
        charged += heap4_block_bytes(e.size) as u64;
        count += 1;
        let i = EDGES.iter().position(|&edge| e.size <= edge).unwrap_or(8);
        bucket_n[i] += 1;
        bucket_b[i] += e.size as u64;
    }
    let skipped_b = SKIPPED_BYTES.load(Ordering::Relaxed);
    let skipped_n = SKIPPED_COUNT.load(Ordering::Relaxed);
    println!(
        "[memmon] census native: live blocks={count} payload={payload}B heap4={charged}B (header+align tax {}B) untraced<{}B: {skipped_n}n/{skipped_b}B (cumulative)",
        charged - payload,
        MIN_BYTES.load(Ordering::Relaxed)
    );
    print!("[memmon] census native sizes:");
    let labels = [
        "<=16", "<=32", "<=64", "<=128", "<=256", "<=512", "<=1K", "<=4K", ">4K",
    ];
    for i in 0..9 {
        if bucket_n[i] > 0 {
            print!(" {}={}n/{}B", labels[i], bucket_n[i], bucket_b[i]);
        }
    }
    println!();

    // Group by full stack, then by innermost site.
    let mut by_stack: HashMap<&[usize], (u64, u32)> = HashMap::new();
    for e in map.values() {
        let s = by_stack.entry(&e.frames).or_insert((0, 0));
        s.0 += e.size as u64;
        s.1 += 1;
    }
    let mut stacks: Vec<(&[usize], u64, u32)> =
        by_stack.iter().map(|(k, v)| (*k, v.0, v.1)).collect();
    stacks.sort_unstable_by_key(|s| core::cmp::Reverse(s.1));

    let mut cache: HashMap<usize, Option<String>> = HashMap::new();
    let interesting = |frames: &[usize], cache: &mut HashMap<usize, Option<String>>| {
        let mut v = Vec::new();
        for &ip in frames {
            if let Some(s) = resolve(ip, cache) {
                v.push(s);
            }
        }
        v
    };

    // Sites: every stack's innermost frame of ours, summed. Symbolising
    // every distinct stack is the cost here; the cache keeps it to one
    // resolve per distinct address.
    let mut by_site: HashMap<String, (u64, u32)> = HashMap::new();
    let mut stack_frames: Vec<Vec<String>> = Vec::with_capacity(stacks.len());
    for (frames, bytes, n) in &stacks {
        let fr = interesting(frames, &mut cache);
        let site = fr.first().cloned().unwrap_or_else(|| "?".into());
        let s = by_site.entry(site).or_insert((0, 0));
        s.0 += bytes;
        s.1 += n;
        stack_frames.push(fr);
    }
    let mut sites: Vec<(&String, u64, u32)> = by_site.iter().map(|(k, v)| (k, v.0, v.1)).collect();
    sites.sort_unstable_by_key(|s| core::cmp::Reverse(s.1));
    println!(
        "[memmon] census native sites: {} distinct, top {}",
        sites.len(),
        top_n.min(sites.len())
    );
    for (site, bytes, n) in sites.iter().take(top_n) {
        println!("[memmon]   {bytes:>7}B {n:>5}n  {site}");
    }

    println!(
        "[memmon] census native stacks: {} distinct, top {}",
        stacks.len(),
        top_n.min(stacks.len())
    );
    for (i, (_, bytes, n)) in stacks.iter().take(top_n).enumerate() {
        let fr = &stack_frames[i];
        let shown: Vec<&str> = fr.iter().take(5).map(|s| s.as_str()).collect();
        println!("[memmon]   {bytes:>7}B {n:>5}n  {}", shown.join(" <- "));
    }
}
