/// Park the calling core in a RAM spin loop with interrupts disabled.
///
/// Called from jvm_task (core 0) so that core 0 does not access flash (via XIP)
/// while the install task (core 1) erases or programs flash.  The RP2040 SSI is
/// shared between both cores; any XIP access during a ROM flash operation causes
/// undefined behaviour (bus hang or hard-fault).
///
/// Core 0 stays parked until the install coordinator sets
/// [`crate::pdb::pending::CORE0_RELEASE`].
///
/// # Safety
/// Must only be called from core 0 after the JVM and all child threads have
/// exited.  Core 1 must eventually set `CORE0_RELEASE` to avoid permanent
/// lockup.
#[cfg(not(feature = "sim"))]
#[link_section = ".data"]
#[inline(never)]
pub unsafe fn park_for_flash() {
    use crate::pdb::pending;

    // Pre-compute flag addresses while XIP is still enabled (as_ptr() may not
    // be inlined in debug builds; calling before cpsid i is safe).
    let parked_addr = pending::CORE0_PARKED.as_ptr() as usize;
    let release_addr = pending::CORE0_RELEASE.as_ptr() as usize;
    let park_req_addr = pending::FLASH_PARK_REQUESTED.as_ptr() as usize;

    let primask: u32;
    core::arch::asm!(
        "mrs {0}, PRIMASK",
        out(reg) primask,
        options(nomem, nostack, preserves_flags)
    );
    core::arch::asm!("cpsid i", options(nomem, nostack));

    // Signal core 1 we are parked.
    //
    // Use raw strb/ldrb inline asm — read_volatile/write_volatile are only
    // #[inline] (not #[inline(always)]), so in debug builds they are NOT
    // inlined and call into .text (flash).  That crashes once core 1's
    // flash_exit_xip has disabled XIP.  Inline asm is guaranteed to be a
    // single strb/ldrb with no function call.
    //
    // dmb before strb = Release fence; ldrb then dmb = Acquire load.
    core::arch::asm!(
        "dmb sy",
        "strb {val}, [{ptr}]",
        val = in(reg) 1u32,
        ptr = in(reg) parked_addr,
        options(nostack, preserves_flags)
    );

    // Spin until core 1 sets CORE0_RELEASE.
    // NO `readonly` — that would let the compiler hoist the load out of the
    // loop, reading CORE0_RELEASE only once.  Without `readonly` the asm is
    // treated as a memory side-effect and re-executed on every iteration.
    loop {
        let val: u32;
        core::arch::asm!(
            "ldrb {val}, [{ptr}]",
            "dmb sy",
            val = out(reg) val,
            ptr = in(reg) release_addr,
            options(nostack, preserves_flags)
        );
        if val != 0 {
            break;
        }
        // Use inline asm, NOT cortex_m::asm::nop() — the cortex-m crate's
        // nop() calls `__nop` via a thunk into flash, which faults once
        // core 1 has disabled XIP.
        core::arch::asm!("nop", options(nomem, nostack, preserves_flags));
    }

    // Clear all flags (interrupts still off, no reorder risk).
    core::arch::asm!(
        "strb {z}, [{p1}]",
        "strb {z}, [{p2}]",
        "strb {z}, [{p3}]",
        z  = in(reg) 0u32,
        p1 = in(reg) parked_addr,
        p2 = in(reg) release_addr,
        p3 = in(reg) park_req_addr,
        options(nostack, preserves_flags)
    );

    if primask == 0 {
        core::arch::asm!("cpsie i", options(nomem, nostack));
    }
}
