# Storage: Files and Preferences

On-device persistent storage. Packages: `picodroid.io` (raw files) and `picodroid.content` (typed key-value settings). See [docs/README.md](../README.md) for the full API index.

Both APIs sit on top of an on-chip [LittleFS](https://github.com/littlefs-project/littlefs) volume. On hardware the volume lives in a dedicated flash region; under the simulator it is backed by a host file (`build/sim/littlefs.img`) so writes survive across `sim.sh` runs.

## `picodroid.io` — Files

`picodroid.io.File`, `FileInputStream`, and `FileOutputStream` provide a stripped-down `java.io`-style API.

Each `read()` / `write()` is independent — there is no native file handle to keep open, so `close()` is a no-op (it is provided so the streams can still be used in try-with-resources blocks).

```java
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;

File f = new File("/data/notes.txt");
boolean exists = f.exists();
boolean isFile = f.isFile();
long    size   = f.length();
boolean ok     = f.delete();

File dir = new File("/data");
dir.mkdir();
new File("/data/old.txt").renameTo(new File("/data/new.txt"));

// Append a line
try (FileOutputStream out = new FileOutputStream("/data/log.txt", /*append=*/true)) {
    out.write("hello\n".getBytes());
    out.flush();
}

// Read it back
try (FileInputStream in = new FileInputStream(new File("/data/log.txt"))) {
    byte[] buf = new byte[64];
    int n = in.read(buf);
    Log.i("FS", "read " + n + " bytes");
}
```

| Class | Selected methods |
|-------|------------------|
| `File` | `exists()`, `isFile()`, `isDirectory()`, `length()`, `delete()`, `mkdir()`, `renameTo(File)` |
| `FileInputStream` | `read(byte[], int, int)`, `read(byte[])`, `available()`, `close()` |
| `FileOutputStream` | `write(byte[], int, int)`, `write(byte[])`, `write(int)`, `flush()`, `close()`; constructors `(File)`, `(String)`, `(String, boolean append)` |

## `picodroid.content.Preferences`

Typed key-value settings store inspired by Jetpack DataStore. Backed by a CRC32-protected blob written atomically (tmp file + rename) into `/prefs/<name>` on the LittleFS volume.

Supported value types: `String`, `int`, `long`, `boolean`. Limits: 64 entries per file, 63-char keys, 1024-char string values, 4 KB total blob.

```java
import picodroid.content.Preferences;
import picodroid.content.Editor;

Preferences prefs = Preferences.open("settings");
int boots = prefs.getInt("boot_count", 0);

Editor e = prefs.edit();
e.putInt("boot_count", boots + 1);
e.putString("device_name", "pico-01");
e.putBoolean("debug", true);
boolean ok = e.commit();      // false on I/O failure

if (prefs.contains("device_name")) {
    String name = prefs.getString("device_name", "");
}
```

`commit()` is atomic with respect to power loss: it writes to a `.tmp` file, verifies the size, and only then renames into place. A corrupt blob (failed CRC32) is silently treated as empty on the next `open()`. `Preferences` instances are not thread-safe — synchronize externally if shared.

---

**See also:** [core.md](core.md) (Java language) · [system.md](system.md) (logging, clock, threads) · [peripherals.md](peripherals.md) (GPIO, UART, I2C, SPI, PWM, ADC) · [networking.md](networking.md) (sockets) · [ui.md](ui.md) (display, widgets)
