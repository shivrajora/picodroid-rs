// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import java.io.IOException;
import picodroid.app.AlarmManager;
import picodroid.app.NotificationManager;
import picodroid.app.usage.StorageStatsManager;
import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.content.pm.PackageManager;
import picodroid.content.res.Resources;
import picodroid.hardware.SensorManager;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.net.ConnectivityManager;
import picodroid.net.wifi.WifiManager;
import picodroid.os.IBinder;
import picodroid.os.PowerManager;

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
   * Name for {@link #getSystemService}: retrieves the {@link ConnectivityManager}, which tells an
   * app when the board's network comes and goes.
   */
  public static final String CONNECTIVITY_SERVICE = "connectivity";

  /**
   * Name for {@link #getSystemService}: retrieves the {@link WifiManager}, which scans for networks
   * and saves the one to join. On a board without WiFi its methods answer "none" — check {@code
   * getPackageManager().hasSystemFeature(PackageManager.FEATURE_WIFI)} first.
   */
  public static final String WIFI_SERVICE = "wifi";

  /**
   * Name for {@link #getSystemService}: retrieves the {@link PowerManager}, which says whether the
   * display is on.
   */
  public static final String POWER_SERVICE = "power";

  /**
   * Flag for {@link #registerReceiver(BroadcastReceiver, IntentFilter, int)}: other apps may send
   * to this receiver. Android's value; accepted and without effect here, where one app runs and a
   * broadcast never crosses apps.
   */
  public static final int RECEIVER_EXPORTED = 0x2;

  /** Flag for {@link #registerReceiver(BroadcastReceiver, IntentFilter, int)}: this app only. */
  public static final int RECEIVER_NOT_EXPORTED = 0x4;

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
    if (CONNECTIVITY_SERVICE.equals(name)) {
      return ConnectivityManager.getInstance();
    }
    if (WIFI_SERVICE.equals(name)) {
      return WifiManager.getInstance();
    }
    if (POWER_SERVICE.equals(name)) {
      return PowerManager.getInstance();
    }
    return null;
  }

  /**
   * Mirrors Android: the handle {@link picodroid.provider.Settings.System} takes. There are no
   * content providers here, so it is only that.
   */
  public ContentResolver getContentResolver() {
    return ContentResolver.INSTANCE;
  }

  /**
   * Mirrors Android: an {@link Executor} that runs its tasks on the main thread, the one that owns
   * the views. The same executor as {@link Executors#mainExecutor()}.
   */
  public Executor getMainExecutor() {
    return Executors.mainExecutor();
  }

  /** The package manager: what this device has installed and can launch. */
  public PackageManager getPackageManager() {
    return PackageManager.getInstance();
  }

  /**
   * Mirrors Android: this app's compiled {@code res/} tree — {@code
   * getResources().getString(R.string.app_name)}.
   */
  public Resources getResources() {
    return Resources.getInstance();
  }

  /**
   * Mirrors {@code android.content.Context#setTheme(int)}: makes the style {@code R.style.*} this
   * app's theme, so the framework's widgets take their default colours from it. Call it before any
   * view is made: in {@code onCreate}, before {@code setContentView}.
   *
   * <p>There is one theme per app, not one per context: it lives in {@link
   * picodroid.graphics.Theme}. A manifest that names the theme ({@code <application
   * android:theme="@style/AppTheme">}) has it applied before the first Activity's {@code onCreate},
   * so this call is for an app that picks a theme at run time. A layout's {@code ?attr/…}
   * references do not wait for either; they are resolved when the app is built, against the
   * manifest's theme, else the style named {@code AppTheme}.
   */
  public void setTheme(int resid) {
    getResources().applyTheme(resid);
  }

  /** Mirrors Android: shorthand for {@code getResources().getString(id)}. */
  public final String getString(int id) {
    return getResources().getString(id);
  }

  /**
   * Mirrors Android: the string {@code id} used as a format, filled with {@code formatArgs} as
   * {@code String.format} does.
   */
  public final String getString(int id, Object... formatArgs) {
    return getResources().getString(id, formatArgs);
  }

  /** Mirrors Android: shorthand for {@code getResources().getColor(id)}. */
  public final int getColor(int id) {
    return getResources().getColor(id);
  }

  /** This app's package name, as its manifest declares it. */
  public native String getPackageName();

  /**
   * Retrieve a {@link SharedPreferences} for the given name, the standard Android idiom: {@code
   * context.getSharedPreferences("settings", Context.MODE_PRIVATE).edit().putInt(...).apply()}.
   * Every call for one name returns the same instance, which is safe to use from any thread. {@code
   * mode} is accepted for source compatibility; only {@link #MODE_PRIVATE} semantics exist, every
   * app's storage being private.
   */
  public SharedPreferences getSharedPreferences(String name, int mode) {
    return SharedPreferences.getInstance(name);
  }

  /**
   * The file writes {@link SharedPreferences.Editor#apply} left to a background thread, as one
   * task; null until there has been one, so an app without preferences never loads the class.
   */
  static volatile Runnable sQueuedWork;

  /**
   * Finishes, in the calling thread, the file writes {@link SharedPreferences.Editor#apply} left to
   * a background thread. The framework calls it when an Activity stops, where Android waits for its
   * queued work, so what was applied on a screen is stored by the time the screen is gone.
   */
  protected final void finishQueuedWork() {
    Runnable work = sQueuedWork;
    if (work != null) {
      work.run();
    }
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
   * Start an Activity, mirroring {@code android.content.Context#startActivity}: from a {@code
   * BroadcastReceiver}'s Context, a Service or the Application as well as an Activity. The new
   * Activity is pushed above whatever is showing once the current callback returns; a {@link
   * Intent#setPackage} target launches that app instead (multi-app boards).
   */
  public native void startActivity(Intent intent);

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
   * Flag for {@link #bindService}: create the Service as long as the binding exists. The only
   * binding there is here, so the flag is accepted and changes nothing.
   */
  public static final int BIND_AUTO_CREATE = 0x0001;

  /**
   * Bind to a Service. Mirrors {@code android.content.Context#bindService(Intent,
   * ServiceConnection, int)}: calls {@code onCreate} (first time) and {@code onBind}, then delivers
   * the returned IBinder to {@code conn.onServiceConnected}. The binding is scoped to the calling
   * Activity (or to the Application when called outside an Activity). {@code flags} is accepted for
   * source compatibility; every binding behaves as {@link #BIND_AUTO_CREATE}.
   *
   * @return whether the Service was found and the bind is under way; when {@code false}, {@code
   *     conn} hears nothing
   */
  public boolean bindService(Intent service, ServiceConnection conn, int flags) {
    if (conn == null) {
      throw new IllegalArgumentException("connection is null");
    }
    return nativeBindService(service, conn);
  }

  private native boolean nativeBindService(Intent service, ServiceConnection conn);

  /**
   * Called by the framework with the binder a bind obtained. The call to {@code conn} is made here,
   * in bytecode, so a connection whose callbacks a base class declares is found as any interface
   * method is.
   */
  static void dispatchServiceConnected(ServiceConnection conn, Context service, IBinder binder) {
    conn.onServiceConnected(componentOf(service), binder);
  }

  private static ComponentName componentOf(Context service) {
    return service == null ? null : new ComponentName(service, service.getClass());
  }

  /** Drop a connection previously established via {@link #bindService}. */
  public final native void unbindService(ServiceConnection conn);

  /**
   * Set once any receiver has been registered this run, so an Activity's destroy touches the
   * registry only when there is one. Written by {@code BroadcastReceiver}, read by {@link
   * #unregisterLeakedReceivers}.
   */
  static boolean sReceiversRegistered;

  /**
   * Hear broadcasts whose action {@code filter} names, through {@code receiver.onReceive} on the
   * main thread, until {@link #unregisterReceiver}. Mirrors {@code
   * android.content.Context#registerReceiver(BroadcastReceiver, IntentFilter)}: the system actions
   * are {@link Intent#ACTION_TIME_TICK} and the others on {@link Intent}, {@code WifiManager}'s and
   * {@code ConnectivityManager}'s; an app's own actions come from {@link #sendBroadcast}. Register
   * from any thread.
   *
   * @return {@code null}: there are no sticky broadcasts to hand back
   * @throws IllegalArgumentException if {@code filter} is null or {@code receiver} is already
   *     registered
   * @throws IllegalStateException if {@link BroadcastReceiver#MAX_RECEIVERS} are registered
   */
  public Intent registerReceiver(BroadcastReceiver receiver, IntentFilter filter) {
    return BroadcastReceiver.register(this, receiver, filter);
  }

  /**
   * The form Android requires from API 34: {@code flags} is {@link #RECEIVER_EXPORTED} or {@link
   * #RECEIVER_NOT_EXPORTED}, accepted for source compatibility and without effect.
   */
  public Intent registerReceiver(BroadcastReceiver receiver, IntentFilter filter, int flags) {
    return registerReceiver(receiver, filter);
  }

  /**
   * Stop delivering to {@code receiver}. Nothing is delivered after this returns; a receiver may
   * unregister itself from inside {@code onReceive}.
   *
   * @throws IllegalArgumentException if {@code receiver} is null or not registered, as Android
   */
  public void unregisterReceiver(BroadcastReceiver receiver) {
    BroadcastReceiver.unregister(receiver);
  }

  /**
   * Deliver {@code intent} to this app's receivers: the ones registered for its action, or, for an
   * Intent that names a receiver class ({@code new Intent(this, MyReceiver.class)}), a new instance
   * of that class. Asynchronous, as on Android: the receivers run on the main thread after this
   * call returns, in a turn of their own. An Intent with neither an action nor a class reaches
   * nobody. One app runs at a time, so a broadcast never leaves the app ({@link #setPackage} is
   * refused), and the permission form {@code sendBroadcast(Intent, String)} is not here.
   *
   * @throws IllegalArgumentException if {@code intent} is null or targets another package
   */
  public void sendBroadcast(Intent intent) {
    if (intent == null) {
      throw new IllegalArgumentException("null Intent");
    }
    if (intent.getPackage() != null) {
      throw new IllegalArgumentException("a broadcast cannot target another package");
    }
    final Context context = this;
    Executors.mainExecutor().execute(() -> BroadcastReceiver.send(context, intent));
  }

  /**
   * The framework's half of an Activity's destroy: receivers this Context registered and never
   * unregistered are dropped, with Android's "has leaked IntentReceiver" warning. {@code
   * Activity.performDestroy} calls it after {@code onDestroy} returned.
   */
  protected final void unregisterLeakedReceivers() {
    if (sReceiversRegistered) {
      BroadcastReceiver.unregisterOwned(this);
    }
  }
}
