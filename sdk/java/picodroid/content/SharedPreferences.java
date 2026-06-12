// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

/**
 * Typed key-value settings store mirroring {@code android.content.SharedPreferences}: obtain via
 * {@link Context#getSharedPreferences(String, int)} (or the picodroid-native {@link #open}), mutate
 * through {@link #edit()} / {@link Editor}. Not thread-safe; callers that share a SharedPreferences
 * instance across threads must synchronize externally.
 */
public final class SharedPreferences {
  private static final String TAG = "SharedPreferences";
  private static final String DIR = "/prefs";

  static final byte T_STRING = 1;
  static final byte T_INT = 2;
  static final byte T_LONG = 3;
  static final byte T_BOOL = 5;

  static final int MAX_ENTRIES = 64;
  static final int MAX_KEY_LEN = 63;
  static final int MAX_STRING_VAL = 1024;
  static final int MAX_BLOB = 4096;

  private static final int MAGIC = 0x50505246; // "PPRF" big-endian view
  private static final byte VERSION = 1;

  final String name;
  String[] keys = new String[MAX_ENTRIES];
  byte[] types = new byte[MAX_ENTRIES];
  String[] strVals = new String[MAX_ENTRIES];
  int[] intVals = new int[MAX_ENTRIES];
  // Long values split into two int halves to avoid requiring long[] array opcodes.
  int[] longValsLo = new int[MAX_ENTRIES];
  int[] longValsHi = new int[MAX_ENTRIES];
  int count;

  private SharedPreferences(String name) {
    this.name = name;
  }

  public static SharedPreferences open(String name) {
    if (!validName(name)) {
      throw new IllegalArgumentException("invalid preferences name");
    }
    SharedPreferences p = new SharedPreferences(name);
    p.load();
    return p;
  }

  public boolean contains(String key) {
    return indexOf(key) >= 0;
  }

  public String getString(String key, String def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_STRING) ? strVals[i] : def;
  }

  public int getInt(String key, int def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_INT) ? intVals[i] : def;
  }

  public long getLong(String key, long def) {
    int i = indexOf(key);
    if (i < 0 || types[i] != T_LONG) {
      return def;
    }
    return (((long) longValsHi[i]) << 32) | (((long) longValsLo[i]) & 0xffffffffL);
  }

  public boolean getBoolean(String key, boolean def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_BOOL) ? (intVals[i] != 0) : def;
  }

  public String[] getAllKeys() {
    String[] out = new String[count];
    for (int i = 0; i < count; i++) {
      out[i] = keys[i];
    }
    return out;
  }

  public Editor edit() {
    return new Editor(this);
  }

  int indexOf(String key) {
    for (int i = 0; i < count; i++) {
      if (keys[i].equals(key)) {
        return i;
      }
    }
    return -1;
  }

  static boolean validName(String name) {
    if (name == null) {
      return false;
    }
    int n = name.length();
    if (n == 0 || n > 32) {
      return false;
    }
    for (int i = 0; i < n; i++) {
      int c = name.charAt(i);
      boolean ok =
          (c >= 'a' && c <= 'z')
              || (c >= 'A' && c <= 'Z')
              || (c >= '0' && c <= '9')
              || c == '_'
              || c == '-';
      if (!ok) {
        return false;
      }
    }
    return true;
  }

  String path() {
    return DIR + "/" + name;
  }

  String tmpPath() {
    return DIR + "/" + name + ".tmp";
  }

  void load() {
    File f = new File(path());
    if (!f.exists()) {
      return;
    }
    long sz = f.length();
    if (sz <= 0 || sz > MAX_BLOB) {
      Log.i(TAG, "blob size out of range for " + name + "; using defaults");
      return;
    }
    int len = (int) sz;
    byte[] buf = new byte[len];
    FileInputStream in = new FileInputStream(f);
    int off = 0;
    while (off < len) {
      int n = in.read(buf, off, len - off);
      if (n <= 0) {
        break;
      }
      off += n;
    }
    in.close();
    if (off != len) {
      Log.i(TAG, "short read for " + name + "; using defaults");
      return;
    }
    if (!decode(buf, len)) {
      Log.i(TAG, "corrupt blob for " + name + "; using defaults");
      clearState();
    }
  }

  void clearState() {
    for (int i = 0; i < count; i++) {
      keys[i] = null;
      strVals[i] = null;
      intVals[i] = 0;
      longValsLo[i] = 0;
      longValsHi[i] = 0;
      types[i] = 0;
    }
    count = 0;
  }

  // ── decode ─────────────────────────────────────────────────────────────

  private boolean decode(byte[] buf, int len) {
    if (len < 12) { // 4 magic + 1 ver + 1 flags + 2 count + 4 crc
      return false;
    }
    int p = 0;
    int magic = readInt32(buf, p);
    p += 4;
    if (magic != MAGIC) {
      return false;
    }
    int version = buf[p] & 0xff;
    p += 1;
    int flags = buf[p] & 0xff;
    p += 1;
    if (version != VERSION || flags != 0) {
      return false;
    }
    int n = (buf[p] & 0xff) | ((buf[p + 1] & 0xff) << 8);
    p += 2;
    if (n > MAX_ENTRIES) {
      return false;
    }

    // Verify CRC32 over [0 .. len-4), stored as u32 LE trailer.
    int stored = readInt32LE(buf, len - 4);
    int computed = crc32(buf, 0, len - 4);
    if (stored != computed) {
      return false;
    }

    clearState();
    for (int i = 0; i < n; i++) {
      if (p >= len - 4) {
        return false;
      }
      int klen = buf[p] & 0xff;
      p += 1;
      if (klen == 0 || klen > MAX_KEY_LEN || p + klen > len - 4) {
        return false;
      }
      String key = bytesToString(buf, p, klen);
      p += klen;
      if (p + 1 > len - 4) {
        return false;
      }
      byte type = buf[p];
      p += 1;
      keys[i] = key;
      types[i] = type;
      if (type == T_STRING) {
        if (p + 2 > len - 4) {
          return false;
        }
        int vlen = (buf[p] & 0xff) | ((buf[p + 1] & 0xff) << 8);
        p += 2;
        if (vlen > MAX_STRING_VAL || p + vlen > len - 4) {
          return false;
        }
        strVals[i] = bytesToString(buf, p, vlen);
        p += vlen;
      } else if (type == T_INT || type == T_BOOL) {
        if (type == T_BOOL) {
          if (p + 1 > len - 4) {
            return false;
          }
          intVals[i] = buf[p] & 0xff;
          p += 1;
        } else {
          if (p + 4 > len - 4) {
            return false;
          }
          intVals[i] = readInt32LE(buf, p);
          p += 4;
        }
      } else if (type == T_LONG) {
        if (p + 8 > len - 4) {
          return false;
        }
        longValsLo[i] = readInt32LE(buf, p);
        longValsHi[i] = readInt32LE(buf, p + 4);
        p += 8;
      } else {
        return false;
      }
    }
    if (p != len - 4) {
      return false;
    }
    count = n;
    return true;
  }

  // ── encode (called by Editor.commit) ───────────────────────────────────

  int encode(byte[] out) {
    int p = 0;
    writeInt32(out, p, MAGIC);
    p += 4;
    out[p++] = VERSION;
    out[p++] = 0;
    out[p++] = (byte) (count & 0xff);
    out[p++] = (byte) ((count >> 8) & 0xff);

    for (int i = 0; i < count; i++) {
      String k = keys[i];
      int klen = k.length();
      out[p++] = (byte) klen;
      for (int j = 0; j < klen; j++) {
        out[p++] = (byte) k.charAt(j);
      }
      byte t = types[i];
      out[p++] = t;
      if (t == T_STRING) {
        String v = strVals[i];
        int vlen = v.length();
        out[p++] = (byte) (vlen & 0xff);
        out[p++] = (byte) ((vlen >> 8) & 0xff);
        for (int j = 0; j < vlen; j++) {
          out[p++] = (byte) v.charAt(j);
        }
      } else if (t == T_INT) {
        writeInt32LE(out, p, intVals[i]);
        p += 4;
      } else if (t == T_BOOL) {
        out[p++] = (byte) (intVals[i] != 0 ? 1 : 0);
      } else if (t == T_LONG) {
        writeInt32LE(out, p, longValsLo[i]);
        writeInt32LE(out, p + 4, longValsHi[i]);
        p += 8;
      }
    }
    int crc = crc32(out, 0, p);
    writeInt32LE(out, p, crc);
    p += 4;
    return p;
  }

  int serializedSize() {
    int n = 4 + 1 + 1 + 2 + 4; // header + trailer crc
    for (int i = 0; i < count; i++) {
      n += 1 + keys[i].length() + 1; // key_len + key + type
      byte t = types[i];
      if (t == T_STRING) {
        n += 2 + strVals[i].length();
      } else if (t == T_INT) {
        n += 4;
      } else if (t == T_BOOL) {
        n += 1;
      } else if (t == T_LONG) {
        n += 8;
      }
    }
    return n;
  }

  // ── encoding primitives ────────────────────────────────────────────────

  private static int readInt32(byte[] b, int p) {
    // Big-endian, used only for the ASCII magic check.
    return ((b[p] & 0xff) << 24)
        | ((b[p + 1] & 0xff) << 16)
        | ((b[p + 2] & 0xff) << 8)
        | (b[p + 3] & 0xff);
  }

  private static int readInt32LE(byte[] b, int p) {
    return (b[p] & 0xff)
        | ((b[p + 1] & 0xff) << 8)
        | ((b[p + 2] & 0xff) << 16)
        | ((b[p + 3] & 0xff) << 24);
  }

  private static void writeInt32(byte[] b, int p, int v) {
    b[p] = (byte) ((v >>> 24) & 0xff);
    b[p + 1] = (byte) ((v >>> 16) & 0xff);
    b[p + 2] = (byte) ((v >>> 8) & 0xff);
    b[p + 3] = (byte) (v & 0xff);
  }

  private static void writeInt32LE(byte[] b, int p, int v) {
    b[p] = (byte) (v & 0xff);
    b[p + 1] = (byte) ((v >>> 8) & 0xff);
    b[p + 2] = (byte) ((v >>> 16) & 0xff);
    b[p + 3] = (byte) ((v >>> 24) & 0xff);
  }

  private static String bytesToString(byte[] b, int off, int len) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < len; i++) {
      sb.append((char) (b[off + i] & 0xff));
    }
    return sb.toString();
  }

  static int crc32(byte[] buf, int off, int len) {
    int c = 0xffffffff;
    for (int i = 0; i < len; i++) {
      c ^= buf[off + i] & 0xff;
      for (int j = 0; j < 8; j++) {
        int mask = -(c & 1);
        c = (c >>> 1) ^ (0xedb88320 & mask);
      }
    }
    return ~c;
  }

  /** Pending mutations for a {@link SharedPreferences} instance. */
  public static final class Editor {
    private static final String TAG = "SharedPreferences";

    private final SharedPreferences base;

    // Pending state: copied from base on construction, mutated in place,
    // swapped onto base only after a successful commit().
    private String[] keys = new String[SharedPreferences.MAX_ENTRIES];
    private byte[] types = new byte[SharedPreferences.MAX_ENTRIES];
    private String[] strVals = new String[SharedPreferences.MAX_ENTRIES];
    private int[] intVals = new int[SharedPreferences.MAX_ENTRIES];
    private int[] longValsLo = new int[SharedPreferences.MAX_ENTRIES];
    private int[] longValsHi = new int[SharedPreferences.MAX_ENTRIES];
    private int count;

    Editor(SharedPreferences base) {
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
      if (value.length() > SharedPreferences.MAX_STRING_VAL) {
        throw new IllegalArgumentException("value too long");
      }
      int i = slot(key);
      types[i] = SharedPreferences.T_STRING;
      strVals[i] = value;
      return this;
    }

    public Editor putInt(String key, int value) {
      checkKey(key);
      int i = slot(key);
      types[i] = SharedPreferences.T_INT;
      intVals[i] = value;
      return this;
    }

    public Editor putLong(String key, long value) {
      checkKey(key);
      int i = slot(key);
      types[i] = SharedPreferences.T_LONG;
      longValsLo[i] = (int) value;
      longValsHi[i] = (int) (value >>> 32);
      return this;
    }

    public Editor putBoolean(String key, boolean value) {
      checkKey(key);
      int i = slot(key);
      types[i] = SharedPreferences.T_BOOL;
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
      if (need > SharedPreferences.MAX_BLOB) {
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

    /**
     * Commits the pending state like {@link #commit()}, discarding the result. Mirrors Android's
     * {@code SharedPreferences.Editor.apply()} signature; unlike Android the write happens
     * synchronously — LittleFS blobs are a few KB and picodroid's main loop is cooperative, so
     * there is no async persistence queue to hand off to.
     */
    public void apply() {
      commit();
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
      if (count >= SharedPreferences.MAX_ENTRIES) {
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
      if (n == 0 || n > SharedPreferences.MAX_KEY_LEN) {
        throw new IllegalArgumentException("key length out of range");
      }
    }
  }
}
