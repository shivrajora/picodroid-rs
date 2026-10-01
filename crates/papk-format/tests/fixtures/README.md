# Golden PAPK fixtures

These `.papk` files were produced by `papk-pack` (format **v2**, link-table
layout 2 of 2026-09-29: the linked-class layout of
docs/designs/class-link-2026-09.md plus the section's string-literal pool)
and are the
byte-for-byte ground truth for `papk-format`'s parser and writer: the
parser must extract the known contents, and `PapkBuilder` must reproduce
each file exactly (`tests/golden.rs`, `papk-pack`'s `pack_integration`).
Regenerate them only when the format changes on purpose, with the
invocations below, and update the sizes and contents listed here.

The v1 fixtures were dropped with v1 itself: nothing deployed carried the
old layout, so no reader for it exists.

## Inputs (checked in alongside)

- `Main.class` (261 bytes) — compiled from the source below with
  `javac 21.0.11` (`javac -d <classes-dir> fixture/Main.java`). It is checked
  in so the tests never depend on a JDK. The repo's real example apps are all
  `application`-entry (no class in the tree declares
  `static void main(String[])`, and papk-pack's `validate_entry_point` hard
  errors on a `--main-class` without one), hence this tiny purpose-built
  main-class:

  ```java
  // fixture/Main.java
  package fixture;

  /** Minimal main-class entry point for the papk-format golden fixtures. */
  public class Main {
      public static void main(String[] args) {}
  }
  ```

- `gradient.png` (170 bytes) — deterministic 8x8 RGB PNG, generated with
  Python stdlib only (no PIL). Pixel `(x, y)` has
  `r = x*32, g = y*32, b = (x^y)*32`, which lets the golden test compute the
  expected RGB565 payload independently:

  ```python
  import struct, zlib
  w = h = 8
  def chunk(typ, data):
      return struct.pack('>I', len(data)) + typ + data + \
             struct.pack('>I', zlib.crc32(typ + data) & 0xffffffff)
  rows = b''
  for y in range(h):
      rows += b'\x00'  # filter type 0 (None)
      for x in range(w):
          rows += bytes((x * 32, y * 32, (x ^ y) * 32))
  png = b'\x89PNG\r\n\x1a\n'
  png += chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
  png += chunk(b'IDAT', zlib.compress(rows, 9))
  png += chunk(b'IEND', b'')
  open('gradient.png', 'wb').write(png)
  ```

## Exact pack invocations

Layout on disk before packing (papk-pack derives the JVM class name from the
path relative to `--classes-dir`):

```text
$WORK/fixture-classes/fixture/Main.class
$WORK/fixture-assets/gradient.png
```

`minimal.papk` (576 bytes — MANI + CLSS, no ASSETS section):

```bash
cargo run -p papk-pack --target x86_64-unknown-linux-gnu -- \
  --main-class fixture/Main \
  --package-name fixture \
  --version 1.0 \
  --framework-map-version 0.0.0 \
  --classes-dir $WORK/fixture-classes \
  --output crates/papk-format/tests/fixtures/minimal.papk
```

`with-assets.papk` (752 bytes — MANI + CLSS + ASST, one 8x8 RGB565 asset):

```bash
cargo run -p papk-pack --target x86_64-unknown-linux-gnu -- \
  --main-class fixture/Main \
  --package-name fixture \
  --version 1.0 \
  --framework-map-version 0.0.0 \
  --classes-dir $WORK/fixture-classes \
  --assets-dir $WORK/fixture-assets \
  --output crates/papk-format/tests/fixtures/with-assets.papk
```

(`--target <host triple>` is required because the workspace's default build
target is `thumbv6m-none-eabi`; substitute the output of
`rustc -vV | grep host` on non-x86_64 hosts.)

## Known contents (asserted by tests/golden.rs)

Both files: header `PAPK`, version 2.0, `manifest_offset` 28; manifest keys
in order: `main-class=fixture/Main`, `package-name=fixture`, `version=1.0`,
`framework-map-version=0.0.0`; one class `fixture/Main` whose data is exactly
`Main.class`, followed by its link table (two methods, `<init>` and `main`)
a one-entry class index and an empty literal pool (the class has no
`String` constants).

`minimal.papk`: `section_count` 2, `assets_offset` 0.
`with-assets.papk`: `section_count` 3, one asset `gradient.png`
(8x8, cf `0x12` = `LV_COLOR_FORMAT_RGB565`, stride 0, 128 data bytes).
