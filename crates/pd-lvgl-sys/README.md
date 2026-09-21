# pd-lvgl-sys

Hand-written `no_std` FFI bindings for [LVGL](https://lvgl.io) v9.6 — the
subset picodroid calls — together with the build of the vendored C sources
in `third_party/lvgl`.

- `src/lib.rs`: opaque pointer types, the `#[repr(C)]` structs LVGL hands
  back, the enum-value constants, and one `extern "C"` block. Unit tests parse
  the vendored headers and fail if a constant drifts from its C definition.
- `build.rs` + `lvgl/lv_conf.h`: compiles LVGL with `cc`, applying the active
  board's `lv_dpi` / `lv_mem_kb` / `lv_mem_in_psram` and, for panels that
  scroll their own frame memory, `lvgl/hw_vscroll.c`. Non-ARM targets get
  `-fshort-enums` so C and Rust agree on enum width.
- `links = "lvgl"`, so a second crate compiling LVGL is a Cargo error.

The board is chosen by a forwarded `board-*` feature and read from
`platforms/*/boards/<name>/board.toml`; with none, `lv_conf.h` defaults apply.
The safe widget/engine layer on top lives in picodroid-core's
`graphics/lvgl/` for now (it still depends on picodroid's HAL facade).

```sh
cargo test -p pd-lvgl-sys
```
