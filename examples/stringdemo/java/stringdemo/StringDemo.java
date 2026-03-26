package stringdemo;

import picodroid.util.Log;

public class StringDemo {
  private static final String TAG = "StringDemo";

  public static void main() {
    // ── Basic ──────────────────────────────────────────────────────────────
    String s = "Hello, Pico!";
    Log.i(TAG, "length=" + s.length()); // 12
    Log.i(TAG, "charAt(7)=" + s.charAt(7)); // P
    Log.i(TAG, "isEmpty=" + s.isEmpty()); // false
    Log.i(TAG, "empty isEmpty=" + "".isEmpty()); // true

    // ── Comparison ────────────────────────────────────────────────────────
    String a = "hello";
    String b = "hello";
    String c = "HELLO";
    Log.i(TAG, "equals=" + a.equals(b)); // true
    Log.i(TAG, "equals diff=" + a.equals(c)); // false
    Log.i(TAG, "equalsIgnoreCase=" + a.equalsIgnoreCase(c)); // true
    Log.i(TAG, "compareTo=" + a.compareTo(b)); // 0

    // ── Predicates ────────────────────────────────────────────────────────
    String msg = "GET /status HTTP/1.1";
    Log.i(TAG, "startsWith GET=" + msg.startsWith("GET")); // true
    Log.i(TAG, "endsWith 1.1=" + msg.endsWith("1.1")); // true
    Log.i(TAG, "contains status=" + msg.contains("status")); // true

    // ── Search ────────────────────────────────────────────────────────────
    String csv = "alpha,beta,gamma";
    Log.i(TAG, "indexOf comma=" + csv.indexOf(',')); // 5
    Log.i(TAG, "lastIndexOf comma=" + csv.lastIndexOf(',')); // 10
    Log.i(TAG, "indexOf beta=" + csv.indexOf("beta")); // 6

    // ── Allocating / transforms ────────────────────────────────────────────
    String padded = "  trim me  ";
    String trimmed = padded.trim();
    Log.i(TAG, "trim=[" + trimmed + "]"); // [trim me]

    String lower = "PICO";
    Log.i(TAG, "toLowerCase=" + lower.toLowerCase()); // pico

    String upper = "pico";
    Log.i(TAG, "toUpperCase=" + upper.toUpperCase()); // PICO

    String full = "Hello, Pico!";
    Log.i(TAG, "substring(7,11)=" + full.substring(7, 11)); // Pico
    Log.i(TAG, "substring(7)=" + full.substring(7)); // Pico!

    // ── String.valueOf ─────────────────────────────────────────────────────
    Log.i(TAG, "valueOf(int)=" + String.valueOf(42)); // 42
    Log.i(TAG, "valueOf(long)=" + String.valueOf(9876543210L)); // 9876543210
    Log.i(TAG, "valueOf(true)=" + String.valueOf(true)); // true
    Log.i(TAG, "valueOf(false)=" + String.valueOf(false)); // false

    // ── StringBuilder ─────────────────────────────────────────────────────
    // NOTE: the JVM uses a single shared StringBuilder buffer. Capture results
    // into local variables before using string concatenation (+), which itself
    // creates an internal StringBuilder that shares the same buffer.
    StringBuilder sb = new StringBuilder("val=");
    sb.append(100);
    String sbResult = sb.toString(); // capture before any concatenation
    Log.i(TAG, "sb basic=" + sbResult); // val=100

    StringBuilder sb2 = new StringBuilder();
    sb2.append("x=");
    sb2.append(3.14f);
    String sb2Result = sb2.toString();
    Log.i(TAG, "sb float=" + sb2Result); // x=3.14...

    StringBuilder sb3 = new StringBuilder();
    sb3.append("flag=");
    sb3.append(true);
    String sb3Result = sb3.toString();
    Log.i(TAG, "sb bool=" + sb3Result); // flag=true

    StringBuilder sb4 = new StringBuilder("abcde");
    int sb4Len = sb4.length(); // capture before concatenation
    int sb4Ch = sb4.charAt(2); // capture before concatenation
    Log.i(TAG, "sb length=" + sb4Len); // 5
    Log.i(TAG, "sb charAt(2)=" + sb4Ch); // 99 (ASCII 'c')
  }
}
