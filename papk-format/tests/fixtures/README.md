# Golden PAPK fixtures

These `.papk` files were produced by the **pre-refactor** `papk-pack` CLI
(repo state: commit `7234d0b`, 2026-07-25) and are the byte-for-byte ground
truth for `papk-format`'s parser and `PapkBuilder` writer. Do NOT regenerate
them casually — the whole point is that they pin the on-disk layout emitted
by the original writer. If the format ever changes intentionally, cut new
fixtures with a new minor/major version and keep these for the old version.

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

`minimal.papk` (432 bytes — MANI + CLSS, no ASSETS section):

```bash
cargo run -p papk-pack --target x86_64-unknown-linux-gnu -- \
  --main-class fixture/Main \
  --package-name fixture \
  --version 1.0 \
  --framework-map-version 0.0.0 \
  --classes-dir $WORK/fixture-classes \
  --output papk-format/tests/fixtures/minimal.papk
```

`with-assets.papk` (608 bytes — MANI + CLSS + ASST, one 8x8 RGB565 asset):

```bash
cargo run -p papk-pack --target x86_64-unknown-linux-gnu -- \
  --main-class fixture/Main \
  --package-name fixture \
  --version 1.0 \
  --framework-map-version 0.0.0 \
  --classes-dir $WORK/fixture-classes \
  --assets-dir $WORK/fixture-assets \
  --output papk-format/tests/fixtures/with-assets.papk
```

(`--target <host triple>` is required because the workspace's default build
target is `thumbv6m-none-eabi`; substitute the output of
`rustc -vV | grep host` on non-x86_64 hosts.)

## Known contents (asserted by tests/golden.rs)

Both files: header `PAPK`, version 1.1, `manifest_offset` 24; manifest keys
in order: `main-class=fixture/Main`, `package-name=fixture`, `version=1.0`,
`framework-map-version=0.0.0`; one class `fixture/Main` whose data is exactly
`Main.class`.

`minimal.papk`: `section_count` 2, `assets_offset` 0.
`with-assets.papk`: `section_count` 3, one asset `gradient.png`
(8x8, cf `0x12` = `LV_COLOR_FORMAT_RGB565`, stride 0, 128 data bytes).
