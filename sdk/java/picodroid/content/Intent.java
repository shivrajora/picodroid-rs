// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Description of an operation to be performed: launch an Activity, start or bind a Service. An
 * Intent identifies the target component by class and optionally carries primitive extras that the
 * recipient reads back.
 *
 * <p>Picodroid supports only explicit Intents (a {@code Class<?>} target) — implicit Intents
 * (action / category / data resolution against a manifest) are out of scope.
 *
 * <pre>{@code
 * startActivity(new Intent(DetailActivity.class));
 * startService(new Intent(SyncService.class).putExtra("interval", 60));
 * }</pre>
 */
public final class Intent {
  /** JVM-internal class name of the target component (e.g. "app/MyService"). */
  private final String targetClassName;

  // Extras: linear key/value table, allocated lazily. tags[i] == 0 → int,
  // 1 → String, 2 → boolean. intVals[i] holds int / packed boolean; strVals[i]
  // holds String for tag 1 and is null otherwise.
  private String[] keys;
  private int[] intVals;
  private String[] strVals;
  private byte[] tags;
  private int n;

  public Intent(Class<?> targetClass) {
    // getName() returns the Java-spec dot-form; the native lifecycle ops
    // resolve classes by internal slash-form, so normalize here.
    this.targetClassName = targetClass.getName().replace('.', '/');
  }

  /** Internal-form class name (slash-separated), e.g. "app/MyService". */
  public String getTargetClassName() {
    return targetClassName;
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
