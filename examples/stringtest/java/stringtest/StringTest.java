// SPDX-License-Identifier: GPL-3.0-only
package stringtest;

import picodroid.app.Application;
import picodroid.util.Log;

public class StringTest extends Application {
  private static final String TAG = "StringTest";

  static int passed = 0;
  static int failed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  public void onCreate() {
    run();
  }

  public static void run() {
    Log.i(TAG, "=== String Tests ===");

    testConcat();
    testHashCode();
    testReplaceChar();
    testReplaceString();
    testToCharArray();
    testSplit();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  static void testConcat() {
    String a = "hello";
    String b = a.concat(" world");
    check("concat basic", b.equals("hello world"));
    String c = a.concat("");
    check("concat empty", c.equals("hello"));
    String d = "".concat("x");
    check("concat to empty", d.equals("x"));
  }

  static void testHashCode() {
    check("hashCode empty=0", "".hashCode() == 0);
    check("hashCode abc=96354", "abc".hashCode() == 96354);
    check("hashCode consistent", "test".hashCode() == "test".hashCode());
  }

  static void testReplaceChar() {
    String s = "hello";
    String r = s.replace('l', 'r');
    check("replace char l->r", r.equals("herro"));
    String r2 = s.replace('z', 'q');
    check("replace char no-match", r2.equals("hello"));
  }

  static void testReplaceString() {
    String s = "aXbXc";
    String r = s.replace("X", "YY");
    check("replace string", r.equals("aYYbYYc"));
    String r2 = "abc".replace("b", "");
    check("replace empty", r2.equals("ac"));
  }

  static void testToCharArray() {
    char[] arr = "abc".toCharArray();
    check("toCharArray length=3", arr.length == 3);
    check("toCharArray [0]=a", arr[0] == 'a');
    check("toCharArray [1]=b", arr[1] == 'b');
    check("toCharArray [2]=c", arr[2] == 'c');
    char[] empty = "".toCharArray();
    check("toCharArray empty length=0", empty.length == 0);
  }

  static void testSplit() {
    String[] parts = "a,b,c".split(",");
    check("split length=3", parts.length == 3);
    check("split [0]=a", parts[0].equals("a"));
    check("split [1]=b", parts[1].equals("b"));
    check("split [2]=c", parts[2].equals("c"));

    String[] noDelim = "hello".split(",");
    check("split no match length=1", noDelim.length == 1);
    check("split no match [0]=hello", noDelim[0].equals("hello"));

    String[] multi = "a::b::c".split("::");
    check("split multi-char length=3", multi.length == 3);
    check("split multi-char [0]=a", multi[0].equals("a"));
    check("split multi-char [2]=c", multi[2].equals("c"));
  }
}
