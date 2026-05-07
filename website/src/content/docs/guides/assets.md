---
title: "Bundled image assets"
description: "Ship pre-decoded PNG images inside the PAPK and load them with ImageView.setImageSource at zero runtime cost."
---

Picodroid PAPK format **v1.1** adds an `ASST` ("asset") section that carries pre-decoded PNG images as LVGL-native RGB565 structures. The framework builds the asset at PAPK-pack time, embeds it directly in the file, and the firmware maps it from XIP flash at runtime — `ImageView.setImageSource("foo.png")` becomes a name-keyed lookup with no PNG decoder on the device.

This guide covers the manifest format, the build pipeline, and the runtime API.

## Manifest format

In your app's directory (e.g. `examples/imagedemo/`), declare the assets next to the Java sources:

```
examples/imagedemo/
  app.toml
  assets/
    icon.png
    splash.png
  src/main/java/imagedemo/ImageDemoApp.java
```

`papk-pack` discovers anything under `assets/` and emits one entry per file into the PAPK ASST section.

PNG decoding happens **at pack time** on the host. The on-device firmware sees only RGB565 framebuffers + dimensions. PNG, JPEG, GIF, and any other formats `papk-pack` learns to read in the future all collapse to the same on-device representation.

## API: `ImageView.setImageSource(String)`

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView();
img.setImageSource("icon.png");   // matches assets/icon.png in the manifest
```

The string is the asset's relative path under `assets/` — no leading slash, forward-slashes for subdirectories. The lookup is a hash on the asset name and runs in O(1) per `setImageSource`.

If the asset is missing, the call logs a warning and the `ImageView` stays empty. There is no exception in v1 — the lookup is best-effort.

### Scale, tint, and aspect

The full Tier C ImageView surface lives in [Graphics & UI → ImageView](/api/ui/#picodroidwidgetimageview):

```java
img.setScaleType(ImageView.SCALE_FIT_CENTER);
img.setScale(150);                // 100 = 1.0×
img.setTint(Color.RED);
```

For scaled rendering to be anti-aliased, `LV_DRAW_SW_SUPPORT_RGB565A8` must be enabled in `lv_conf.h` (it is, by default). Without it scaled images render aliased.

## PAPK compatibility

Bundled assets land outside the framework class set, so they don't change the **shrink map**. v1.1 PAPKs run unchanged on v1.0 firmware **only if they don't reference an asset** — the firmware will skip the ASST section. PAPKs that call `setImageSource` need v1.1 firmware (any picodroid release ≥ v0.8.0).

The `papk-info` tool prints both the manifest and the asset table:

```bash
cargo run -p papk-info -- build/apks/imagedemo.papk
```

## Internals (for the curious)

- ASST entries are stored as `[u32 name_hash][u16 width][u16 height][u32 byte_count][u8...]` triples followed by a string table.
- The firmware-side resolver lives in `src/papk/assets.rs` and registers each entry with LVGL's image cache as `lv_img_dsc_t` pointers into XIP flash.
- The `assets/` directory must be under 256 KiB total per PAPK in v1.1; the framework reserves the remainder of the binary section for code.
- Re-pack any PAPK that was built before v1.1 if you start using `setImageSource` — `pdb install` will reject the older format with `FrameworkVersionMismatch`.

See [`examples/imagedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/imagedemo) for a worked example.
