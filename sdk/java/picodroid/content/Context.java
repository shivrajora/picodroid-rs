// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import java.io.IOException;
import picodroid.app.AlarmManager;
import picodroid.app.NotificationManager;
import picodroid.app.usage.StorageStatsManager;
import picodroid.content.pm.PackageManager;
import picodroid.hardware.SensorManager;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;

/**
 * Common base for {@code Application}, {@code Activity} and {@code Service}: provides
 * component-launch APIs ({@code startService}, {@code bindService}; {@code startActivity} lives on
 * Activity and Application) and system-service lookup ({@code getSystemService}).
 *
 * <p>Picodroid is single-process, so cross-process Intents and the {@code android:process}
 * attribute don't exist.
 */
public class Context {
  public static final String SENSOR_SERVICE = "sensor";

  /** Name for {@link #getSystemService}: retrieves the {@link NotificationManager}. */
  public static final String NOTIFICATION_SERVICE = "notification";

  /**
   * Name for {@link #getSystemService}: retrieves the {@link StorageStatsManager} (multi-app boards
   * only; a single-app board's framework has no such class).
   */
  public static final String STORAGE_STATS_SERVICE = "storagestats";

  /**
   * Name for {@link #getSystemService}: retrieves the {@link AlarmManager} (multi-app boards only;
   * a single-app board's framework has no such class).
   */
  public static final String ALARM_SERVICE = "alarm";

  /**
   * File-creation mode for {@link #getSharedPreferences} and {@link #openFileOutput}: accessible
   * only to this app, which on Picodroid every file is — the storage sandbox keeps each app inside
   * its own directory.
   */
  public static final int MODE_PRIVATE = 0;

  /**
   * File-creation mode for {@link #openFileOutput}: append to the file instead of truncating it.
   */
  public static final int MODE_APPEND = 32768;

  /** Where {@link #openFileOutput} and friends keep this app's files. */
  private static final String FILES_DIR = "/files";

  /**
   * Resolve a system service by name. Subclasses may extend; the base handles the well-known set.
   */
  public Object getSystemService(String name) {
    if (SENSOR_SERVICE.equals(name)) {
      return SensorManager.getInstance();
    }
    if (NOTIFICATION_SERVICE.equals(name)) {
      return NotificationManager.getInstance();
    }
    if (STORAGE_STATS_SERVICE.equals(name)) {
      return StorageStatsManager.getInstance();
    }
    if (ALARM_SERVICE.equals(name)) {
      return AlarmManager.getInstance();
    }
    return null;
  }

  /** The package manager: what this device has installed and can launch. */
  public PackageManager getPackageManager() {
    return PackageManager.getInstance();
  }

  /** This app's package name, as its manifest declares it. */
  public native String getPackageName();

  /**
   * Retrieve a {@link SharedPreferences} for the given name, the standard Android idiom: {@code
   * context.getSharedPreferences("settings", Context.MODE_PRIVATE).edit().putInt(...).apply()}.
   * {@code mode} is accepted for source compatibility; only {@link #MODE_PRIVATE} semantics exist,
   * every app's storage being private.
   */
  public SharedPreferences getSharedPreferences(String name, int mode) {
    return SharedPreferences.open(name);
  }

  /**
   * This app's private data directory, the root of everything it stores. Every path an app names —
   * {@code new File("/x")}, a {@link SharedPreferences} file, {@link #getFilesDir} — lives under
   * it, and no path can reach another app's directory: on Picodroid it is the app's own root,
   * {@code "/"}.
   */
  public File getDataDir() {
    return new File("/");
  }

  /** The directory {@link #openFileOutput} writes into, created the first time it is asked for. */
  public File getFilesDir() {
    File dir = new File(FILES_DIR);
    if (!dir.exists()) {
      dir.mkdir();
    }
    return dir;
  }

  /**
   * Open a private file for writing, creating it if needed; {@code mode} is {@link #MODE_PRIVATE}
   * (truncate) or {@link #MODE_APPEND}. Android declares {@code FileNotFoundException} here;
   * Picodroid throws its parent, {@link IOException}, when the file cannot be created.
   */
  public FileOutputStream openFileOutput(String name, int mode) throws IOException {
    String path = filePath(name);
    getFilesDir();
    return new FileOutputStream(path, (mode & MODE_APPEND) != 0);
  }

  /**
   * Open a private file for reading. Android declares {@code FileNotFoundException}; Picodroid
   * throws its parent, {@link IOException}, when there is no such file.
   */
  public FileInputStream openFileInput(String name) throws IOException {
    File f = new File(filePath(name));
    if (!f.isFile()) {
      throw new IOException("no such file: " + name);
    }
    return new FileInputStream(f);
  }

  /** The names of the files in {@link #getFilesDir}; empty when there are none yet. */
  public String[] fileList() {
    String[] names = new File(FILES_DIR).list();
    return names == null ? new String[0] : names;
  }

  /** Delete a private file; {@code false} when there was none. */
  public boolean deleteFile(String name) {
    return new File(filePath(name)).delete();
  }

  /** A private file's path: the name is a bare name, as on Android, never a path. */
  private static String filePath(String name) {
    if (name == null || name.length() == 0 || name.indexOf('/') >= 0) {
      throw new IllegalArgumentException("File " + name + " contains a path separator");
    }
    return FILES_DIR + "/" + name;
  }

  /**
   * Start a Service. Calls {@code onCreate} on first launch, then {@code onStartCommand} for every
   * call (including repeats). The framework owns instantiation; the target Service must have a
   * public no-arg constructor.
   */
  public final native void startService(Intent intent);

  /**
   * Stop a Service started via {@link #startService}. If the Service is also bound, it lives until
   * the last client unbinds; if neither, {@code onDestroy} runs immediately.
   */
  public final native void stopService(Intent intent);

  /**
   * Bind to a Service. Calls {@code onCreate} (first time) and {@code onBind}, then delivers the
   * returned IBinder to {@code conn.onServiceConnected}. The binding is scoped to the calling
   * Activity (or to the Application when called outside an Activity).
   */
  public final native void bindService(Intent intent, ServiceConnection conn);

  /** Drop a connection previously established via {@link #bindService}. */
  public final native void unbindService(ServiceConnection conn);
}
