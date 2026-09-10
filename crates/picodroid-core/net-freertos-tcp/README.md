# net-freertos-tcp — the stack glue every FreeRTOS+TCP family shares

Written once, compiled by each family's `build.rs` through
`build_support::network::build_freertos_tcp` (it must be compiled against the
family's `FreeRTOSConfig.h` and kernel port headers, so core cannot compile it
itself; the LVGL precedent does not transfer).

| File | What it is |
|---|---|
| `net_init.c` | `picodroid_net_stack_init(mac)`: registers the link driver's interface, fills a DHCP endpoint, starts the IP task. Plus the five FreeRTOS+TCP application hooks. |
| `libc_str.c` | `strcmp`/`strncmp`/`strcpy`/`strncpy`/`strchr` for targets that link without a libc (FreeRTOS+TCP's DNS files need them). |
| `FreeRTOSIPConfig.h` | The IP stack policy. Its first include is the family's `FreeRTOSIPConfig_family.h`. |

Two seams a family or link driver provides (link-time C symbols):

- `NetworkInterface_t *pxPicodroidNetLink_FillInterfaceDescriptor(BaseType_t, NetworkInterface_t *)`
  — defined by the link driver's `NetworkInterface_<X>.c`.
- `uint32_t picodroid_port_entropy32(void)` — defined by the family
  (`platforms/rp/src/hal/rp/entropy.rs`).

One symbol core provides: `picodroid_net_ip_event(up, ip_nbo)`.

Design: `docs/designs/network-seam-2026-09.md`. Invariants a host test pins:
`picodroid-core/src/hal/freertos_tcp/config_guard.rs`.
