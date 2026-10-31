---
title: "Bundled image assets"
description: "Ship pre-decoded PNG images inside the PAPK and show them with ImageView.setImageResource(R.drawable.x) at zero runtime cost."
---

A PAPK can carry an `ASST` ("asset") section of pre-decoded PNG images as LVGL-native RGB565 structures (the section arrived with format v1.1 and is unchanged in today's v2). The framework builds the asset at PAPK-pack time, embeds it directly in the file, and the firmware maps it from XIP flash at runtime: `ImageView.setImageResource(R.drawable.logo)` becomes a lookup with no PNG decoder on the device.

This guide covers where the images go, the build pipeline, and the runtime API.

## Where images go

An app's own images go under `res/drawable/`, as on Android, and are referenced by id:
`R.drawable.logo` in Java, `android:src="@drawable/logo"` in a layout (see
[resources](/guides/resources/#drawables)). They are stored in the ASSETS section under the name
`res/drawable/<file>`.

```
examples/imagedemo/
  PicodroidManifest.xml
  build.gradle.kts
  res/drawable/
    logo.png
  assets/
    logo.png          (the launcher icon the manifest names)
  java/imagedemo/ImageDemoApp.java
```

The `assets/` directory holds the file the manifest's `icon` names (below); `papk-pack` emits one
entry per file under it too.

PNG decoding happens **at pack time** on the host. The on-device firmware sees only RGB565 framebuffers + dimensions. Today `papk-pack` bundles **PNG only** — the file scan is case-insensitive on the `.png` extension and any other file is silently skipped (the host packer is built with the PNG codec alone). Alpha is discarded during the RGB565 conversion, so transparency is not preserved.

## API: `ImageView.setImageResource(int)`

```java
import picodroid.widget.ImageView;

ImageView img = new ImageView(this);
img.setImageResource(R.drawable.logo);   // res/drawable/logo.png
```

A missing resource throws `Resources.NotFoundException`, as on Android; since the id comes from
the generated `R`, that means a package built wrong, not a typo.

## App icon

The manifest's `<application icon="…">` names one of the files under `assets/`
as the app's icon — what a launcher shows beside the app's `android:label`. (`icon` is Picodroid's attribute, bare: Android's `android:icon` names a drawable resource.) The file
must exist (the Gradle build fails at configuration time when it does not) and
should be a small square: 48×48 is the convention, 64×64 the point past which
`papk-pack` warns, because every icon costs `width × height × 2` bytes of flash
and a launcher scales anything larger. See the
[manifest reference](/reference/manifest/#app-identity).

```xml
<application android:name=".MyApp" android:label="My App" icon="icon.png" />
```

### Scale type and tint

The full ImageView surface lives in [Graphics & UI → ImageView](/api/ui/#picodroidwidgetimageview):

```java
img.setScaleType(ImageView.ScaleType.CENTER_CROP);
img.setColorFilter(Color.argb(128, 255, 0, 0));   // the colour's alpha is the blend strength
```

For scaled rendering to be anti-aliased, `LV_DRAW_SW_SUPPORT_RGB565A8` must be enabled in `lv_conf.h` (it is, by default). Without it scaled images render aliased.

## Step by step: from PNG to screen

1. **Size the PNG for the screen.** Each bundled image costs `width × height × 2` bytes on flash (RGB565, 2 bytes per pixel — there is no compression on device). Budget against the app region every installed app shares — `app_region_kb` in the board's `board.toml`: 1536 KiB on `testbench_rp2350` and `pico_enviro_mon`, 1280 KiB on the RP2350 WiFi boards, 10 MiB on `pico_touch_kit`, and 768 KiB on `testbench_rp2040` (see [limits](/reference/limits/)):

   | Image | Bytes | On flash |
   |-------|-------|----------|
   | 32×32 | 32 × 32 × 2 | 2 KiB |
   | 64×64 | 64 × 64 × 2 | 8 KiB |
   | 128×128 | 128 × 128 × 2 | 32 KiB |
   | 240×240 (full screen) | 240 × 240 × 2 | 112.5 KiB |

   Keep a full-screen background to one image — two of them already eat a quarter of a megabyte. As a rule of thumb keep the images well under ~256 KiB in total so code and bytecode still fit; the only hard error the packer raises is for a single image larger than 65535 px on a side.

2. **Drop it in `res/drawable/`.** Create the directory next to your Java sources and add the `.png` (no manifest entry needed); the build generates `R.drawable.logo` for `logo.png`:

   ```text
   examples/myapp/
     PicodroidManifest.xml
     build.gradle.kts
     res/drawable/
       logo.png
     java/myapp/MyApp.java
   ```

3. **Build the PAPK.**

   ```bash
   ./scripts/build-apk.sh --app myapp
   ```

4. **Confirm the asset landed** with `papk-info` — it prints the ASST table with each image's dimensions, color format, and size:

   ```bash
   ./scripts/papk-info.sh build/apks/myapp.papk
   ```

   ```text
   Assets  (8220 bytes @ 0x608)  tag "ASST"
     ┌───────────────────────┬───────┬───────────────┬─────────┐
     │ Asset                 │ Dim   │ Format        │    Size │
     ├───────────────────────┼───────┼───────────────┼─────────┤
     │ res/drawable/logo.png │ 64x64 │ RGB565 (0x12) │ 8.0 KiB │
     └───────────────────────┴───────┴───────────────┴─────────┘
   ```

   If your image isn't in the table, the file name didn't end in `.png` or wasn't directly under `res/drawable/`.

5. **Show it.** Reference it by id:

   ```java
   ImageView img = new ImageView(this);
   img.setImageResource(R.drawable.logo);
   ```

## PAPK compatibility

Bundled assets land outside the framework class set, so they don't change the **shrink map**, and `--shrink` / `--shrink-app` never rename an asset: a drawable is reached by its resource id, and the icon by the manifest's file name.

The container itself is at format **v3**: classes are linked when the app is packed, and `pdb install` and the firmware read only that version (`PAPK file is not a valid PAPK: PAPK format version is not the one this build reads`). Sections are 2-byte aligned, so an image's pixels never land at an odd address; the firmware skips, rather than draws, an image whose pixels do and logs `[assets] <name> skipped: pixel data is not 2-byte aligned`.

The `papk-info` tool prints both the manifest and the asset table:

```bash
./scripts/papk-info.sh build/apks/imagedemo.papk
```

## Internals (for the curious)

- The ASST section is a `[u32 count]` followed by one record per asset: `[u16 name_len][name bytes][u16 width][u16 height][u8 cf][u8 reserved0][u16 stride (0 = derive from width + cf)][u32 data_len]`, each record padded to a 4-byte boundary before and after its pixel data. Every PAPK section also starts on a 4-byte boundary in the file, so pixel data sits at an aligned flash address — LVGL reads it in place as 16-bit words, and a Cortex-M0+ (RP2040) hard-faults on an unaligned halfword read.
- The firmware-side resolver lives in `crates/picodroid-core/src/graphics/assets.rs` and registers each entry with LVGL's image cache as `lv_img_dsc_t` pointers into XIP flash.
- There is no asset-section byte cap in the packer — the only enforced image limit is a per-axis maximum of 65535 px. The real ceiling is the app region the PAPK is installed into, shared with its classes and manifest and with every other installed app, so keep the images modest (a few hundred KiB at most) to leave room for code.
- Another app's icon reaches your app as a `BitmapDrawable` from `PackageManager.getApplicationIcon`, read out of that app's installed image; show it with `ImageView.setImageDrawable`. That is how the launcher draws its rows.

See [`examples/imagedemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/imagedemo) for a worked example.
