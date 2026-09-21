# heap4

A bit-faithful Rust port of FreeRTOS `heap_4.c` that keeps the device's
32-bit block arithmetic on any host: headers live inside the arena as
`{next_off: u32, size_and_flag: u32}`, so every size, split and coalesce
decision matches a 32-bit MCU even when the host is 64-bit. Compiling the C
for a 64-bit host doubles the header and moves the allocated bit, which is
exactly the difference a host-side OOM or fragmentation test must not have.

`Heap4::init(base, size)` takes a caller-owned arena; `malloc` / `free` work
in arena offsets; `stats()` mirrors `vPortGetHeapStats`. `no_std`, no
dependencies. The tests replay a free/min-ever trace captured from a real
RP2350.

picodroid's simulator allocator (`crates/picodroid-core/src/hal/sim/allocator.rs`)
is the first consumer.

```sh
cargo test -p heap4
```
