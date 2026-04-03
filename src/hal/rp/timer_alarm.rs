//! Hardware timer alarm for core 1 park-detection on RP2350.
//!
//! On RP2350 with configTICK_CORE=0, the FreeRTOS tick freezes when core 0
//! parks (interrupts disabled).  This module uses a TIMER0 alarm that fires
//! every 1 ms on core 1, independently of FreeRTOS.  The ISR checks
//! CORE0_PARKED and signals the PDB task via the park-signal queue.

#[cfg(feature = "chip-rp2350")]
use rp235x_hal::pac;

/// TIMER0 alarm 0 IRQ number on RP2350.
#[cfg(feature = "chip-rp2350")]
const TIMER0_IRQ_0_NUM: u16 = 0;

/// Arm a repeating 1 ms alarm on TIMER0 alarm 0.
/// Must be called from core 1 (the PDB task core).
#[cfg(feature = "chip-rp2350")]
pub fn arm_park_alarm() {
    let p = unsafe { pac::Peripherals::steal() };
    let timer = &p.TIMER0;

    // Read current time and set alarm 0 to fire 1 ms from now
    let now = timer.timerawl().read().bits();
    timer
        .alarm0()
        .write(|w| unsafe { w.bits(now.wrapping_add(1_000)) });

    // Enable alarm 0 interrupt
    timer.inte().modify(|_, w| w.alarm_0().set_bit());

    // Set IRQ priority and enable in NVIC (must run on core 1)
    unsafe {
        // TIMER0_IRQ_0 is IRQ 0 on RP2350
        let nvic_ipr = 0xE000_E400 as *mut u8;
        // Priority 0x20 = level 2 (above configMAX_SYSCALL_INTERRUPT_PRIORITY = 0x10)
        // so it fires even during FreeRTOS critical sections... actually we want
        // it to be FreeRTOS-safe (calls xQueueSendFromISR), so use 0x10 (level 1).
        nvic_ipr.add(TIMER0_IRQ_0_NUM as usize).write_volatile(0x10);
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::TIMER0_IRQ_0);
    }
}

/// Disarm the park-detection alarm.
#[cfg(feature = "chip-rp2350")]
pub fn disarm_park_alarm() {
    let p = unsafe { pac::Peripherals::steal() };
    let timer = &p.TIMER0;

    // Disable alarm 0 interrupt
    timer.inte().modify(|_, w| w.alarm_0().clear_bit());

    // Mask in NVIC
    cortex_m::peripheral::NVIC::mask(pac::Interrupt::TIMER0_IRQ_0);
}

/// TIMER0_IRQ_0 handler.  Fires every ~1 ms on core 1.
/// Checks CORE0_PARKED and signals the PDB task if set.
#[cfg(feature = "chip-rp2350")]
#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn TIMER0_IRQ_0() {
    let p = unsafe { pac::Peripherals::steal() };
    let timer = &p.TIMER0;

    // Clear the alarm interrupt
    timer.intr().write(|w| w.alarm_0().clear_bit_by_one());

    // Check if core 0 has parked
    if crate::pdb::pending::CORE0_PARKED.load(core::sync::atomic::Ordering::Relaxed) {
        // Signal the PDB task via queue (from ISR context)
        crate::pdb::pending::signal_park_from_isr();
        // Don't re-arm — we're done
        return;
    }

    // Re-arm alarm for next 1 ms
    let now = timer.timerawl().read().bits();
    timer
        .alarm0()
        .write(|w| unsafe { w.bits(now.wrapping_add(1_000)) });
}
