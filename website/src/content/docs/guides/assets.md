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
  PicodroidManifest.xml
  build.gradle.kts
  assets/
    logo.png
  java/imagedemo/ImageDemoApp.java
```

`papk-pack` discovers anything under `assets/` and emits one entry per file into the PAPK ASST section.

PNG decoding happens **at pack time** on the host. The on-device firmware sees only RGB565 framebuffers + dimensions. Today `papk-pack` bundles **PNG only** — the file scan is case-insensitive on the `.png` extension and any other file in `assets/` is silently skipped (the host packer is built with the PNG codec alone). Alpha is discarded during the RGB565 conversion, so transparency is not preserved.

## API: `ImageView.setImageSource(String)`

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView();
img.setImageSource("logo.png");   // matches assets/logo.png in the app directory
```

The string is the asset's file name — `setImageSource("logo.png")` matches `assets/logo.png`. The packer scans `assets/` flat (non-recursive), so the key is just the base file name including its extension. Each `setImageSource` is a small linear scan of the registry.

If the asset name doesn't match anything, the call does nothing — **no warning, no exception, no change** to the widget. The lookup is silently best-effort, so double-check the spelling against `papk-info` (below) if an image doesn't appear.

### Scale, tint, and aspect

The full Tier C ImageView surface lives in [Graphics & UI → ImageView](/api/ui/#picodroidwidgetimageview):

```java
img.setScaleType(ImageView.SCALE_FIT_CENTER);
img.setScale(150);                // 100 = 1.0×
img.setTint(Color.RED);
```

For scaled rendering to be anti-aliased, `LV_DRAW_SW_SUPPORT_RGB565A8` must be enabled in `lv_conf.h` (it is, by default). Without it scaled images render aliased.

## Step by step: from PNG to screen

1. **Size the PNG for the screen.** Each bundled image costs `width × height × 2` bytes on flash (RGB565, 2 bytes per pixel — there is no compression on device). Budget against the board's flash and the PAPK install ceiling (~1020 KiB total per PAPK):

   | Image | Bytes | On flash |
   |-------|-------|----------|
   | 32×32 | 32 × 32 × 2 | 2 KiB |
   | 64×64 | 64 × 64 × 2 | 8 KiB |
   | 128×128 | 128 × 128 × 2 | 32 KiB |
   | 240×240 (full screen) | 240 × 240 × 2 | 112.5 KiB |

   Keep a full-screen background to one image — two of them already eat a quarter of a megabyte. As a rule of thumb keep the `assets/` total well under ~256 KiB so code and bytecode still fit; the only hard error the packer raises is for a single image larger than 65535 px on a side.

2. **Drop it in `assets/`.** Create the directory next to your Java sources and add the `.png` (no manifest entry needed — the build auto-detects `assets/`):

   ```text
   examples/myapp/
     PicodroidManifest.xml
     build.gradle.kts
     assets/
       logo.png
     java/myapp/MyApp.java
   ```

3. **Build the PAPK.**

   ```bash
   ./scripts/build-apk.sh --app myapp
   ```

4. **Confirm the asset landed** with `papk-info` — it prints the ASST table with each image's dimensions, color format, and size:

   ```bash
   cargo run -p papk-info -- build/apks/myapp.papk
   ```

   ```text
   Assets  (8220 bytes @ 0x608)  tag "ASST"
     ┌──────────┬───────┬───────────────┬─────────┐
     │ Asset    │ Dim   │ Format        │    Size │
     ├──────────┼───────┼───────────────┼─────────┤
     │ logo.png │ 64x64 │ RGB565 (0x12) │ 8.0 KiB │
     └──────────┴───────┴───────────────┴─────────┘
   ```

   If your image isn't in the table, the file name didn't end in `.png` or wasn't directly under `assets/`.

5. **Show it.** Reference the file by name:

   ```java
   ImageView img = new ImageView();
   img.setImageSource("logo.png");
   ```

## PAPK compatibility

Bundled assets land outside the framework class set, so they don't change the **shrink map**. v1.1 PAPKs run unchanged on v1.0 firmware **only if they don't reference an asset** — the firmware will skip the ASST section. PAPKs that call `setImageSource` need v1.1 firmware (any picodroid release ≥ v0.8.0).

The `papk-info` tool prints both the manifest and the asset table:

```bash
cargo run -p papk-info -- build/apks/imagedemo.papk
```

## Internals (for the curious)

- The ASST section is a `[u32 count]` followed by one record per asset: `[u16 name_len][name bytes][u16 width][u16 height][u8 cf][u8 reserved0][u16 stride (0 = derive from width + cf)][u32 data_len]`, each record padded to a 4-byte boundary before and after its pixel data.
- The firmware-side resolver lives in `platforms/rp/src/system/picodroid/graphics/assets.rs` and registers each entry with LVGL's image cache as `lv_img_dsc_t` pointers into XIP flash.
- There is no asset-section byte cap in the packer — the only enforced image limit is a per-axis maximum of 65535 px. The real ceiling is the whole-PAPK install limit (~1020 KiB), shared with classes and the manifest, so keep `assets/` modest (a few hundred KiB at most) to leave room for code.
- Re-pack any PAPK that was built before v1.1 if you start using `setImageSource` — `pdb install` will reject the older format with `FrameworkVersionMismatch`.

See [`examples/imagedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/imagedemo) for a worked example.
