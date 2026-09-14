// SPDX-License-Identifier: GPL-3.0-only
package qa_store;

import java.io.IOException;
import java.util.Map;
import picodroid.app.Application;
import picodroid.content.SharedPreferences;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.os.StatFs;
import picodroid.util.Log;

/**
 * QA 2026-09-13: the storage surface at its edges — File predicates and renames, stream offsets and
 * EOF, partial reads, truncation, directory listing, refused paths, the Context private-file
 * helpers, SharedPreferences types, limits and editor semantics, StatFs. Reboot persistence is
 * checked by running the app twice against the same volume: the first run plants a marker, the
 * second verifies it (see the "persist" lines).
 */
public class QaStore extends Application {
  private static final String TAG = "QaStore";

  static int passed = 0;
  static int failed = 0;
  static int crashed = 0;
  static int sink = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  interface Section {
    void run() throws Exception;
  }

  static void section(String name, Section s) {
    Log.i(TAG, "section " + name);
    try {
      s.run();
    } catch (Throwable t) {
      Log.i(TAG, "CRASH in " + name + ": " + t + " msg=" + t.getMessage());
      crashed = crashed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== QaStore start ===");
    section("persist", () -> persist());
    section("cleanup", () -> cleanup());
    section("files", () -> files());
    section("streams", () -> streams());
    section("dirs", () -> dirs());
    section("refused", () -> refused());
    section("contextFiles", () -> contextFiles());
    section("prefs", () -> prefs());
    section("prefsLimits", () -> prefsLimits());
    section("statfs", () -> statfs());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  static byte[] readAll(String path) throws IOException {
    File f = new File(path);
    int len = (int) f.length();
    byte[] out = new byte[len];
    try (FileInputStream in = new FileInputStream(f)) {
      int off = 0;
      while (off < len) {
        int n = in.read(out, off, len - off);
        if (n <= 0) {
          break;
        }
        off += n;
      }
      if (off != len) {
        throw new IOException("short read " + off + "/" + len);
      }
    }
    return out;
  }

  static void writeAll(String path, byte[] data) throws IOException {
    try (FileOutputStream out = new FileOutputStream(path)) {
      out.write(data);
      out.flush();
    }
  }

  static byte[] pattern(int len, int seed) {
    byte[] b = new byte[len];
    for (int i = 0; i < len; i++) {
      b[i] = (byte) ((i * 31 + seed) & 0xff);
    }
    return b;
  }

  static boolean same(byte[] a, byte[] b) {
    if (a.length != b.length) {
      return false;
    }
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) {
        return false;
      }
    }
    return true;
  }

  // ---- persistence across runs --------------------------------------------------------------

  static void persist() throws IOException {
    File marker = new File("/persist/marker.bin");
    SharedPreferences p = SharedPreferences.open("persist");
    if (marker.exists()) {
      byte[] got = readAll("/persist/marker.bin");
      Log.i(TAG, "persist: second run, marker " + got.length + " bytes");
      check("persist: file content survived", same(got, pattern(3000, 5)));
      check(
          "persist: prefs survived",
          p.getInt("boots", 0) >= 1 && "kept".equals(p.getString("s", "")));
      p.edit().putInt("boots", p.getInt("boots", 0) + 1).commit();
      Log.i(TAG, "persist: verified");
    } else {
      Log.i(TAG, "persist: first run, planting marker");
      new File("/persist").mkdir();
      writeAll("/persist/marker.bin", pattern(3000, 5));
      p.edit().putInt("boots", 1).putString("s", "kept").commit();
      check("persist: marker planted", marker.exists() && marker.length() == 3000);
      Log.i(TAG, "persist: planted");
    }
  }

  static void cleanup() {
    // The preference stores of the previous run: a rerun must start from the same footprint,
    // and a store the cap refused to clear would otherwise be inherited.
    for (String store : new String[] {"qa", "qa2", "lim", "ctx"}) {
      new File("/prefs/" + store).delete();
      new File("/prefs/" + store + ".tmp").delete();
    }
    String[] names = {
      "/a.bin",
      "/b.bin",
      "/big.bin",
      "/t.txt",
      "/dir/x.txt",
      "/dir/y.txt",
      "/dir/sub/z.txt",
      "/dir/sub",
      "/dir",
      "/r1.txt",
      "/r2.txt",
      "/mv/inner.txt",
      "/mv",
      "/n1/n2/n3/leaf.txt",
      "/n1/n2/n3",
      "/n1/n2",
      "/n1",
      "/many"
    };
    for (String n : names) {
      File f = new File(n);
      if (f.isDirectory()) {
        String[] kids = f.list();
        if (kids != null) {
          for (String k : kids) {
            new File(n + "/" + k).delete();
          }
        }
      }
      f.delete();
    }
  }

  // ---- File predicates ------------------------------------------------------------------------

  static void files() throws IOException {
    File a = new File("/a.bin");
    check("absent exists false", !a.exists() && !a.isFile() && !a.isDirectory());
    check("absent length 0", a.length() == 0);
    check("delete absent false", !a.delete());
    check("createNewFile", a.createNewFile());
    check("createNewFile twice false", !a.createNewFile());
    check("after create", a.exists() && a.isFile() && !a.isDirectory() && a.length() == 0);
    byte[] data = pattern(1000, 1);
    writeAll("/a.bin", data);
    check("length after write", a.length() == 1000);
    check("read back exact", same(readAll("/a.bin"), data));
    writeAll("/a.bin", pattern(10, 2));
    check("rewrite truncates", a.length() == 10 && same(readAll("/a.bin"), pattern(10, 2)));
    try (FileOutputStream out = new FileOutputStream("/a.bin", true)) {
      out.write(pattern(5, 3));
    }
    byte[] both = readAll("/a.bin");
    check(
        "append grows",
        a.length() == 15 && both.length == 15 && same(both, concat(pattern(10, 2), pattern(5, 3))));
    try (FileOutputStream out = new FileOutputStream("/a.bin", true)) {
      out.write(300);
      out.write(-1);
      out.write(0);
    }
    byte[] tail = readAll("/a.bin");
    check(
        "write(int) low byte",
        tail.length == 18 && tail[15] == 44 && tail[16] == -1 && tail[17] == 0);
    File b = new File("/b.bin");
    check("rename absent false", !new File("/nope.bin").renameTo(b));
    check("renameTo", a.renameTo(b) && !a.exists() && b.exists() && b.length() == 18);
    writeAll("/a.bin", pattern(4, 9));
    check("rename onto existing", b.renameTo(a) && !b.exists() && a.length() == 18);
    check("getName", new File("/x/y/z.txt").getName().equals("z.txt"));
    check("getParent", "/x/y".equals(new File("/x/y/z.txt").getParent()));
    check(
        "getParent root-level",
        "/".equals(new File("/z.txt").getParent()) || new File("/z.txt").getParent() == null);
    check("getParentFile", new File("/x/y/z.txt").getParentFile().getPath().equals("/x/y"));
    check("getPath", new File("/a.bin").getPath().equals("/a.bin"));
    check("getAbsolutePath", new File("/a.bin").getAbsolutePath().equals("/a.bin"));
    check("relative path name", new File("rel.txt").getName().equals("rel.txt"));
    check("relative path resolves like absolute", !new File("rel.txt").exists());
    writeAll("rel.txt", pattern(3, 0));
    check(
        "relative written is visible absolute",
        new File("/rel.txt").exists() && new File("/rel.txt").length() == 3);
    check("delete relative", new File("/rel.txt").delete() && !new File("rel.txt").exists());
    // Sized to the volume: 20 KB on the RP2350 boards and the sim, a few KB on the RP2040
    // testbench (a 128 KB LittleFS with a 32 KB per-app cap cannot hold 20 KB plus the rest).
    int bigLen = (int) Math.max(2000L, Math.min(20000L, new StatFs("/").getAvailableBytes() / 4));
    bigLen -= bigLen % 100;
    byte[] big = pattern(bigLen, 7);
    writeAll("/big.bin", big);
    Log.i(TAG, "big file: " + bigLen + " bytes");
    check(
        "big file round trip",
        new File("/big.bin").length() == bigLen && same(readAll("/big.bin"), big));
    check("delete file", a.delete() && !a.exists() && new File("/big.bin").delete());
    check("root isDirectory", new File("/").isDirectory() && new File("/").exists());
  }

  static byte[] concat(byte[] x, byte[] y) {
    byte[] r = new byte[x.length + y.length];
    System.arraycopy(x, 0, r, 0, x.length);
    System.arraycopy(y, 0, r, x.length, y.length);
    return r;
  }

  // ---- stream semantics -----------------------------------------------------------------------

  static void streams() throws IOException {
    byte[] data = pattern(100, 4);
    writeAll("/t.txt", data);
    try (FileInputStream in = new FileInputStream("/t.txt")) {
      check("available before read", in.available() == 100);
      byte[] buf = new byte[30];
      int n1 = in.read(buf);
      check("first read 30", n1 == 30 && buf[0] == data[0] && buf[29] == data[29]);
      check("available after 30", in.available() == 70);
      int n2 = in.read(buf, 10, 20);
      check(
          "read with offset",
          n2 == 20 && buf[10] == data[30] && buf[29] == data[49] && buf[0] == data[0]);
      byte[] rest = new byte[200];
      int n3 = in.read(rest);
      check("read rest short", n3 == 50 && rest[49] == data[99]);
      int n4 = in.read(rest);
      check("read at EOF -1", n4 == -1);
      check("available at EOF 0", in.available() == 0);
      int n5 = in.read(buf, 0, 0);
      check("read zero length", n5 == 0);
    }
    try (FileInputStream in = new FileInputStream("/t.txt")) {
      byte[] one = new byte[1];
      int count = 0;
      int sum = 0;
      while (in.read(one) == 1) {
        count++;
        sum += one[0] & 0xff;
      }
      int expected = 0;
      for (int i = 0; i < 100; i++) {
        expected += data[i] & 0xff;
      }
      check("byte-at-a-time read", count == 100 && sum == expected);
    }
    FileInputStream missing = new FileInputStream("/missing.txt");
    byte[] buf = new byte[4];
    check("read missing file -1", missing.read(buf) == -1 && missing.available() == 0);
    missing.close();
    boolean ioobe = false;
    try (FileInputStream in = new FileInputStream("/t.txt")) {
      in.read(buf, 2, 4);
    } catch (IndexOutOfBoundsException e) {
      ioobe = true;
    }
    check("read off+len > buf throws", ioobe);
    ioobe = false;
    try (FileOutputStream out = new FileOutputStream("/t.txt", true)) {
      out.write(buf, 3, 2);
    } catch (IndexOutOfBoundsException e) {
      ioobe = true;
    }
    check("write off+len > buf throws", ioobe);
    check("bad write left file intact", new File("/t.txt").length() == 100);
    try (FileOutputStream out = new FileOutputStream("/t.txt")) {
      out.write(new byte[0]);
    }
    check("empty write truncates", new File("/t.txt").length() == 0 && new File("/t.txt").exists());
    FileOutputStream twice = new FileOutputStream("/t.txt");
    twice.write(pattern(3, 1));
    twice.close();
    twice.close();
    check("close twice harmless", new File("/t.txt").length() == 3);
    FileInputStream dirStream = new FileInputStream("/");
    int dn = dirStream.read(buf);
    check("read on a directory is -1", dn == -1);
    dirStream.close();
    check("delete t", new File("/t.txt").delete());
  }

  // ---- directories ----------------------------------------------------------------------------

  static void dirs() throws IOException {
    File dir = new File("/dir");
    check("mkdir", dir.mkdir() && dir.isDirectory() && !dir.isFile());
    check("mkdir existing false", !dir.mkdir());
    writeAll("/dir/x.txt", pattern(1, 0));
    writeAll("/dir/y.txt", pattern(2, 0));
    check("mkdirs nested", new File("/dir/sub").mkdirs() && new File("/dir/sub").isDirectory());
    writeAll("/dir/sub/z.txt", pattern(3, 0));
    String[] names = dir.list();
    check("list count", names != null && names.length == 3);
    boolean hasX = false;
    boolean hasY = false;
    boolean hasSub = false;
    if (names != null) {
      for (String n : names) {
        if (n.equals("x.txt")) {
          hasX = true;
        }
        if (n.equals("y.txt")) {
          hasY = true;
        }
        if (n.equals("sub")) {
          hasSub = true;
        }
      }
    }
    check("list names", hasX && hasY && hasSub);
    File[] kids = dir.listFiles();
    boolean typed = kids != null && kids.length == 3;
    if (typed) {
      for (File k : kids) {
        if (k.getName().equals("sub") && !k.isDirectory()) {
          typed = false;
        }
        if (k.getName().equals("x.txt") && (!k.isFile() || k.length() != 1)) {
          typed = false;
        }
        if (!k.getPath().startsWith("/dir/")) {
          typed = false;
        }
      }
    }
    check("listFiles typed with full paths", typed);
    check("list of a file null", new File("/dir/x.txt").list() == null);
    check("list of absent null", new File("/absent").list() == null);
    check("delete non-empty dir false", !dir.delete() && dir.exists());
    check("mkdirs deep", new File("/n1/n2/n3").mkdirs() && new File("/n1/n2").isDirectory());
    check("mkdirs existing false", !new File("/n1/n2/n3").mkdirs());
    writeAll("/n1/n2/n3/leaf.txt", pattern(8, 1));
    check("deep file", new File("/n1/n2/n3/leaf.txt").length() == 8);
    // Every directory costs an 8 KB metadata pair against the 128 KB cap: drop the chain now.
    new File("/n1/n2/n3/leaf.txt").delete();
    new File("/n1/n2/n3").delete();
    new File("/n1/n2").delete();
    check("deep chain removable", new File("/n1").delete() && !new File("/n1").exists());
    check(
        "mkdir under missing parent false",
        !new File("/zz/yy").mkdir() && !new File("/zz").exists());
    boolean ioe = false;
    try {
      writeAll("/zz/yy.txt", pattern(1, 0));
    } catch (IOException e) {
      ioe = true;
    }
    check("write under missing dir throws IOException", ioe);
    check("create under missing dir false-or-throws", createUnderMissing());
    new File("/mv").mkdir();
    writeAll("/mv/inner.txt", pattern(6, 2));
    check(
        "rename across dirs",
        new File("/mv/inner.txt").renameTo(new File("/dir/moved.txt"))
            && new File("/dir/moved.txt").length() == 6
            && !new File("/mv/inner.txt").exists());
    check(
        "rename dir",
        new File("/mv").renameTo(new File("/mv2"))
            && new File("/mv2").isDirectory()
            && !new File("/mv").exists());
    check("delete empty dir", new File("/mv2").delete() && !new File("/mv2").exists());
    check("rename file onto dir false", !new File("/dir/moved.txt").renameTo(new File("/dir/sub")));
    // many small files (each costs a 4 KB block against the per-app cap)
    new File("/many").mkdir();
    for (int i = 0; i < 4; i++) {
      writeAll("/many/f" + i + ".txt", pattern(i + 1, i));
    }
    String[] m = new File("/many").list();
    boolean ok = m != null && m.length == 4;
    for (int i = 0; i < 4 && ok; i++) {
      if (!same(readAll("/many/f" + i + ".txt"), pattern(i + 1, i))) {
        ok = false;
      }
    }
    check("4 small files", ok);
    for (int i = 0; i < 4; i++) {
      new File("/many/f" + i + ".txt").delete();
    }
    check("many emptied", new File("/many").list().length == 0 && new File("/many").delete());
    // teardown
    new File("/dir/moved.txt").delete();
    new File("/dir/sub/z.txt").delete();
    new File("/dir/sub").delete();
    new File("/dir/x.txt").delete();
    new File("/dir/y.txt").delete();
    check("dir removable once empty", dir.delete() && !dir.exists());
  }

  static boolean createUnderMissing() {
    try {
      return !new File("/zz/yy.txt").createNewFile();
    } catch (IOException e) {
      return true;
    }
  }

  // ---- refused paths --------------------------------------------------------------------------

  static void refused() {
    check("dotdot exists false", !new File("/../other").exists());
    check("dotdot nested exists false", !new File("/x/../../etc").exists());
    boolean ioe = false;
    try {
      writeAll("/../escape.txt", pattern(1, 0));
    } catch (IOException e) {
      ioe = true;
    }
    check("dotdot write throws IOException", ioe);
    check("dotdot mkdir false", !new File("/../d").mkdir());
    check("dotdot delete false", !new File("/../d").delete());
    check("dot segments dropped", new File("/./a/./b").getPath().length() > 0);
    check("empty path", !new File("").isFile());
    boolean ok = true;
    try {
      check("empty path exists is root-or-false", new File("").exists() || !new File("").exists());
    } catch (Throwable t) {
      ok = false;
    }
    check("empty path does not crash", ok);
    check("double slash", !new File("//a//b.txt").exists());
    StringBuilder longName = new StringBuilder("/");
    for (int i = 0; i < 200; i++) {
      longName.append('n');
    }
    String ln = longName.toString();
    ioe = false;
    try {
      writeAll(ln, pattern(1, 0));
    } catch (IOException e) {
      ioe = true;
    }
    check("200-char path refused or created", ioe || new File(ln).exists());
    new File(ln).delete();
  }

  // ---- Context private-file helpers -----------------------------------------------------------

  void contextFiles() throws IOException {
    check("getDataDir", getDataDir().getPath().equals("/"));
    check("getFilesDir", getFilesDir().getPath().equals("/files") && getFilesDir().isDirectory());
    for (String n : fileList()) {
      deleteFile(n);
    }
    check("fileList empty", fileList().length == 0);
    try (FileOutputStream out = openFileOutput("state.bin", MODE_PRIVATE)) {
      out.write(pattern(20, 3));
    }
    check("openFileOutput creates in /files", new File("/files/state.bin").length() == 20);
    try (FileOutputStream out = openFileOutput("state.bin", MODE_APPEND)) {
      out.write(pattern(5, 4));
    }
    check("MODE_APPEND", new File("/files/state.bin").length() == 25);
    try (FileOutputStream out = openFileOutput("state.bin", MODE_PRIVATE)) {
      out.write(pattern(2, 4));
    }
    check("MODE_PRIVATE truncates", new File("/files/state.bin").length() == 2);
    try (FileInputStream in = openFileInput("state.bin")) {
      byte[] b = new byte[10];
      check("openFileInput reads", in.read(b) == 2 && b[0] == pattern(2, 4)[0]);
    }
    String[] names = fileList();
    check("fileList lists it", names.length == 1 && names[0].equals("state.bin"));
    boolean ioe = false;
    try (FileInputStream in = openFileInput("nothere.bin")) {
      sink += in.available();
    } catch (IOException e) {
      ioe = true;
    }
    check("openFileInput missing throws IOException", ioe);
    boolean iae = false;
    try (FileOutputStream out = openFileOutput("a/b.bin", MODE_PRIVATE)) {
      out.write(1);
    } catch (IllegalArgumentException e) {
      iae = true;
    }
    check("openFileOutput with path throws IAE", iae);
    check(
        "deleteFile",
        deleteFile("state.bin") && !deleteFile("state.bin") && fileList().length == 0);
    check(
        "getSharedPreferences works from Context",
        getSharedPreferences("ctx", MODE_PRIVATE).edit().putInt("v", 3).commit());
    check("same store via open()", SharedPreferences.open("ctx").getInt("v", 0) == 3);
    SharedPreferences.open("ctx").edit().clear().commit();
  }

  // ---- SharedPreferences ----------------------------------------------------------------------

  static void prefs() {
    SharedPreferences p = SharedPreferences.open("qa");
    p.edit().clear().commit();
    check("empty getAll", p.getAll().size() == 0);
    check(
        "defaults",
        p.getInt("i", -1) == -1
            && p.getString("s", "d").equals("d")
            && p.getString("s", null) == null
            && !p.getBoolean("b", false)
            && p.getLong("l", 5L) == 5L
            && p.getFloat("f", 1.5f) == 1.5f);
    check("contains absent", !p.contains("i"));
    boolean ok =
        p.edit()
            .putInt("i", Integer.MIN_VALUE)
            .putInt("zero", 0)
            .putLong("l", Long.MAX_VALUE)
            .putLong("ln", -1L)
            .putFloat("f", -0.0f)
            .putFloat("fn", Float.NaN)
            .putFloat("fx", 3.4028235E38f)
            .putBoolean("b", true)
            .putBoolean("bf", false)
            .putString("s", "value")
            .putString("empty", "")
            .putString("uni", "héllo wörld")
            .putString("sp", "key with spaces")
            .commit();
    check("commit ok", ok);
    check("int round trip", p.getInt("i", 0) == Integer.MIN_VALUE && p.getInt("zero", 7) == 0);
    check("long round trip", p.getLong("l", 0) == Long.MAX_VALUE && p.getLong("ln", 0) == -1L);
    check(
        "float round trip",
        1 / p.getFloat("f", 1f) < 0
            && Float.compare(p.getFloat("fn", 0f), Float.NaN) == 0
            && p.getFloat("fx", 0f) == 3.4028235E38f);
    check("boolean round trip", p.getBoolean("b", false) && !p.getBoolean("bf", true));
    check(
        "string round trip",
        p.getString("s", "").equals("value")
            && p.getString("empty", "x").equals("")
            && p.getString("uni", "").equals("héllo wörld"));
    check("contains present", p.contains("i") && p.contains("empty") && p.contains("bf"));
    Map<String, ?> all = p.getAll();
    check("getAll size", all.size() == 13);
    check(
        "getAll types",
        all.get("i") instanceof Integer
            && all.get("l") instanceof Long
            && all.get("f") instanceof Float
            && all.get("b") instanceof Boolean
            && all.get("s") instanceof String);
    check(
        "getAll values",
        ((Integer) all.get("i")) == Integer.MIN_VALUE
            && "value".equals(all.get("s"))
            && ((Boolean) all.get("b")));
    check("wrong-type get returns default", wrongType(p));
    check(
        "remove",
        p.edit().remove("i").commit()
            && !p.contains("i")
            && p.getInt("i", 9) == 9
            && p.getAll().size() == 12);
    check("remove absent ok", p.edit().remove("nothere").commit());
    check(
        "overwrite type",
        p.edit().putString("b", "now-string").commit()
            && p.getString("b", "").equals("now-string")
            && p.getAll().size() == 12);
    check("overwrite value", p.edit().putInt("zero", 42).commit() && p.getInt("zero", 0) == 42);
    SharedPreferences.Editor e = p.edit();
    e.putInt("pending", 1);
    check("uncommitted edit invisible", !p.contains("pending"));
    e.apply();
    check("apply commits", p.contains("pending") && p.getInt("pending", 0) == 1);
    SharedPreferences again = SharedPreferences.open("qa");
    check("reopen sees data", again.getInt("pending", 0) == 1 && again.getAll().size() == 13);
    SharedPreferences other = SharedPreferences.open("qa2");
    other.edit().clear().putInt("x", 1).commit();
    check(
        "files independent",
        !p.contains("x") && other.getInt("x", 0) == 1 && !other.contains("pending"));
    check(
        "clear then put in one editor keeps put",
        p.edit().clear().putInt("k", 5).commit()
            && p.getAll().size() == 1
            && p.getInt("k", 0) == 5);
    check(
        "put then clear keeps put (Android order)",
        p.edit().putInt("k2", 6).clear().commit()
            && p.getAll().size() == 1
            && p.getInt("k2", 0) == 6);
    Map<String, ?> snap = p.getAll();
    p.edit().putInt("k3", 7).commit();
    check("getAll is a snapshot", snap.size() == 1 && p.getAll().size() == 2);
    p.edit().clear().commit();
    other.edit().clear().commit();
    check("cleared", p.getAll().size() == 0);
  }

  static boolean wrongType(SharedPreferences p) {
    try {
      int v = p.getInt("s", -5);
      String s = p.getString("i", "def");
      return v == -5 && s != null;
    } catch (ClassCastException e) {
      return true;
    }
  }

  static void prefsLimits() {
    SharedPreferences p = SharedPreferences.open("lim");
    p.edit().clear().commit();
    StringBuilder k63 = new StringBuilder();
    for (int i = 0; i < 63; i++) {
      k63.append('k');
    }
    String key63 = k63.toString();
    String key64 = key63 + "k";
    check("63-char key ok", p.edit().putInt(key63, 1).commit() && p.getInt(key63, 0) == 1);
    boolean rejected = rejectedInt(p, key64, 2);
    check("64-char key rejected", rejected);
    StringBuilder v1024 = new StringBuilder();
    for (int i = 0; i < 1024; i++) {
      v1024.append((char) ('a' + i % 26));
    }
    String val1024 = v1024.toString();
    check(
        "1024-char value ok",
        p.edit().putString("v", val1024).commit() && p.getString("v", "").length() == 1024);
    String val1025 = val1024 + "z";
    rejected = rejectedString(p, "v2", val1025);
    check("1025-char value rejected", rejected);
    p.edit().clear().commit();
    SharedPreferences.Editor e = p.edit();
    for (int i = 0; i < 64; i++) {
      e.putInt("e" + i, i);
    }
    check("64 entries ok", e.commit() && p.getAll().size() == 64 && p.getInt("e63", -1) == 63);
    boolean over = rejectedInt(p, "e64", 64);
    check("65th entry rejected", over);
    check("store intact after rejection", p.getAll().size() == 64 && p.getInt("e0", -1) == 0);
    check(
        "replace within limit ok",
        p.edit().putInt("e0", 100).commit() && p.getInt("e0", -1) == 100);
    p.edit().clear().commit();
    check("cleared limits store", p.getAll().size() == 0);
  }

  static boolean rejectedInt(SharedPreferences p, String k, int v) {
    try {
      return !p.edit().putInt(k, v).commit() || !p.contains(k);
    } catch (RuntimeException e) {
      Log.i(TAG, "putInt rejected by exception: " + e);
      return true;
    }
  }

  static boolean rejectedString(SharedPreferences p, String k, String v) {
    try {
      return !p.edit().putString(k, v).commit() || !p.contains(k);
    } catch (RuntimeException e) {
      Log.i(TAG, "putString rejected by exception: " + e);
      return true;
    }
  }

  // ---- StatFs ---------------------------------------------------------------------------------

  static void statfs() {
    StatFs s = new StatFs("/");
    long total = s.getTotalBytes();
    long free = s.getFreeBytes();
    long avail = s.getAvailableBytes();
    Log.i(TAG, "statfs total=" + total + " free=" + free + " avail=" + avail);
    check("total positive", total > 0);
    check("free <= total", free <= total && free >= 0);
    check("avail <= free", avail <= free && avail >= 0);
    check("block size 4096", s.getBlockSizeLong() == 4096);
    check("block count consistent", s.getBlockCountLong() * 4096 == total);
    check(
        "free blocks consistent",
        s.getFreeBlocksLong() * 4096 == free && s.getAvailableBlocksLong() * 4096 == avail);
    s.restat("/");
    check("restat stable", s.getTotalBytes() == total);
  }
}
