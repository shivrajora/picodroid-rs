// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.os.Bundle;

/**
 * Description of an operation to be performed: launch an Activity, start or bind a Service. An
 * Intent identifies the target component by class and optionally carries extras (a {@link Bundle})
 * that the recipient reads back.
 *
 * <p>Picodroid supports only explicit Intents: a {@code Class<?>} target in this app, or on a
 * multi-app board a package name ({@link #setPackage}) that launches another installed app.
 * Implicit Intents (action / category / data resolution against a manifest) are out of scope.
 *
 * <pre>{@code
 * startActivity(new Intent(DetailActivity.class));
 * startService(new Intent(SyncService.class).putExtra("interval", 60));
 * startActivity(getPackageManager().getLaunchIntentForPackage("com.example.weather"));
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
   * Package to launch instead of a class in this app (multi-app boards). Declared last: the
   * framework reads it by field slot, after the fields above.
   */
  private String packageName;

  /** An Intent with no target yet; see {@link #setPackage}. */
  public Intent() {
    this.targetClassName = null;
  }

  public Intent(Class<?> targetClass) {
    // getName() returns the Java-spec dot-form; the native lifecycle ops
    // resolve classes by internal slash-form, so normalize here.
    this.targetClassName = targetClass.getName().replace('.', '/');
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
