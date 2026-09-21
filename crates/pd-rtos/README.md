# pd-rtos

The kernel seam picodroid's shared code is written against.

- `Rtos` is the trait a platform implements: task spawn, word and pointer
  queues, recursive mutexes, binary semaphores, task notifications, a periodic
  tick timer and delays. Stack sizes are bytes; core affinity and stack sizing
  policy stay with the implementor, which is told only the `TaskKind`.
- `set_rtos!(MyRtos)` binds an implementation at link time by emitting the
  `__pd_rtos_*` symbols the free functions here call. No vtable, no
  registration call, nothing to forget at boot.
- `run_lock` is a one-runner-at-a-time lock for a shared, lock-free heap:
  a task takes `Held` before it touches the heap, and every blocking wrapper
  in this crate gives the lock up for the duration of the wait and takes it
  back after. With no scheduler running every operation is a no-op, so host
  tests need no kernel.

`no_std` + `alloc`, no dependencies, no FreeRTOS: the FreeRTOS binding lives
in picodroid-core (`rtos/freertos.rs`) and in each platform's `Rtos` impl.
