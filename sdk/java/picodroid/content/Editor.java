package picodroid.content;

import picodroid.io.File;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

/** Pending mutations for a {@link Preferences} instance. */
public final class Editor {
  private static final String TAG = "Preferences";

  private final Preferences base;

  // Pending state: copied from base on construction, mutated in place,
  // swapped onto base only after a successful commit().
  private String[] keys = new String[Preferences.MAX_ENTRIES];
  private byte[] types = new byte[Preferences.MAX_ENTRIES];
  private String[] strVals = new String[Preferences.MAX_ENTRIES];
  private int[] intVals = new int[Preferences.MAX_ENTRIES];
  private int[] longValsLo = new int[Preferences.MAX_ENTRIES];
  private int[] longValsHi = new int[Preferences.MAX_ENTRIES];
  private int count;

  Editor(Preferences base) {
    this.base = base;
    this.count = base.count;
    for (int i = 0; i < count; i++) {
      keys[i] = base.keys[i];
      types[i] = base.types[i];
      strVals[i] = base.strVals[i];
      intVals[i] = base.intVals[i];
      longValsLo[i] = base.longValsLo[i];
      longValsHi[i] = base.longValsHi[i];
    }
  }

  public Editor putString(String key, String value) {
    checkKey(key);
    if (value == null) {
      throw new IllegalArgumentException("value is null");
    }
    if (value.length() > Preferences.MAX_STRING_VAL) {
      throw new IllegalArgumentException("value too long");
    }
    int i = slot(key);
    types[i] = Preferences.T_STRING;
    strVals[i] = value;
    return this;
  }

  public Editor putInt(String key, int value) {
    checkKey(key);
    int i = slot(key);
    types[i] = Preferences.T_INT;
    intVals[i] = value;
    return this;
  }

  public Editor putLong(String key, long value) {
    checkKey(key);
    int i = slot(key);
    types[i] = Preferences.T_LONG;
    longValsLo[i] = (int) value;
    longValsHi[i] = (int) (value >>> 32);
    return this;
  }

  public Editor putBoolean(String key, boolean value) {
    checkKey(key);
    int i = slot(key);
    types[i] = Preferences.T_BOOL;
    intVals[i] = value ? 1 : 0;
    return this;
  }

  public Editor remove(String key) {
    int i = indexOf(key);
    if (i >= 0) {
      // swap-remove to keep the array dense
      int last = count - 1;
      if (i != last) {
        keys[i] = keys[last];
        types[i] = types[last];
        strVals[i] = strVals[last];
        intVals[i] = intVals[last];
        longValsLo[i] = longValsLo[last];
        longValsHi[i] = longValsHi[last];
      }
      keys[last] = null;
      strVals[last] = null;
      intVals[last] = 0;
      longValsLo[last] = 0;
      longValsHi[last] = 0;
      types[last] = 0;
      count--;
    }
    return this;
  }

  public Editor clear() {
    for (int i = 0; i < count; i++) {
      keys[i] = null;
      strVals[i] = null;
      intVals[i] = 0;
      longValsLo[i] = 0;
      longValsHi[i] = 0;
      types[i] = 0;
    }
    count = 0;
    return this;
  }

  /** Atomically writes the pending state to disk. Returns false on I/O failure. */
  public boolean commit() {
    // Publish editor state into the base instance first so serializedSize /
    // encode can reuse the base's serializer without duplicating logic.
    String[] sk = base.keys;
    byte[] st = base.types;
    String[] ss = base.strVals;
    int[] si = base.intVals;
    int[] sll = base.longValsLo;
    int[] slh = base.longValsHi;
    int sc = base.count;

    base.keys = keys;
    base.types = types;
    base.strVals = strVals;
    base.intVals = intVals;
    base.longValsLo = longValsLo;
    base.longValsHi = longValsHi;
    base.count = count;

    int need = base.serializedSize();
    if (need > Preferences.MAX_BLOB) {
      // Roll back the swap.
      base.keys = sk;
      base.types = st;
      base.strVals = ss;
      base.intVals = si;
      base.longValsLo = sll;
      base.longValsHi = slh;
      base.count = sc;
      throw new IllegalArgumentException("preferences blob exceeds MAX_BLOB");
    }

    byte[] blob = new byte[need];
    int written = base.encode(blob);
    if (written != need) {
      base.keys = sk;
      base.types = st;
      base.strVals = ss;
      base.intVals = si;
      base.longValsLo = sll;
      base.longValsHi = slh;
      base.count = sc;
      Log.i(TAG, "encoder size mismatch");
      return false;
    }

    // Make sure /prefs exists. mkdir returns false if it already exists;
    // we do not distinguish here.
    File dir = new File("/prefs");
    if (!dir.exists()) {
      dir.mkdir();
    }

    String tmp = base.tmpPath();
    String finalPath = base.path();

    // Clear any stale tmp from a prior failed commit.
    File tmpFile = new File(tmp);
    if (tmpFile.exists()) {
      tmpFile.delete();
    }

    FileOutputStream out = new FileOutputStream(tmp);
    out.write(blob, 0, written);
    out.close();

    // Verify the write actually landed by comparing size.
    File tmpAfter = new File(tmp);
    if (tmpAfter.length() != (long) written) {
      tmpAfter.delete();
      base.keys = sk;
      base.types = st;
      base.strVals = ss;
      base.intVals = si;
      base.longValsLo = sll;
      base.longValsHi = slh;
      base.count = sc;
      Log.i(TAG, "tmp write short; aborting commit");
      return false;
    }

    if (!tmpAfter.renameTo(new File(finalPath))) {
      tmpAfter.delete();
      base.keys = sk;
      base.types = st;
      base.strVals = ss;
      base.intVals = si;
      base.longValsLo = sll;
      base.longValsHi = slh;
      base.count = sc;
      Log.i(TAG, "atomic rename failed");
      return false;
    }
    return true;
  }

  private int indexOf(String key) {
    for (int i = 0; i < count; i++) {
      if (keys[i].equals(key)) {
        return i;
      }
    }
    return -1;
  }

  private int slot(String key) {
    int i = indexOf(key);
    if (i >= 0) {
      return i;
    }
    if (count >= Preferences.MAX_ENTRIES) {
      throw new IllegalArgumentException("preferences full");
    }
    int n = count;
    keys[n] = key;
    count = n + 1;
    return n;
  }

  private static void checkKey(String key) {
    if (key == null) {
      throw new IllegalArgumentException("key is null");
    }
    int n = key.length();
    if (n == 0 || n > Preferences.MAX_KEY_LEN) {
      throw new IllegalArgumentException("key length out of range");
    }
  }
}
