// SPDX-License-Identifier: GPL-3.0-only
package stringdemo;

import java.util.IllegalFormatException;
import picodroid.app.Application;
import picodroid.util.Log;

public class StringDemo extends Application {
  private static final String TAG = "StringDemo";

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

    testBasic();
    testComparison();
    testPredicates();
    testSearch();
    testTransforms();
    testValueOf();
    testStringBuilder();
    testConcat();
    testHashCode();
    testReplaceChar();
    testReplaceString();
    testToCharArray();
    testSplit();
    testFormatConversions();
    testFormatFlags();
    testFormatWidthPrecision();
    testFormatAutoboxing();
    testFormatMixed();
    testFormatErrors();

    String passStr = String.valueOf(passed);
    String failStr = String.valueOf(failed);
    Log.i(TAG, "Results: " + passStr + " passed, " + failStr + " failed");
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== SOME FAILED ===");
    }
  }

  // ── String basics ─────────────────────────────────────────────────────────

  static void testBasic() {
    String s = "Hello, Pico!";
    check("length=12", s.length() == 12);
    check("charAt(7)=P", s.charAt(7) == 'P');
    check("isEmpty=false", !s.isEmpty());
    check("empty isEmpty=true", "".isEmpty());
  }

  static void testComparison() {
    String a = "hello";
    String b = "hello";
    String c = "HELLO";
    check("equals same", a.equals(b));
    check("equals diff case", !a.equals(c));
    check("equalsIgnoreCase", a.equalsIgnoreCase(c));
    check("compareTo equal=0", a.compareTo(b) == 0);
  }

  static void testPredicates() {
    String msg = "GET /status HTTP/1.1";
    check("startsWith GET", msg.startsWith("GET"));
    check("endsWith 1.1", msg.endsWith("1.1"));
    check("contains status", msg.contains("status"));
  }

  static void testSearch() {
    String csv = "alpha,beta,gamma";
    check("indexOf comma=5", csv.indexOf(',') == 5);
    check("lastIndexOf comma=10", csv.lastIndexOf(',') == 10);
    check("indexOf beta=6", csv.indexOf("beta") == 6);
  }

  static void testTransforms() {
    check("trim", "  trim me  ".trim().equals("trim me"));
    check("toLowerCase", "PICO".toLowerCase().equals("pico"));
    check("toUpperCase", "pico".toUpperCase().equals("PICO"));
    check("substring(7,11)", "Hello, Pico!".substring(7, 11).equals("Pico"));
    check("substring(7)", "Hello, Pico!".substring(7).equals("Pico!"));
  }

  static void testValueOf() {
    check("valueOf(int)=42", String.valueOf(42).equals("42"));
    check("valueOf(long)", String.valueOf(9876543210L).equals("9876543210"));
    check("valueOf(true)", String.valueOf(true).equals("true"));
    check("valueOf(false)", String.valueOf(false).equals("false"));
  }

  static void testStringBuilder() {
    // NOTE: the JVM uses a single shared StringBuilder buffer. Capture results
    // into local variables before string concatenation (+), which itself
    // creates an internal StringBuilder that shares the same buffer.
    StringBuilder sb = new StringBuilder("val=");
    sb.append(100);
    String sbResult = sb.toString();
    check("sb basic", sbResult.equals("val=100"));

    StringBuilder sb2 = new StringBuilder();
    sb2.append("x=");
    sb2.append(3.14f);
    String sb2Result = sb2.toString();
    check("sb float", sb2Result.equals("x=3.14"));

    StringBuilder sb3 = new StringBuilder();
    sb3.append("flag=");
    sb3.append(true);
    String sb3Result = sb3.toString();
    check("sb bool", sb3Result.equals("flag=true"));

    StringBuilder sb4 = new StringBuilder("abcde");
    int sb4Len = sb4.length();
    int sb4Ch = sb4.charAt(2);
    check("sb length=5", sb4Len == 5);
    check("sb charAt(2)=99 (ASCII c)", sb4Ch == 99);
  }

  // ── String operations ────────────────────────────────────────────────────

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

  // ── String.format ────────────────────────────────────────────────────────

  static void testFormatConversions() {
    check("fmt %s", String.format("s=[%s]", "pico").equals("s=[pico]"));
    check("fmt %d", String.format("d=[%d]", 42).equals("d=[42]"));
    check("fmt %x", String.format("x=[%x]", 255).equals("x=[ff]"));
    check("fmt %X", String.format("X=[%X]", 255).equals("X=[FF]"));
    check("fmt %o", String.format("o=[%o]", 8).equals("o=[10]"));
    check("fmt %c", String.format("c=[%c]", 'A').equals("c=[A]"));
    check("fmt %b", String.format("b=[%b]", true).equals("b=[true]"));
    check("fmt %f default", String.format("f=[%f]", 3.14).equals("f=[3.140000]"));
    check("fmt %e", String.format("e=[%e]", 12345.678).equals("e=[1.234568e+04]"));
    check("fmt %g", String.format("g=[%g]", 0.0001234).equals("g=[0.000123400]"));
    check("fmt %%", String.format("pct=[%%]").equals("pct=[%]"));
    check("fmt %n", String.format("nl=[%n]").equals("nl=[\n]"));
  }

  static void testFormatFlags() {
    check("flag - left-align", String.format("[%-10s]", "hi").equals("[hi        ]"));
    check("flag 0 zero-pad", String.format("[%05d]", 42).equals("[00042]"));
    check("flag + sign", String.format("[%+d]", 42).equals("[+42]"));
    check("flag space", String.format("[% d]", 42).equals("[ 42]"));
    check("flag , group", String.format("[%,d]", 1234567).equals("[1,234,567]"));
    check("flag # alt hex", String.format("[%#x]", 255).equals("[0xff]"));
    check("flag # alt octal", String.format("[%#o]", 8).equals("[010]"));
  }

  static void testFormatWidthPrecision() {
    check("precision %.3s", String.format("[%.3s]", "abcdef").equals("[abc]"));
    check("width.precision %10.4f", String.format("[%10.4f]", 3.14159).equals("[    3.1416]"));
    check("zero-pad neg %08.2f", String.format("[%08.2f]", -1.5).equals("[-0001.50]"));
  }

  static void testFormatAutoboxing() {
    check("autobox Integer", String.format("int=%d", Integer.valueOf(7)).equals("int=7"));
    check("autobox long", String.format("long=%d", 9876543210L).equals("long=9876543210"));
    check(
        "autobox Double", String.format("double=%.2f", Double.valueOf(2.5)).equals("double=2.50"));
    check("autobox Boolean", String.format("bool=%b", Boolean.valueOf(true)).equals("bool=true"));
    check("autobox null", String.format("null=%s", (Object) null).equals("null=null"));
  }

  static void testFormatMixed() {
    String s = String.format("name=%s count=%d hex=%#06x done=%b", "pico", 42, 0xab, true);
    check("mixed formatters", s.equals("name=pico count=42 hex=0x00ab done=true"));
  }

  static void testFormatErrors() {
    boolean caught = false;
    try {
      String.format("%d %d", 1);
    } catch (IllegalFormatException e) {
      caught = true;
    } catch (RuntimeException e) {
      // Fallback in case IllegalFormatException isn't separately visible.
      caught = true;
    }
    check("err caught for too-few args", caught);
  }
}
