// SPDX-License-Identifier: GPL-3.0-only
package filesdemo;

import java.io.IOException;
import picodroid.app.Application;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

/**
 * Conformance checks for the storage sandbox and the {@code Context} file API (multi-app M3): every
 * path this app names lives under its own directory, {@code getFilesDir()} is {@code /files},
 * {@code openFileOutput} / {@code openFileInput} / {@code fileList} / {@code deleteFile}
 * round-trip, {@code File.list} works, and a path that climbs out or is too long is refused. A
 * bench row on every board.
 */
public class FilesDemo extends Application {
  private static final String TAG = "FilesDemo";

  static int passed;
  static int failed;

  static void check(String name, boolean cond) {
    if (cond) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== Files tests ===");
    cleanup();
    testDirs();
    testRoundTrip();
    testAppend();
    testFileList();
    testDeleteFile();
    testMkdirsAndListFiles();
    testRename();
    testRefusals();
    testLongPath();
    cleanup();
    Log.i(TAG, "Results: " + passed + " passed, " + failed + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  void testDirs() {
    check("data dir is the root", "/".equals(getDataDir().getPath()));
    File files = getFilesDir();
    check("files dir path", "/files".equals(files.getPath()));
    check("files dir exists", files.exists() && files.isDirectory());
  }

  void testRoundTrip() {
    byte[] payload = "hello sandbox".getBytes();
    try {
      FileOutputStream out = openFileOutput("note.txt", MODE_PRIVATE);
      out.write(payload);
      out.close();
      FileInputStream in = openFileInput("note.txt");
      byte[] buf = new byte[64];
      int n = in.read(buf);
      in.close();
      check("round trip length", n == payload.length);
      check("round trip bytes", sameBytes(buf, payload, n));
      check("file length", new File("/files/note.txt").length() == (long) payload.length);
    } catch (IOException e) {
      check("round trip threw " + e.getMessage(), false);
    }
  }

  void testAppend() {
    try {
      FileOutputStream out = openFileOutput("note.txt", MODE_APPEND);
      out.write(" more".getBytes());
      out.close();
      check("append grows the file", new File("/files/note.txt").length() == 18L);
      FileOutputStream trunc = openFileOutput("note.txt", MODE_PRIVATE);
      trunc.write("x".getBytes());
      trunc.close();
      check("private mode truncates", new File("/files/note.txt").length() == 1L);
    } catch (IOException e) {
      check("append threw " + e.getMessage(), false);
    }
  }

  void testFileList() {
    check("fileList has the note", contains(fileList(), "note.txt"));
    String[] raw = new File("/files").list();
    check("File.list matches", raw != null && contains(raw, "note.txt"));
    check("list of a missing dir is null", new File("/nowhere").list() == null);
    check("list of a file is null", new File("/files/note.txt").list() == null);
  }

  void testDeleteFile() {
    check("deleteFile", deleteFile("note.txt"));
    check("deleteFile again is false", !deleteFile("note.txt"));
    boolean threw = false;
    try {
      openFileInput("note.txt");
    } catch (IOException e) {
      threw = true;
    }
    check("openFileInput of a missing file throws", threw);
    check("fileList no longer has it", !contains(fileList(), "note.txt"));
  }

  void testMkdirsAndListFiles() {
    check("mkdirs two deep", new File("/fd/a/b").mkdirs());
    check("mkdirs isDirectory", new File("/fd/a/b").isDirectory());
    File[] kids = new File("/fd").listFiles();
    boolean one = kids != null && kids.length == 1;
    check("listFiles one child", one && "a".equals(kids[0].getName()));
    check("listFiles path", one && "/fd/a".equals(kids[0].getPath()));
  }

  void testRename() {
    try {
      check("createNewFile", new File("/fd/x").createNewFile());
      check("createNewFile again is false", !new File("/fd/x").createNewFile());
    } catch (IOException e) {
      check("createNewFile threw " + e.getMessage(), false);
    }
    check("renameTo", new File("/fd/x").renameTo(new File("/fd/y")));
    check("renamed exists", new File("/fd/y").exists() && !new File("/fd/x").exists());
  }

  void testRefusals() {
    boolean threw = false;
    try {
      new File("../escape").createNewFile();
    } catch (IOException e) {
      threw = true;
    }
    check("createNewFile past the root throws", threw);
    check("exists past the root is false", !new File("/../escape").exists());
    check("mkdir past the root is false", !new File("/fd/../../escape").mkdir());
    // A path that spells another package's directory stays inside this
    // app's own root: `/data` here is a fresh directory of this app's.
    try {
      new File("/data/other").mkdirs();
      FileOutputStream out = new FileOutputStream("/data/other/x");
      out.write("y".getBytes());
      out.close();
    } catch (IOException e) {
      check("write under /data threw " + e.getMessage(), false);
    }
    String[] under = new File("/data").list();
    boolean own = under != null && under.length == 1 && "other".equals(under[0]);
    check("/data is this app's own directory", own);
  }

  void testLongPath() {
    StringBuilder sb = new StringBuilder("/");
    for (int i = 0; i < 200; i++) {
      sb.append('a');
    }
    String longPath = sb.toString();
    boolean threw = false;
    try {
      new File(longPath).createNewFile();
    } catch (IOException e) {
      threw = true;
    }
    check("a 201-byte path is refused", threw && !new File(longPath).exists());
  }

  void cleanup() {
    new File("/files/note.txt").delete();
    new File("/fd/x").delete();
    new File("/fd/y").delete();
    new File("/fd/a/b").delete();
    new File("/fd/a").delete();
    new File("/fd").delete();
    new File("/data/other/x").delete();
    new File("/data/other").delete();
    new File("/data").delete();
  }

  static boolean contains(String[] names, String want) {
    for (int i = 0; i < names.length; i++) {
      if (want.equals(names[i])) {
        return true;
      }
    }
    return false;
  }

  static boolean sameBytes(byte[] got, byte[] want, int n) {
    if (n != want.length) {
      return false;
    }
    for (int i = 0; i < n; i++) {
      if (got[i] != want[i]) {
        return false;
      }
    }
    return true;
  }
}
