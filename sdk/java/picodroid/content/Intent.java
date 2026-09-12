// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Description of an operation to be performed: launch an Activity, start or bind a Service. An
 * Intent identifies the target component by class and optionally carries primitive extras that the
 * recipient reads back.
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

  // Extras: linear key/value table, allocated lazily. tags[i] == 0 → int,
  // 1 → String, 2 → boolean. intVals[i] holds int / packed boolean; strVals[i]
  // holds String for tag 1 and is null otherwise.
  private String[] keys;
  private int[] intVals;
  private String[] strVals;
  private byte[] tags;
  private int n;

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
    int i = locateOrAppend(key);
    tags[i] = 0;
    intVals[i] = value;
    strVals[i] = null;
    return this;
  }

  public Intent putExtra(String key, String value) {
    int i = locateOrAppend(key);
    tags[i] = 1;
    strVals[i] = value;
    return this;
  }

  public Intent putExtra(String key, boolean value) {
    int i = locateOrAppend(key);
    tags[i] = 2;
    intVals[i] = value ? 1 : 0;
    strVals[i] = null;
    return this;
  }

  public int getIntExtra(String key, int defaultValue) {
    int i = locate(key);
    if (i < 0 || tags[i] != 0) {
      return defaultValue;
    }
    return intVals[i];
  }

  public String getStringExtra(String key) {
    int i = locate(key);
    if (i < 0 || tags[i] != 1) {
      return null;
    }
    return strVals[i];
  }

  public boolean getBooleanExtra(String key, boolean defaultValue) {
    int i = locate(key);
    if (i < 0 || tags[i] != 2) {
      return defaultValue;
    }
    return intVals[i] != 0;
  }

  public boolean hasExtra(String key) {
    return locate(key) >= 0;
  }

  /**
   * How many extras this Intent carries. Framework-internal, as {@link #getTargetClassName} is:
   * {@code picodroid.app.PendingIntent} flattens the table through these accessors, there being no
   * package-private route between {@code picodroid.content} and {@code picodroid.app}.
   */
  public int extraCount() {
    return n;
  }

  /** The key of extra {@code i}, where {@code i} is below {@link #extraCount}. */
  public String extraKey(int i) {
    return keys[i];
  }

  /**
   * Whether extra {@code i} holds an {@code int}, as opposed to a String or a boolean. A predicate
   * rather than a tag constant on purpose: this class may declare no more fields, static ones
   * included, because the framework addresses {@code packageName} by its slot and every field
   * declared here shifts it.
   */
  public boolean isIntExtra(int i) {
    return tags[i] == 0;
  }

  /** The value of extra {@code i}, as stored: an int, or a boolean packed as 0 or 1. */
  public int extraInt(int i) {
    return intVals[i];
  }

  private int locate(String key) {
    for (int i = 0; i < n; i++) {
      if (keys[i].equals(key)) {
        return i;
      }
    }
    return -1;
  }

  private int locateOrAppend(String key) {
    int i = locate(key);
    if (i >= 0) {
      return i;
    }
    if (keys == null || n == keys.length) {
      grow();
    }
    keys[n] = key;
    int idx = n;
    n++;
    return idx;
  }

  private void grow() {
    int newCap = (keys == null) ? 4 : keys.length * 2;
    String[] nk = new String[newCap];
    int[] niv = new int[newCap];
    String[] nsv = new String[newCap];
    byte[] nt = new byte[newCap];
    if (keys != null) {
      for (int i = 0; i < n; i++) {
        nk[i] = keys[i];
        niv[i] = intVals[i];
        nsv[i] = strVals[i];
        nt[i] = tags[i];
      }
    }
    keys = nk;
    intVals = niv;
    strVals = nsv;
    tags = nt;
  }
}
