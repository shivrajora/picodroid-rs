use core::sync::atomic::{AtomicU32, Ordering};

static GPIO_OE: AtomicU32 = AtomicU32::new(0);
static GPIO_OUT: AtomicU32 = AtomicU32::new(0);

pub(super) fn set_direction(pin: u8, direction: i32) {
    GPIO_OE.fetch_or(1u32 << pin, Ordering::Relaxed);
    if direction == 1 {
        GPIO_OUT.fetch_or(1u32 << pin, Ordering::Relaxed);
        println!("[sim] GP{pin}: output, initially HIGH");
    } else {
        GPIO_OUT.fetch_and(!(1u32 << pin), Ordering::Relaxed);
        println!("[sim] GP{pin}: output, initially LOW");
    }
}

pub(super) fn set_value(pin: u8, high: bool) {
    if high {
        GPIO_OUT.fetch_or(1u32 << pin, Ordering::Relaxed);
    } else {
        GPIO_OUT.fetch_and(!(1u32 << pin), Ordering::Relaxed);
    }
    println!("[sim] GP{pin}: {}", if high { "HIGH" } else { "LOW" });
}
