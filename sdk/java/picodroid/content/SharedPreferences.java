// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import java.util.zip.CRC32;
import picodroid.concurrent.Executors;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

/**
 * Typed key-value settings store mirroring {@code android.content.SharedPreferences}: obtain via
 * {@link Context#getSharedPreferences(String, int)}, mutate through {@link #edit()} / {@link
 * Editor}.
 *
 * <p>As on Android, {@code getSharedPreferences} hands every caller the one instance of a file, and
 * that instance is safe to read and edit from any thread. {@link Editor#apply()} changes what the
 * getters return at once and leaves the file write to a background thread; {@link Editor#commit()}
 * writes before it returns. A write still pending is finished when the Activity stops, as Android
 * finishes its queued work there, and before the file is read again.
 *
 * <p>{@link #open} is picodroid's own: a separate instance read from the file on every call.
 */
public final class SharedPreferences {
  private static final String TAG = "SharedPreferences";
  private static final String DIR = "/prefs";

  static final byte T_STRING = 1;
  static final byte T_INT = 2;
  static final byte T_LONG = 3;
  static final byte T_BOOL = 5;
  // 4 is Editor.T_REMOVED. Floats are stored as Float.floatToIntBits in intVals.
  static final byte T_FLOAT = 6;

  static final int MAX_ENTRIES = 64;
  static final int MAX_KEY_LEN = 63;
  static final int MAX_STRING_VAL = 1024;
  static final int MAX_BLOB = 4096;

  private static final int MAGIC = 0x50505246; // "PPRF" big-endian view
  private static final byte VERSION = 1;

  /** The instances {@link #getInstance} handed out, one per file; guarded by the class. */
  private static ArrayList<SharedPreferences> sInstances;

  /** Every instance holding state its file does not have yet; guarded by the class. */
  private static ArrayList<SharedPreferences> sUnwritten;

  final String name;

  // The state in memory, guarded by this. An Editor replaces the arrays whole.
  String[] keys = new String[MAX_ENTRIES];
  byte[] types = new byte[MAX_ENTRIES];
  String[] strVals = new String[MAX_ENTRIES];
  int[] intVals = new int[MAX_ENTRIES];
  // Long values split into two int halves to avoid requiring long[] array opcodes.
  int[] longValsLo = new int[MAX_ENTRIES];
  int[] longValsHi = new int[MAX_ENTRIES];
  int count;

  /** Counts the changes made in memory; guarded by this. */
  private int mMemoryGeneration;

  /** The {@link #mMemoryGeneration} the file holds; guarded by this. */
  private int mDiskGeneration;

  /** Held across a file write, so two writers cannot interleave. Taken before this. */
  private final Object mWriteLock = new Object();

  private SharedPreferences(String name) {
    this.name = name;
  }

  /**
   * A new instance over the file {@code name}, read from storage now. Not an Android method: an
   * Android app gets its preferences from {@link Context#getSharedPreferences}, which shares one
   * instance per file. Two instances over one file do not see each other's changes.
   */
  public static SharedPreferences open(String name) {
    if (!validName(name)) {
      throw new IllegalArgumentException("invalid preferences name");
    }
    flushPending(name);
    SharedPreferences p = new SharedPreferences(name);
    p.load();
    return p;
  }

  /** The one shared instance over the file {@code name}, as Android keeps per process. */
  static SharedPreferences getInstance(String name) {
    if (!validName(name)) {
      throw new IllegalArgumentException("invalid preferences name");
    }
    synchronized (SharedPreferences.class) {
      if (sInstances == null) {
        sInstances = new ArrayList<SharedPreferences>();
      }
      for (int i = 0; i < sInstances.size(); i++) {
        SharedPreferences p = sInstances.get(i);
        if (p.name.equals(name)) {
          return p;
        }
      }
    }
    // Read outside the class lock: it is file I/O. Two threads racing here both read the same
    // file; the first to register wins and the other's copy is dropped.
    SharedPreferences fresh = open(name);
    synchronized (SharedPreferences.class) {
      for (int i = 0; i < sInstances.size(); i++) {
        SharedPreferences p = sInstances.get(i);
        if (p.name.equals(name)) {
          return p;
        }
      }
      sInstances.add(fresh);
      return fresh;
    }
  }

  /**
   * Writes every preferences file whose {@link Editor#apply} has not reached storage yet, in the
   * calling thread: what {@link Context#finishQueuedWork} runs.
   */
  static void flushPending() {
    flushPending(null);
  }

  /**
   * Writes the unwritten instances over the file {@code name}, or all of them when null. Returns
   * once each one's file holds what its memory held when this was called, a write another thread
   * had under way included.
   */
  private static void flushPending(String name) {
    ArrayList<SharedPreferences> pending = null;
    synchronized (SharedPreferences.class) {
      if (sUnwritten != null) {
        for (int i = 0; i < sUnwritten.size(); i++) {
          SharedPreferences p = sUnwritten.get(i);
          if (name == null || p.name.equals(name)) {
            if (pending == null) {
              pending = new ArrayList<SharedPreferences>();
            }
            pending.add(p);
          }
        }
      }
    }
    if (pending != null) {
      for (int i = 0; i < pending.size(); i++) {
        pending.get(i).writeToDisk();
      }
    }
  }

  /** The state in memory moved on: remember this instance until its file has caught up. */
  void markUnwritten() {
    synchronized (SharedPreferences.class) {
      if (sUnwritten == null) {
        sUnwritten = new ArrayList<SharedPreferences>();
        Context.sQueuedWork = SharedPreferences::flushPending;
      }
      for (int i = 0; i < sUnwritten.size(); i++) {
        if (sUnwritten.get(i) == this) {
          return;
        }
      }
      sUnwritten.add(this);
    }
  }

  /**
   * Brings the file up to the state in memory, unless it already has it; any thread. Atomic: the
   * blob goes to a temporary file that is then renamed over the old one, so a power cut leaves the
   * old file or the new one. Returns false when the write failed; the state in memory stays, as on
   * Android, and the next write tries again.
   */
  boolean writeToDisk() {
    synchronized (mWriteLock) {
      byte[] blob = null;
      int generation;
      synchronized (this) {
        generation = mMemoryGeneration;
        if (generation != mDiskGeneration) {
          blob = new byte[serializedSize()];
          encode(blob);
        }
      }
      if (blob != null && !writeBlob(blob)) {
        return false; // still unwritten: the next write or flush tries again
      }
      synchronized (this) {
        mDiskGeneration = generation;
        if (generation == mMemoryGeneration) {
          // Only now, with the write lock held and no apply able to slip in between the
          // comparison and the removal: a reader that finds this instance gone from the
          // unwritten list may trust the file.
          synchronized (SharedPreferences.class) {
            if (sUnwritten != null) {
              sUnwritten.remove(this);
            }
          }
        }
      }
      return true;
    }
  }

  /** The background half of {@link Editor#apply}. */
  private void writeBehind() {
    writeToDisk();
  }

  /** Hands the file write to the framework's background pool. */
  void scheduleWrite() {
    // One task per apply and no "scheduled" flag: the pool drops a task when its queue is full,
    // and a flag would then hold every later write back. A task that finds the file current
    // returns at once, which is how several applies become one write.
    Executors.backgroundExecutor().execute(this::writeBehind);
  }

  private boolean writeBlob(byte[] blob) {
    int written = blob.length;
    // Make sure /prefs exists. mkdir returns false if it already exists;
    // we do not distinguish here.
    File dir = new File(DIR);
    if (!dir.exists()) {
      dir.mkdir();
    }

    String tmp = tmpPath();
    String finalPath = path();

    // Clear any stale tmp from a prior failed commit.
    File tmpFile = new File(tmp);
    if (tmpFile.exists()) {
      tmpFile.delete();
    }

    try {
      FileOutputStream out = new FileOutputStream(tmp);
      out.write(blob, 0, written);
      out.close();
    } catch (IOException e) {
      tmpFile.delete();
      // At the per-app storage cap the tmp file's own block is what is refused, so an
      // atomic commit could never shrink or clear a store that filled the last block.
      // A blob no larger than the file it replaces is rewritten in place instead: the
      // cap never refuses a truncating rewrite, and a power cut mid-write only leaves a
      // blob the CRC rejects on the next open, the same as a lost tmp would.
      if (written <= new File(finalPath).length() && writeInPlace(finalPath, blob, written)) {
        Log.i(TAG, "tmp write refused (" + e.getMessage() + "); rewrote in place");
        return true;
      }
      Log.i(TAG, "tmp write failed: " + e.getMessage());
      return false;
    }

    // Verify the write actually landed by comparing size.
    File tmpAfter = new File(tmp);
    if (tmpAfter.length() != (long) written) {
      tmpAfter.delete();
      Log.i(TAG, "tmp write short; aborting commit");
      return false;
    }

    if (!tmpAfter.renameTo(new File(finalPath))) {
      tmpAfter.delete();
      Log.i(TAG, "atomic rename failed");
      return false;
    }
    return true;
  }

  /** Truncate-and-rewrite `path`; {@code false} when the write or its size check fails. */
  private static boolean writeInPlace(String path, byte[] blob, int written) {
    try {
      FileOutputStream out = new FileOutputStream(path);
      out.write(blob, 0, written);
      out.close();
    } catch (IOException e) {
      return false;
    }
    return new File(path).length() == (long) written;
  }

  public synchronized boolean contains(String key) {
    return indexOf(key) >= 0;
  }

  public synchronized String getString(String key, String def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_STRING) ? strVals[i] : def;
  }

  public synchronized int getInt(String key, int def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_INT) ? intVals[i] : def;
  }

  public synchronized long getLong(String key, long def) {
    int i = indexOf(key);
    if (i < 0 || types[i] != T_LONG) {
      return def;
    }
    return (((long) longValsHi[i]) << 32) | (((long) longValsLo[i]) & 0xffffffffL);
  }

  public synchronized float getFloat(String key, float def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_FLOAT) ? Float.intBitsToFloat(intVals[i]) : def;
  }

  public synchronized boolean getBoolean(String key, boolean def) {
    int i = indexOf(key);
    return (i >= 0 && types[i] == T_BOOL) ? (intVals[i] != 0) : def;
  }

  /**
   * Every stored preference, as an unmodifiable-by-convention map from key to boxed value ({@code
   * String}, {@code Integer}, {@code Long} or {@code Boolean}). Mirrors {@code
   * android.content.SharedPreferences.getAll()}. The map is a fresh copy: mutating it does not
   * touch the stored preferences.
   */
  public synchronized Map<String, ?> getAll() {
    HashMap<String, Object> out = new HashMap<String, Object>();
    for (int i = 0; i < count; i++) {
      byte t = types[i];
      if (t == T_STRING) {
        out.put(keys[i], strVals[i]);
      } else if (t == T_INT) {
        out.put(keys[i], Integer.valueOf(intVals[i]));
      } else if (t == T_FLOAT) {
        out.put(keys[i], Float.valueOf(Float.intBitsToFloat(intVals[i])));
      } else if (t == T_BOOL) {
        out.put(keys[i], Boolean.valueOf(intVals[i] != 0));
      } else if (t == T_LONG) {
        long v = (((long) longValsHi[i]) << 32) | (((long) longValsLo[i]) & 0xffffffffL);
        out.put(keys[i], Long.valueOf(v));
      }
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
      } else if (type == T_INT || type == T_FLOAT || type == T_BOOL) {
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

  // ── encode (called by writeToDisk, holding this) ───────────────────────

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
      } else if (t == T_INT || t == T_FLOAT) {
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
      } else if (t == T_INT || t == T_FLOAT) {
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

  /**
   * The blob's trailer. A native step per array, not a Java loop per bit: verifying a 90-byte
   * preferences file cost the RP2350 some 40 ms of interpreted bytecode inside Service.onCreate
   * (claudeusage device QA, 2026-09-26), most of the one slow tick a boot had left.
   */
  static int crc32(byte[] buf, int off, int len) {
    CRC32 c = new CRC32();
    c.update(buf, off, len);
    return (int) c.getValue();
  }

  /**
   * Pending mutations for a {@link SharedPreferences} instance.
   *
   * <p>Mirrors Android's {@code EditorImpl}: the editor records a set of modifications (puts,
   * removes, and a {@code clear} flag) rather than a copy of the whole state. At {@link #commit}
   * the flag is applied first and the puts second — so {@code putString("a", "1").clear()} and
   * {@code clear().putString("a", "1")} both keep {@code a} — the merged state is published into
   * fresh arrays (a reused editor never aliases the live preferences), and the pending set is
   * emptied so the editor can be used again. An editor itself belongs to one thread.
   */
  public static final class Editor {
    private static final String TAG = "SharedPreferences";

    /** Pending-set marker for {@link #remove}. */
    private static final byte T_REMOVED = 4;

    private final SharedPreferences base;

    // Pending modifications since edit() / the last commit(): keys with a
    // type of T_REMOVED are removals, anything else is a put.
    private String[] keys = new String[SharedPreferences.MAX_ENTRIES];
    private byte[] types = new byte[SharedPreferences.MAX_ENTRIES];
    private String[] strVals = new String[SharedPreferences.MAX_ENTRIES];
    private int[] intVals = new int[SharedPreferences.MAX_ENTRIES];
    private int[] longValsLo = new int[SharedPreferences.MAX_ENTRIES];
    private int[] longValsHi = new int[SharedPreferences.MAX_ENTRIES];
    private int count;
    private boolean clearRequested;

    Editor(SharedPreferences base) {
      this.base = base;
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

    public Editor putFloat(String key, float value) {
      checkKey(key);
      int i = slot(key);
      types[i] = SharedPreferences.T_FLOAT;
      intVals[i] = Float.floatToIntBits(value);
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
      checkKey(key);
      int i = slot(key);
      types[i] = T_REMOVED;
      strVals[i] = null;
      return this;
    }

    public Editor clear() {
      clearRequested = true;
      return this;
    }

    /**
     * Applies the pending changes to the preferences in memory and writes the file before
     * returning. Returns false when the write failed; the changes stay in memory, as on Android.
     */
    public boolean commit() {
      commitToMemory();
      return base.writeToDisk();
    }

    /**
     * Applies the pending changes to the preferences in memory at once and leaves the file write to
     * a background thread, as {@code android.content.SharedPreferences.Editor#apply()} does: safe
     * to call on the main thread. Several applies in a row cost one write.
     */
    public void apply() {
      synchronized (base) {
        // Listed as unwritten in the same step that changes the memory, so no reader of the
        // file can come between the two.
        commitToMemory();
        base.markUnwritten();
      }
      base.scheduleWrite();
    }

    /** Merges the pending set into the base's state and empties it; any thread. */
    private void commitToMemory() {
      synchronized (base) {
        // Merge: base state (unless cleared), then the pending puts/removes,
        // into fresh arrays that the base will own outright.
        String[] nk = new String[SharedPreferences.MAX_ENTRIES];
        byte[] nt = new byte[SharedPreferences.MAX_ENTRIES];
        String[] ns = new String[SharedPreferences.MAX_ENTRIES];
        int[] ni = new int[SharedPreferences.MAX_ENTRIES];
        int[] nlo = new int[SharedPreferences.MAX_ENTRIES];
        int[] nhi = new int[SharedPreferences.MAX_ENTRIES];
        int nc = 0;
        if (!clearRequested) {
          nc = base.count;
          for (int i = 0; i < nc; i++) {
            nk[i] = base.keys[i];
            nt[i] = base.types[i];
            ns[i] = base.strVals[i];
            ni[i] = base.intVals[i];
            nlo[i] = base.longValsLo[i];
            nhi[i] = base.longValsHi[i];
          }
        }
        for (int p = 0; p < count; p++) {
          int i = -1;
          for (int j = 0; j < nc; j++) {
            if (nk[j].equals(keys[p])) {
              i = j;
              break;
            }
          }
          if (types[p] == T_REMOVED) {
            if (i >= 0) {
              int last = nc - 1;
              nk[i] = nk[last];
              nt[i] = nt[last];
              ns[i] = ns[last];
              ni[i] = ni[last];
              nlo[i] = nlo[last];
              nhi[i] = nhi[last];
              nk[last] = null;
              ns[last] = null;
              nc = last;
            }
            continue;
          }
          if (i < 0) {
            if (nc >= SharedPreferences.MAX_ENTRIES) {
              throw new IllegalArgumentException("preferences full");
            }
            i = nc;
            nk[i] = keys[p];
            nc = nc + 1;
          }
          nt[i] = types[p];
          ns[i] = strVals[p];
          ni[i] = intVals[p];
          nlo[i] = longValsLo[p];
          nhi[i] = longValsHi[p];
        }

        // Publish the merged state so serializedSize can measure it; put the old one back when
        // the result would not fit a file.
        String[] sk = base.keys;
        byte[] st = base.types;
        String[] ss = base.strVals;
        int[] si = base.intVals;
        int[] sll = base.longValsLo;
        int[] slh = base.longValsHi;
        int sc = base.count;

        base.keys = nk;
        base.types = nt;
        base.strVals = ns;
        base.intVals = ni;
        base.longValsLo = nlo;
        base.longValsHi = nhi;
        base.count = nc;

        if (base.serializedSize() > SharedPreferences.MAX_BLOB) {
          base.keys = sk;
          base.types = st;
          base.strVals = ss;
          base.intVals = si;
          base.longValsLo = sll;
          base.longValsHi = slh;
          base.count = sc;
          throw new IllegalArgumentException("preferences blob exceeds MAX_BLOB");
        }
        base.mMemoryGeneration++;

        // The editor starts a fresh pending set (Android's commitToMemory clears mModified /
        // mClear).
        for (int i = 0; i < count; i++) {
          keys[i] = null;
          strVals[i] = null;
          types[i] = 0;
        }
        count = 0;
        clearRequested = false;
      }
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
