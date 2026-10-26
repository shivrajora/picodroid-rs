// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.os.Bundle;

/**
 * Description of an operation to be performed: launch an Activity, start or bind a Service, or
 * broadcast to {@link BroadcastReceiver}s. An Intent identifies the target component by class, or
 * names an action, and optionally carries extras (a {@link Bundle}) that the recipient reads back.
 *
 * <p>Picodroid supports explicit Intents — a {@code Class<?>} target in this app, or on a multi-app
 * board a package name ({@link #setPackage}) that launches another installed app — and, for
 * broadcasts only, an action ({@link #setAction}) matched against registered {@link IntentFilter}s.
 * Resolving an action against a manifest to start an Activity or Service is out of scope.
 *
 * <pre>{@code
 * startActivity(new Intent(this, DetailActivity.class));
 * startService(new Intent(this, SyncService.class).putExtra("interval", 60));
 * startActivity(getPackageManager().getLaunchIntentForPackage("com.example.weather"));
 * sendBroadcast(new Intent("com.example.REFRESH").putExtra("why", "tap"));
 * }</pre>
 */
public final class Intent {
  /**
   * JVM-internal class name of the target component (e.g. "app/MyService"). Not {@code final}:
   * {@link #setClassName} writes it. The framework reads it by field slot, so it stays declared
   * first.
   */
  private String targetClassName;

  /** The extras, allocated on the first put. The typed {@code *Extra} methods are views of it. */
  private Bundle extras;

  /**
   * Package to launch instead of a class in this app (multi-app boards). The framework reads it by
   * field slot, after the fields above: keep it third.
   */
  private String packageName;

  /** The broadcast action ({@link #setAction}); null for a component Intent. Slot 3. */
  private String action;

  /** Android's action for the Activity a launcher starts; what a manifest's intent-filter names. */
  public static final String ACTION_MAIN = "android.intent.action.MAIN";

  /** Broadcast at every minute change, to registered receivers only. No extras. */
  public static final String ACTION_TIME_TICK = "android.intent.action.TIME_TICK";

  /**
   * Broadcast when the wall clock was set: by Settings, by an app's {@code
   * SystemClock.setCurrentTimeMillis}, or by the platform's time service when the network moved the
   * clock by two seconds or more. Android's constant value is {@code TIME_SET}.
   */
  public static final String ACTION_TIME_CHANGED = "android.intent.action.TIME_SET";

  /**
   * Broadcast when the platform zone changed (Settings → Date &amp; time, or {@code
   * AlarmManager.setTimeZone}). Carries {@link #EXTRA_TIMEZONE}.
   */
  public static final String ACTION_TIMEZONE_CHANGED = "android.intent.action.TIMEZONE_CHANGED";

  /**
   * The new zone's id ({@code "UTC"} or {@code "GMT+05:30"}) in {@link #ACTION_TIMEZONE_CHANGED}.
   */
  public static final String EXTRA_TIMEZONE = "time-zone";

  /**
   * Broadcast when the panel lit: a key, a touch, {@code setTurnScreenOn}, {@code KEYCODE_WAKEUP}.
   */
  public static final String ACTION_SCREEN_ON = "android.intent.action.SCREEN_ON";

  /**
   * Broadcast when the panel went dark: the screen timeout, {@code KEYCODE_SLEEP} or {@code POWER}.
   */
  public static final String ACTION_SCREEN_OFF = "android.intent.action.SCREEN_OFF";

  /**
   * Delivered once per boot to a receiver the manifest declares for it ({@code <receiver>} with
   * this action), before the boot app starts; never to a receiver registered at run time, which by
   * then is after boot. Multi-app boards.
   */
  public static final String ACTION_BOOT_COMPLETED = "android.intent.action.BOOT_COMPLETED";

  /** An Intent with no target yet; see {@link #setPackage}, {@link #setAction}. */
  public Intent() {
    this.targetClassName = null;
  }

  /**
   * Mirrors {@code android.content.Intent#Intent(String)}: a broadcast Intent for {@code action}.
   */
  public Intent(String action) {
    this.action = action;
  }

  /**
   * Mirrors {@code android.content.Intent#Intent(Context, Class)}: an Intent for the component
   * {@code cls} of the app {@code packageContext} belongs to, which is this app.
   */
  public Intent(Context packageContext, Class<?> cls) {
    // getName() returns the Java-spec dot-form; the native lifecycle ops
    // resolve classes by internal slash-form, so normalize here.
    this.targetClassName = cls.getName().replace('.', '/');
  }

  /** Internal-form class name (slash-separated), e.g. "app/MyService". */
  public String getTargetClassName() {
    return targetClassName;
  }

  /**
   * Target the named Activity class in this app, the explicit form of {@code
   * android.content.Intent#setClassName}. {@code className} may be given in either the Java
   * dot-form or the JVM internal slash-form. {@code packageName} is accepted for source
   * compatibility and must be {@code null} or this app's own package: an Intent cannot name a class
   * inside another app, whose classes are not loaded.
   */
  public Intent setClassName(String packageName, String className) {
    this.targetClassName = className == null ? null : className.replace('.', '/');
    return this;
  }

  /**
   * Launch the app installed as {@code packageName} (its main component) instead of a class in this
   * app. The current app is torn down first: one app runs at a time, and extras do not cross over.
   * {@code startActivity} throws {@link ActivityNotFoundException} when no such package is
   * installed. Mirrors {@code android.content.Intent#setPackage}.
   */
  public Intent setPackage(String packageName) {
    this.packageName = packageName;
    return this;
  }

  /** The package this Intent launches, or {@code null} for a class in this app. */
  public String getPackage() {
    return packageName;
  }

  /**
   * The action a broadcast of this Intent is matched on ({@link Context#sendBroadcast}, {@code
   * PendingIntent.getBroadcast}). Mirrors {@code android.content.Intent#setAction}; a {@code
   * startActivity} or {@code startService} ignores it.
   */
  public Intent setAction(String action) {
    this.action = action;
    return this;
  }

  /** The action, or {@code null} for an Intent that names a component only. */
  public String getAction() {
    return action;
  }

  public Intent putExtra(String key, int value) {
    extras().putInt(key, value);
    return this;
  }

  public Intent putExtra(String key, long value) {
    extras().putLong(key, value);
    return this;
  }

  public Intent putExtra(String key, String value) {
    extras().putString(key, value);
    return this;
  }

  public Intent putExtra(String key, boolean value) {
    extras().putBoolean(key, value);
    return this;
  }

  public Intent putExtra(String key, Bundle value) {
    extras().putBundle(key, value);
    return this;
  }

  /** Add every mapping of {@code extras} to this Intent's extras. */
  public Intent putExtras(Bundle extras) {
    extras().putAll(extras);
    return this;
  }

  /**
   * A copy of this Intent's extras, or {@code null} when it has none. Mirrors {@code
   * android.content.Intent#getExtras()}, the copy included: changing the returned Bundle does not
   * change the Intent.
   */
  public Bundle getExtras() {
    return extras == null ? null : new Bundle(extras);
  }

  public int getIntExtra(String key, int defaultValue) {
    return extras == null ? defaultValue : extras.getInt(key, defaultValue);
  }

  public long getLongExtra(String key, long defaultValue) {
    return extras == null ? defaultValue : extras.getLong(key, defaultValue);
  }

  public String getStringExtra(String key) {
    return extras == null ? null : extras.getString(key);
  }

  public boolean getBooleanExtra(String key, boolean defaultValue) {
    return extras == null ? defaultValue : extras.getBoolean(key, defaultValue);
  }

  public Bundle getBundleExtra(String key) {
    return extras == null ? null : extras.getBundle(key);
  }

  /**
   * Mirrors {@code android.content.Intent#getParcelableExtra}: an object extra, such as the {@code
   * SupplicantState} under {@code WifiManager.EXTRA_NEW_STATE}, or null.
   */
  public <T> T getParcelableExtra(String key) {
    return extras == null ? null : extras.getParcelable(key);
  }

  public boolean hasExtra(String key) {
    return extras != null && extras.containsKey(key);
  }

  private Bundle extras() {
    if (extras == null) {
      extras = new Bundle();
    }
    return extras;
  }
}
