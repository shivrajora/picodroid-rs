// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

import java.util.LinkedHashSet;
import java.util.Set;

/**
 * A mapping from String keys to typed values. Mirrors {@code android.os.Bundle}: the carrier for
 * Intent extras ({@code Intent.putExtras}/{@code getExtras}) and for an Activity's saved instance
 * state ({@code onSaveInstanceState} → {@code onCreate(Bundle)}).
 *
 * <p>A typed getter whose key is absent, or holds a value of another type, returns the default (
 * {@code 0}, {@code false}, {@code null}, or the one passed in) rather than throwing — Android's
 * rule. Entries keep insertion order. There is no {@code Parcelable}/{@code Serializable}: values
 * are the primitives, String, their arrays, and nested Bundles.
 *
 * <pre>{@code
 * Bundle b = new Bundle();
 * b.putInt("count", 3);
 * b.putString("name", "pico");
 * int count = b.getInt("count");          // 3
 * long missing = b.getLong("nope", -1L);  // -1
 * }</pre>
 */
public final class Bundle {
  // Linear key/value table, allocated lazily: a Bundle holds a handful of entries, and a scan
  // beats a HashMap's per-entry nodes on this heap. Primitives are stored boxed, so the getters
  // type-check with instanceof and no tag array is needed.
  private String[] keys;
  private Object[] vals;
  private int n;

  /** An empty Bundle. */
  public Bundle() {}

  /** A Bundle holding a copy of {@code b}'s mappings (shallow: the values are shared). */
  public Bundle(Bundle b) {
    putAll(b);
  }

  public int size() {
    return n;
  }

  public boolean isEmpty() {
    return n == 0;
  }

  public void clear() {
    keys = null;
    vals = null;
    n = 0;
  }

  public boolean containsKey(String key) {
    return locate(key) >= 0;
  }

  /** The value mapped to {@code key} as stored (primitives boxed), or {@code null}. */
  public Object get(String key) {
    int i = locate(key);
    return i < 0 ? null : vals[i];
  }

  public void remove(String key) {
    int i = locate(key);
    if (i < 0) {
      return;
    }
    n--;
    for (; i < n; i++) {
      keys[i] = keys[i + 1];
      vals[i] = vals[i + 1];
    }
    keys[n] = null;
    vals[n] = null;
  }

  /** Copy every mapping of {@code b} into this Bundle, replacing keys already present. */
  public void putAll(Bundle b) {
    if (b == null) {
      return;
    }
    for (int i = 0; i < b.n; i++) {
      put(b.keys[i], b.vals[i]);
    }
  }

  /** The keys, in insertion order. A fresh set: changing it does not change the Bundle. */
  public Set<String> keySet() {
    Set<String> s = new LinkedHashSet<>();
    for (int i = 0; i < n; i++) {
      s.add(keys[i]);
    }
    return s;
  }

  public void putBoolean(String key, boolean value) {
    put(key, Boolean.valueOf(value));
  }

  public void putInt(String key, int value) {
    put(key, Integer.valueOf(value));
  }

  public void putLong(String key, long value) {
    put(key, Long.valueOf(value));
  }

  public void putFloat(String key, float value) {
    put(key, Float.valueOf(value));
  }

  public void putDouble(String key, double value) {
    put(key, Double.valueOf(value));
  }

  public void putString(String key, String value) {
    put(key, value);
  }

  public void putBundle(String key, Bundle value) {
    put(key, value);
  }

  public void putIntArray(String key, int[] value) {
    put(key, value);
  }

  public void putByteArray(String key, byte[] value) {
    put(key, value);
  }

  public void putStringArray(String key, String[] value) {
    put(key, value);
  }

  public boolean getBoolean(String key) {
    return getBoolean(key, false);
  }

  public boolean getBoolean(String key, boolean defaultValue) {
    Object o = get(key);
    return o instanceof Boolean ? ((Boolean) o).booleanValue() : defaultValue;
  }

  public int getInt(String key) {
    return getInt(key, 0);
  }

  public int getInt(String key, int defaultValue) {
    Object o = get(key);
    return o instanceof Integer ? ((Integer) o).intValue() : defaultValue;
  }

  public long getLong(String key) {
    return getLong(key, 0L);
  }

  public long getLong(String key, long defaultValue) {
    Object o = get(key);
    return o instanceof Long ? ((Long) o).longValue() : defaultValue;
  }

  public float getFloat(String key) {
    return getFloat(key, 0f);
  }

  public float getFloat(String key, float defaultValue) {
    Object o = get(key);
    return o instanceof Float ? ((Float) o).floatValue() : defaultValue;
  }

  public double getDouble(String key) {
    return getDouble(key, 0.0);
  }

  public double getDouble(String key, double defaultValue) {
    Object o = get(key);
    return o instanceof Double ? ((Double) o).doubleValue() : defaultValue;
  }

  public String getString(String key) {
    return getString(key, null);
  }

  public String getString(String key, String defaultValue) {
    Object o = get(key);
    return o instanceof String ? (String) o : defaultValue;
  }

  public Bundle getBundle(String key) {
    Object o = get(key);
    return o instanceof Bundle ? (Bundle) o : null;
  }

  public int[] getIntArray(String key) {
    Object o = get(key);
    return o instanceof int[] ? (int[]) o : null;
  }

  public byte[] getByteArray(String key) {
    Object o = get(key);
    return o instanceof byte[] ? (byte[]) o : null;
  }

  public String[] getStringArray(String key) {
    Object o = get(key);
    return o instanceof String[] ? (String[]) o : null;
  }

  private int locate(String key) {
    for (int i = 0; i < n; i++) {
      if (key == null ? keys[i] == null : key.equals(keys[i])) {
        return i;
      }
    }
    return -1;
  }

  private void put(String key, Object value) {
    int i = locate(key);
    if (i < 0) {
      if (keys == null || n == keys.length) {
        grow();
      }
      i = n++;
      keys[i] = key;
    }
    vals[i] = value;
  }

  private void grow() {
    int newCap = (keys == null) ? 4 : keys.length * 2;
    String[] nk = new String[newCap];
    Object[] nv = new Object[newCap];
    for (int i = 0; i < n; i++) {
      nk[i] = keys[i];
      nv[i] = vals[i];
    }
    keys = nk;
    vals = nv;
  }
}
