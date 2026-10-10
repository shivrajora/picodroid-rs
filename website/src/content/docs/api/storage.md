---
title: "Storage: Files and Preferences"
description: "Files, the per-app sandbox, the Context file API, storage quotas, StatFs, and SharedPreferences over LittleFS."
---

On-device persistent storage. Packages: `picodroid.io` (raw files), `picodroid.content` (typed key-value settings and the `Context` file API), `picodroid.os` (`StatFs`) and `picodroid.app.usage` (`StorageStatsManager`). See [Java API overview](/api/) for the full API index.

Both APIs sit on top of an on-chip [LittleFS](https://github.com/littlefs-project/littlefs) volume. On hardware the volume lives in a dedicated flash region (`fs_kb` in the MCU or board toml: 512 KB on most RP2350 boards, 4 MB on `pico_touch_kit`, 128 KB on the RP2040 testbench); under the simulator it is backed by a host file of the same size (`crates/picodroid-core/target/sim-fs.img`, overridable via the `PICODROID_SIM_FS` env var; `PICODROID_SIM_FS_KB` overrides the size, and an image of another size is started afresh) so writes survive across `sim.sh` runs.

## Every app has its own root

An app never sees the volume itself. Every path it names — `new File("/notes.txt")`, a `SharedPreferences` file, `getFilesDir()` — is resolved under the app's own directory, `/data/<package>` on the volume, where `<package>` is the manifest's `package`. The app's `/` *is* that directory: it cannot name another app's files, `..` is refused (a predicate such as `exists()` answers `false`; a write throws `IOException`), empty and `.` segments are dropped, and both ends of a `renameTo` are mapped. Uninstalling an app removes its directory. This is the same model Android enforces below the app, here at the only seam Java can reach storage through, and it holds on every board, single-app ones included.

Two costs are worth knowing: an app path is at most 185 bytes (LittleFS itself allows 255 per segment), and every directory costs LittleFS an 8 KB metadata pair — an app that uses `/prefs` and `/files` holds 24 KB of metadata before its first byte of data. See [limits](/reference/limits/).

## `picodroid.io` — Files

`picodroid.io.File`, `FileInputStream`, and `FileOutputStream` provide a stripped-down `java.io`-style API. `FileInputStream` **is a** `java.io.InputStream` and `FileOutputStream` **is a** `java.io.OutputStream`, so they wrap in the `java.io` readers and writers exactly as on Android (see [`java.io` streams](/api/core/#javaio-streams)):

```java
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintWriter;

try (PrintWriter out = new PrintWriter(openFileOutput("log.txt", MODE_APPEND))) {
    out.println("boot " + SystemClock.uptimeMillis());
}
try (BufferedReader in = new BufferedReader(new InputStreamReader(openFileInput("log.txt")))) {
    String line;
    while ((line = in.readLine()) != null) {
        Log.i("FS", line);
    }
}
```

Each `read()` / `write()` is independent — there is no native file handle to keep open, so `close()` is a no-op (it is provided so the streams can still be used in try-with-resources blocks).

```java
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;

File f = new File("/notes.txt");
boolean exists = f.exists();
boolean isFile = f.isFile();
long    size   = f.length();
boolean ok     = f.delete();

File dir = new File("/logs");
dir.mkdir();
new File("/logs/old.txt").renameTo(new File("/logs/new.txt"));
String[] names = dir.list();          // null when /logs is not a directory

// Append a line — write() throws IOException when the bytes cannot be stored
try (FileOutputStream out = new FileOutputStream("/logs/log.txt", /*append=*/true)) {
    out.write("hello\n".getBytes());
    out.flush();
} catch (IOException e) {
    Log.i("FS", "write failed: " + e.getMessage());
}

// Read it back
try (FileInputStream in = new FileInputStream(new File("/logs/log.txt"))) {
    byte[] buf = new byte[64];
    int n = in.read(buf);
    Log.i("FS", "read " + n + " bytes");
}
```

The read side reports failure the way `java.io.File`'s predicates do — `false`, `0`, or `-1` from `read()` — and the write side throws `java.io.IOException`: `FileOutputStream.write` and `File.createNewFile` declare it, and a refused path, a full volume or a rejected write arrives as one an app can catch. The stream constructors do not throw; a stream over a refused path fails at its first `write`.

| Class | Selected methods |
|-------|------------------|
| `File` | constructor `File(String path)`; `getPath()`, `getAbsolutePath()`, `getName()`, `getParent()`, `getParentFile()`, `exists()`, `isFile()`, `isDirectory()`, `length()`, `delete()`, `mkdir()`, `mkdirs()`, `createNewFile()` (throws `IOException`), `renameTo(File)`, `list()`, `listFiles()` |
| `FileInputStream` (extends `java.io.InputStream`) | constructors `(File)`, `(String path)`; `read()`, `read(byte[], int, int)`, `read(byte[])`, `available()`, `close()`, and `InputStream`'s `skip` |
| `FileOutputStream` (extends `java.io.OutputStream`) | constructors `(File)`, `(String)`, `(String, boolean append)`; `write(byte[], int, int)`, `write(byte[])`, `write(int)` (all throw `IOException`), `flush()`, `close()` |

`list()` returns the names in a directory in the filesystem's order, and `listFiles()` the same entries as `File`s under the directory's path; both return `null` for a path that is not a directory. `mkdirs()` creates the missing ancestors too and, as on Android, is `true` only when it created the directory — `false` when it already existed. `createNewFile()` is `false` when the path already exists. A `File` has the one constructor: build a child path by concatenation (`new File(dir.getPath() + "/" + name)`).

## `Context` — private files

`Context` (so every `Activity`, `Service` and `Application`) carries Android's private-file helpers, all relative to the app's own root:

```java
File data  = getDataDir();     // "/", the app's root
File files = getFilesDir();    // "/files", created on first use

try (FileOutputStream out = openFileOutput("state.bin", MODE_PRIVATE)) {   // MODE_APPEND to append
    out.write(bytes);
}
try (FileInputStream in = openFileInput("state.bin")) {                    // IOException when absent
    in.read(buf);
}
String[] mine = fileList();          // the names in /files
boolean gone  = deleteFile("state.bin");
```

| Method | Notes |
|---|---|
| `getDataDir()` | The app's root, `/`. |
| `getFilesDir()` | `/files`; created the first time it is asked for. |
| `openFileOutput(String name, int mode)` | `MODE_PRIVATE` truncates, `MODE_APPEND` appends; `name` is a bare name, never a path (`IllegalArgumentException`). Throws `IOException` when the file cannot be created — Android declares the subclass `FileNotFoundException`, which Picodroid does not serve. |
| `openFileInput(String name)` | Throws `IOException` when there is no such file (Android: `FileNotFoundException`). |
| `fileList()` | The names in `/files`; an empty array before the first file. |
| `deleteFile(String name)` | `false` when there was nothing to delete. |

## The reserve and the cap

On a multi-app board (`max_installed_apps` above 1) the framework bounds what an app may write, so no app can fill the volume or starve the others:

- **The system reserve** (`fs_system_reserve_kb`, default 64) is the tail of the volume no app may write into: a write that would leave less than that free throws `IOException("no space left on device")`. The system apps built into the firmware are exempt.
- **The per-app cap** (`app_data_cap_kb`, default a quarter of the volume — 128 KB on a 512 KB volume; `0` lifts it) bounds one app's directory. Past it a write throws `IOException("storage cap reached")` and `mkdir` answers `false`.

The accounting is LittleFS's own currency: a file costs its size rounded up to 4 KB blocks, and every directory costs an 8 KB metadata pair — the app's own `/data/<package>` included, so an empty app already holds 8 KB the moment it first writes. Reads, deletes and truncates are never refused, and a refused write leaves the file as it was. Both keys are board.toml settings ([porting guide](/reference/porting-guide/)). A single-app board keeps neither rule.

Data goes with its package: `pdb uninstall` and `PackageInstaller.uninstall` remove `/data/<package>`, and at boot every board sweeps any `/data/*` directory that names no installed or system package (an uninstall cut short by a power loss, an app removed by reflashing). On a single-app board the only package is the one flashed, so flashing a different app deletes the previous app's `/data/<package>` on the next boot. The simulator sweeps only when it models a device's directory (`sim.sh --system-apps`); a plain `sim.sh --app X` run installs X alone and keeps the other apps' data, so switching apps in the simulator loses nothing.

### `picodroid.os.StatFs`

Android's `StatFs`, over the one volume (the path is accepted and ignored):

```java
StatFs s = new StatFs("/");
long total = s.getTotalBytes();       // the volume (fs_kb)
long free  = s.getFreeBytes();        // unallocated, reserve included
long mine  = s.getAvailableBytes();   // what this app may still write: above the reserve, under its cap
```

`getBlockSizeLong()` is 4096; `getBlockCountLong()`, `getFreeBlocksLong()` and `getAvailableBlocksLong()` are the byte figures over it. On a single-app board `getAvailableBytes()` is simply the free space.

### `picodroid.app.usage.StorageStatsManager`

Per-package figures, multi-app boards only, from `getSystemService(Context.STORAGE_STATS_SERVICE)`:

```java
StorageStatsManager ssm = (StorageStatsManager) getSystemService(Context.STORAGE_STATS_SERVICE);
StorageStats st = ssm.queryStatsForPackage("com.example.weather");   // NameNotFoundException when absent
long app  = st.getAppBytes();    // the installed image: its run in the app region
long data = st.getDataBytes();   // its /data/<package>, by the cap's accounting
```

Picodroid has one volume and one user, so `queryStatsForPackage` takes the package name alone; `getCacheBytes()` is 0 (there is no cache directory).

What the two cost on a device: `queryStatsForPackage` counts a package's directory the first time it is asked (one filesystem listing per directory, a few milliseconds) and keeps the answer until that package runs, when its own writes are counted as they happen, or its directory is wiped. `StatFs` walks the volume for its used-block count (about 20 ms on an RP2350) only after something on it changed, so the total, the free and the available space on one screen cost one walk. A screen that lists every package should still ask from `Executors.backgroundExecutor()` and post the numbers back, as the settings app's Storage screen does.

## `picodroid.content.SharedPreferences`

Typed key-value settings store mirroring `android.content.SharedPreferences`. Backed by a CRC32-protected blob written atomically (tmp file + rename) into `/prefs/<name>` under the app's root.

Supported value types: `String`, `int`, `long`, `float`, `boolean`. Limits: 64 entries per file, 63-char keys, 1024-char string values, 4 KB total blob. A preferences name is 1 to 32 characters from `A-Z a-z 0-9 _ -`.

```java
import picodroid.content.Context;
import picodroid.content.SharedPreferences;
import picodroid.content.SharedPreferences.Editor;

SharedPreferences prefs = getSharedPreferences("settings", Context.MODE_PRIVATE);
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

| Class | Methods |
|-------|---------|
| `SharedPreferences` | `Context.getSharedPreferences(String name, int mode)` or `static open(String name)`; `contains(String)`, `getString(String, String def)`, `getInt(String, int def)`, `getLong(String, long def)`, `getFloat(String, float def)`, `getBoolean(String, boolean def)`, `getAll()`, `edit()` |
| `Editor` | `putString`, `putInt`, `putLong`, `putFloat`, `putBoolean` (each returns the `Editor` for chaining), `remove(String)`, `clear()`, `commit()` (writes the file before returning), `apply()` (changes memory at once, writes the file on a background thread) |

`Context.getSharedPreferences(name, mode)` is the Android idiom and is available on every `Activity`, `Service` and `Application`; `mode` is accepted for source compatibility, since every app's storage is private. As on Android, every call for one name returns the same instance, and that instance is safe to read and edit from any thread. `SharedPreferences.open(name)` is picodroid's own, for code that has no `Context` at hand, such as a `@Provides` method: each call reads the file afresh and returns a new, separate instance (two instances over one file do not see each other's changes until the file is read again).

`apply()` is safe on the main thread: the getters return the new values at once, and the file write goes to the framework's background pool; several applies in a row cost one write. A write still pending is finished when the Activity stops, where Android waits for its queued work, and before the file is next read. `commit()` writes in the calling thread and returns whether the write succeeded. When a write fails the new values stay in memory, as on Android, and the next write tries again.

An invalid name, a `null` or over-long key, a `null` or over-long string value, or a 65th entry throws `IllegalArgumentException`. An `Editor` records changes and applies them at `commit()`, a `clear()` first and the puts second whatever order they were called in, as on Android; after a commit the editor is empty and can be used again.

`getAll()` returns a fresh `Map<String, ?>` of every stored preference, values boxed as `String`, `Integer`, `Long`, `Float` or `Boolean` (Android's signature; mutating the returned map does not touch the store).

A write, from `commit()` or `apply()`, is atomic with respect to power loss: it writes to a `.tmp` file, verifies the size, and only then renames into place. At the per-app storage cap the `.tmp` copy has no room; a commit whose blob is no larger than the stored one — a shrink, a `remove`, a `clear()` — is then rewritten in place instead (not power-loss atomic), so an app can always free its own preferences. A corrupt blob (failed CRC32) is silently treated as empty the next time the file is read. An `Editor` belongs to the thread that made it.

---

**See also:** [core.md](/api/core/) (Java language) · [system.md](/api/system/) (logging, clock, threads) · [peripherals.md](/api/peripherals/) (GPIO, UART, I2C, SPI, PWM, ADC) · [networking.md](/api/networking/) (sockets) · [ui.md](/api/ui/) (display, widgets)
