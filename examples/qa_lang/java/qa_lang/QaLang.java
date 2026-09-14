// SPDX-License-Identifier: GPL-3.0-only
package qa_lang;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * QA 2026-09-13: Java language and bytecode semantics that no other demo pins — numeric conversions
 * and overflow, shift masking, switch forms, evaluation order, try/finally ordering and the
 * catchable exception family. Every check names what it pins; a section that throws is reported and
 * the run continues, so one defect cannot hide the rest.
 */
public class QaLang extends Application {
  private static final String TAG = "QaLang";

  static int passed = 0;
  static int failed = 0;
  static int crashed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  interface Section {
    void run();
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
    Log.i(TAG, "=== QaLang start ===");
    section("narrowing", () -> narrowing());
    section("intArith", () -> intArith());
    section("longArith", () -> longArith());
    section("floatArith", () -> floatArith());
    section("switches", () -> switches());
    section("controlFlow", () -> controlFlow());
    section("evalOrder", () -> evalOrder());
    section("tryFinally", () -> tryFinally());
    section("exceptions", () -> exceptions());
    section("arrays", () -> arrays());
    section("wideLocals", () -> wideLocals());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  // ---- narrowing / widening conversions ----------------------------------------------------

  static int sink = 0;

  static double dbl(double d) {
    sink++;
    return d;
  }

  static float flt(float f) {
    sink++;
    return f;
  }

  static long lng(long l) {
    sink++;
    return l;
  }

  static int integer(int i) {
    sink++;
    return i;
  }

  static void narrowing() {
    check("(byte)200 == -56", (byte) integer(200) == -56);
    check("(short)70000 == 4464", (short) integer(70000) == 4464);
    check("(char)-1 == 65535", (char) integer(-1) == 65535);
    check("(byte)-129 == 127", (byte) integer(-129) == 127);
    check("(int)3.99 == 3", (int) dbl(3.99) == 3);
    check("(int)-3.99 == -3", (int) dbl(-3.99) == -3);
    check("(int)NaN == 0", (int) dbl(Double.NaN) == 0);
    check("(long)NaN == 0", (long) dbl(Double.NaN) == 0L);
    check("(int)1e20 == MAX", (int) dbl(1e20) == Integer.MAX_VALUE);
    check("(int)-1e20 == MIN", (int) dbl(-1e20) == Integer.MIN_VALUE);
    check("(long)1e30 == MAX", (long) dbl(1e30) == Long.MAX_VALUE);
    check("(long)-1e30 == MIN", (long) dbl(-1e30) == Long.MIN_VALUE);
    check("(int)+Inf(float) == MAX", (int) flt(Float.POSITIVE_INFINITY) == Integer.MAX_VALUE);
    check("(int)-Inf(float) == MIN", (int) flt(Float.NEGATIVE_INFINITY) == Integer.MIN_VALUE);
    check("(long)NaN(float) == 0", (long) flt(Float.NaN) == 0L);
    check("(int)-0.9f == 0", (int) flt(-0.9f) == 0);
    check("(char)65.7 == 'A'", (char) dbl(65.7) == 'A');
    check("(byte)300.0 == 44", (byte) dbl(300.0) == 44);
    check("(short)1e10 == -1", (short) dbl(1e10) == -1);
    check("(byte)-129.0 == 127", (byte) dbl(-129.0) == 127);
    check("(int)(char)-1 == 65535", (int) (char) integer(-1) == 65535);
    check("(long)(int)0x1_0000_0000L == 0", (int) lng(0x1_0000_0000L) == 0);
    check("(int)0xFFFF_FFFFL == -1", (int) lng(0xFFFF_FFFFL) == -1);
    check("int 0xFFFFFFFF sign-extends to long -1", lng(0xFFFFFFFF) == -1L);
    check("(float)16777217 == 16777216f", (float) integer(16777217) == 16777216f);
    check("(float)(long)1<<40 exact", (float) lng(1L << 40) == 1099511627776f);
    check("(double)Long.MAX rounds up", (double) lng(Long.MAX_VALUE) == 9.223372036854776E18);
    check(
        "(long)(double)Long.MAX saturates", (long) (double) lng(Long.MAX_VALUE) == Long.MAX_VALUE);
    check("(float)0.1 != 0.1", (float) dbl(0.1) != 0.1);
    check("(double)(float)0.1 == 0.10000000149011612", (double) flt(0.1f) == 0.10000000149011612);
    // compound assignment narrows implicitly
    byte b = 127;
    b += integer(1);
    check("byte 127 += 1 wraps to -128", b == -128);
    char c = 'a';
    c += integer(1);
    check("char 'a' += 1 == 'b'", c == 'b');
    c++;
    check("char++ == 'c'", c == 'c');
    short s = -1;
    s >>>= integer(1);
    check("short -1 >>>= 1 stays -1", s == -1);
    int i = 5;
    i *= dbl(2.5);
    check("int 5 *= 2.5 == 12", i == 12);
    i = 7;
    i /= dbl(2.0);
    check("int 7 /= 2.0 == 3", i == 3);
    long l = 10;
    l += integer(-3);
    check("long += int", l == 7L);
    float f = 1.5f;
    f += dbl(1.25);
    check("float += double narrows", f == 2.75f);
    i = 3;
    i += dbl(0.9);
    check("int 3 += 0.9 == 3", i == 3);
    i = -3;
    i -= dbl(0.9);
    check("int -3 -= 0.9 == -3", i == -3);
    int unaryPlus = +integer(-4);
    check("unary plus", unaryPlus == -4);
    check("'A' + 1 is int 66", ('A' + integer(1)) == 66);
    check("(char)('a' + 25) == 'z'", (char) ('a' + integer(25)) == 'z');
    check("\"\" + 'A' + 1 == \"A1\"", ("" + 'A' + integer(1)).equals("A1"));
    check("1 + 2 + \"3\" == \"33\"", (integer(1) + integer(2) + "3").equals("33"));
    check("\"1\" + 2 + 3 == \"123\"", ("1" + integer(2) + integer(3)).equals("123"));
    check("(long)MAX + 1 == 2147483648", (long) integer(Integer.MAX_VALUE) + 1 == 2147483648L);
    String sn = null;
    sn += "a";
    check("null += \"a\" == \"nulla\"", "nulla".equals(sn));
    check("ternary promotes to double", (integer(1) > 0 ? 1 : 2.0) == 1.0);
    Object o = integer(1) > 0 ? Integer.valueOf(1) : "s";
    check("ternary Object branch is Integer", o instanceof Integer);
  }

  // ---- int arithmetic -------------------------------------------------------------------------

  static void intArith() {
    check("MAX + 1 == MIN", integer(Integer.MAX_VALUE) + 1 == Integer.MIN_VALUE);
    check("MIN - 1 == MAX", integer(Integer.MIN_VALUE) - 1 == Integer.MAX_VALUE);
    check("MAX * 2 == -2", integer(Integer.MAX_VALUE) * 2 == -2);
    check("MIN / -1 == MIN", integer(Integer.MIN_VALUE) / integer(-1) == Integer.MIN_VALUE);
    check("MIN % -1 == 0", integer(Integer.MIN_VALUE) % integer(-1) == 0);
    check("-7 / 2 == -3", integer(-7) / integer(2) == -3);
    check("-7 % 2 == -1", integer(-7) % integer(2) == -1);
    check("7 % -2 == 1", integer(7) % integer(-2) == 1);
    check("-7 % -2 == -1", integer(-7) % integer(-2) == -1);
    check("1 << 32 == 1 (masked)", (integer(1) << integer(32)) == 1);
    check("1 << 33 == 2 (masked)", (integer(1) << integer(33)) == 2);
    check("1 << -1 == MIN", (integer(1) << integer(-1)) == Integer.MIN_VALUE);
    check("-8 >> 1 == -4", (integer(-8) >> integer(1)) == -4);
    check("-8 >>> 28 == 15", (integer(-8) >>> integer(28)) == 15);
    check("-1 >>> 1 == MAX", (integer(-1) >>> integer(1)) == Integer.MAX_VALUE);
    check("MIN >> 31 == -1", (integer(Integer.MIN_VALUE) >> integer(31)) == -1);
    check("MIN >>> 31 == 1", (integer(Integer.MIN_VALUE) >>> integer(31)) == 1);
    check("x >> 32 == x", (integer(12345) >> integer(32)) == 12345);
    check("(byte)0x80 >>> 1 promoted", ((byte) integer(0x80) >>> integer(1)) == 2147483584);
    check("~0 == -1", ~integer(0) == -1);
    check("5 & -2 == 4", (integer(5) & integer(-2)) == 4);
    check("5 | 3 == 7", (integer(5) | integer(3)) == 7);
    check("5 ^ 3 == 6", (integer(5) ^ integer(3)) == 6);
    check("x ^ x == 0", xorSelf() == 0);
    check("(int)0xF0F0F0F0L == -252645136", (int) lng(0xF0F0F0F0L) == -252645136);
    check("0x80000000 >>> 0 == MIN", (integer(0x80000000) >>> integer(0)) == Integer.MIN_VALUE);
    int x = integer(1);
    x = x++;
    check("x = x++ leaves x", x == 1);
    x = integer(1);
    x = x++ + ++x;
    check("i = i++ + ++i == 4", x == 4);
    x = integer(1);
    x += (x = integer(5));
    check("x += (x = 5) == 6", x == 6);
    int y = integer(10);
    int z = y-- - --y;
    check("y-- - --y == 2", z == 2 && y == 8);
    // iinc with a 16-bit constant (wide form)
    int w = integer(0);
    w += 300;
    w += -300;
    w += 32767;
    w += -32768;
    check("wide iinc chain == -1", w == -1);
    w -= 200;
    check("wide iinc negative == -201", w == -201);
    // Math on ints
    check("Math.abs(-7)", Math.abs(integer(-7)) == 7);
    check(
        "Math.max(MIN, MAX)",
        Math.max(integer(Integer.MIN_VALUE), integer(Integer.MAX_VALUE)) == Integer.MAX_VALUE);
    check("Math.min(-0, 0) ints", Math.min(integer(0), integer(-0)) == 0);
    // Boolean logic
    boolean t = integer(1) == 1;
    boolean f = integer(1) == 2;
    boolean tCopy = t;
    check("t ^ f", (t ^ f) && !(t ^ tCopy));
    check("t & f", !(t & f) && (t | f));
    check("!t == f", !t == f);
    // Integer boxing identity
    Integer a1 = Integer.valueOf(integer(127));
    Integer a2 = Integer.valueOf(integer(127));
    Object oa1 = a1;
    Object oa2 = a2;
    check("Integer cache 127 identical", oa1 == oa2);
    check(
        "Integer 1000 equals",
        Integer.valueOf(integer(1000)).equals(Integer.valueOf(integer(1000))));
    Integer nul = null;
    boolean npe = false;
    try {
      int unboxed = nul;
      sink += unboxed;
    } catch (NullPointerException e) {
      npe = true;
    }
    check("unboxing null throws NPE", npe);
    npe = false;
    try {
      int viaTernary = integer(1) > 0 ? nul : integer(0);
      sink += viaTernary;
    } catch (NullPointerException e) {
      npe = true;
    }
    check("ternary unboxing null throws NPE", npe);
    Integer boxedSwitch = Integer.valueOf(integer(2));
    int sw = -1;
    switch (boxedSwitch) {
      case 1:
        sw = 1;
        break;
      case 2:
        sw = 2;
        break;
      default:
        sw = 0;
    }
    check("switch on Integer unboxes", sw == 2);
  }

  // ---- long arithmetic ------------------------------------------------------------------------

  static void longArith() {
    check("Long.MAX + 1 == MIN", lng(Long.MAX_VALUE) + 1 == Long.MIN_VALUE);
    check("Long.MIN - 1 == MAX", lng(Long.MIN_VALUE) - 1 == Long.MAX_VALUE);
    check("Long.MIN / -1 == MIN", lng(Long.MIN_VALUE) / lng(-1) == Long.MIN_VALUE);
    check("Long.MIN % -1 == 0", lng(Long.MIN_VALUE) % lng(-1) == 0);
    check("-9L / 2 == -4", lng(-9) / lng(2) == -4);
    check("-9L % 2 == -1", lng(-9) % lng(2) == -1);
    check("9L % -2 == 1", lng(9) % lng(-2) == 1);
    check("1L << 63 == MIN", (lng(1) << integer(63)) == Long.MIN_VALUE);
    check("1L << 64 == 1 (masked)", (lng(1) << integer(64)) == 1L);
    check("1L << 65 == 2 (masked)", (lng(1) << integer(65)) == 2L);
    check("-1L >>> 60 == 15", (lng(-1) >>> integer(60)) == 15L);
    check("-1L >>> 1 == MAX", (lng(-1) >>> integer(1)) == Long.MAX_VALUE);
    check("MIN >> 63 == -1", (lng(Long.MIN_VALUE) >> integer(63)) == -1L);
    check("MIN >>> 63 == 1", (lng(Long.MIN_VALUE) >>> integer(63)) == 1L);
    check("x >> 64 == x", (lng(12345) >> integer(64)) == 12345L);
    check("~0L == -1", ~lng(0) == -1L);
    check("(1L<<32)|1", ((lng(1) << 32) | 1) == 4294967297L);
    check("lcmp MIN < MAX", lng(Long.MIN_VALUE) < lng(Long.MAX_VALUE));
    check("lcmp MAX > -1", lng(Long.MAX_VALUE) > lng(-1));
    check("lcmp 1<<63 < 0", (lng(1) << 63) < 0);
    check("lcmp -5 < 3", lng(-5) < lng(3));
    long l42a = lng(42);
    long l42b = lng(42);
    check("lcmp equal", l42a == l42b && !(l42a < l42b) && l42a <= l42b);
    check("long * long overflow", lng(3037000500L) * lng(3037000500L) == -9223372036709301616L);
    check(
        "long from int mul overflows as int",
        lng(integer(1000000) * integer(1000000)) == -727379968L);
    check("long from int mul widened", lng(integer(1000000)) * integer(1000000) == 1000000000000L);
    check("Math.abs(Long.MIN)", Math.abs(lng(Long.MIN_VALUE)) == Long.MIN_VALUE);
    check("Math.max longs", Math.max(lng(-1), lng(Long.MIN_VALUE)) == -1L);
    check("l2f big", (float) lng(123456789012L) == 1.23456791E11f);
    check("l2d exact", (double) lng(1L << 53) == 9007199254740992.0);
    long[] arr = new long[3];
    int idx = 0;
    arr[idx++] += lng(5);
    arr[idx] = arr[0] *= lng(3);
    check("long[] compound ops", arr[0] == 15 && arr[1] == 15 && idx == 1);
    long post = arr[0]++;
    long pre = ++arr[0];
    check("long[] post/pre increments", post == 15 && pre == 17 && arr[0] == 17);
    long lv = lng(5);
    lv <<= integer(2);
    lv |= lng(1);
    lv ^= lng(0xFF);
    check("long compound shifts/bits", lv == 234L);
    check("Long.MAX in hex", lng(0x7FFF_FFFF_FFFF_FFFFL) == Long.MAX_VALUE);
    check("negative long literal parse", lng(-9223372036854775808L) == Long.MIN_VALUE);
  }

  // ---- float / double arithmetic -------------------------------------------------------------

  static void floatArith() {
    check("5.0 / 0 == +Inf", dbl(5.0) / dbl(0) == Double.POSITIVE_INFINITY);
    check("-5.0 / 0 == -Inf", dbl(-5.0) / dbl(0) == Double.NEGATIVE_INFINITY);
    double nan = dbl(0) / integer(0);
    check("0.0 / 0 is NaN", isNaN(nan));
    check("NaN == NaN false", !eqd(nan, nan));
    check("NaN < 1 false, NaN > 1 false", !(nan < 1) && !(nan > 1) && !(nan <= 1) && !(nan >= 1));
    check("-0.0 == 0.0", dbl(-0.0) == dbl(0.0));
    check("Double.compare(0.0, -0.0) > 0", Double.compare(dbl(0.0), dbl(-0.0)) > 0);
    check("Double.compare(NaN, NaN) == 0", Double.compare(nan, nan) == 0);
    check("Double.compare(NaN, Inf) > 0", Double.compare(nan, Double.POSITIVE_INFINITY) > 0);
    check("0.1 + 0.2 != 0.3", dbl(0.1) + dbl(0.2) != 0.3);
    check("0.1 + 0.2 == 0.30000000000000004", dbl(0.1) + dbl(0.2) == 0.30000000000000004);
    check("1e308 * 10 == Inf", dbl(1e308) * dbl(10) == Double.POSITIVE_INFINITY);
    check("Double.MIN_VALUE / 2 == 0", dbl(Double.MIN_VALUE) / dbl(2) == 0.0);
    check("Float.MAX * 2 == Inf", flt(Float.MAX_VALUE) * flt(2) == Float.POSITIVE_INFINITY);
    check("Float.MIN / 2 == 0", flt(Float.MIN_VALUE) / flt(2) == 0.0f);
    check("5.5 % 2 == 1.5", dbl(5.5) % dbl(2) == 1.5);
    check("-5.5 % 2 == -1.5", dbl(-5.5) % dbl(2) == -1.5);
    check("5.5 % -2 == 1.5", dbl(5.5) % dbl(-2) == 1.5);
    double infMod = dbl(Double.POSITIVE_INFINITY) % dbl(2);
    check("Inf % 2 is NaN", isNaN(infMod));
    check("5 % Inf == 5", dbl(5) % dbl(Double.POSITIVE_INFINITY) == 5.0);
    check("5.5f % 2f == 1.5f", flt(5.5f) % flt(2f) == 1.5f);
    check("float 1/3", flt(1f) / flt(3f) == 0.33333334f);
    check("double 1/3", dbl(1) / dbl(3) == 0.3333333333333333);
    check("Math.round(2.5) == 3", Math.round(dbl(2.5)) == 3L);
    check("Math.round(-2.5) == -2", Math.round(dbl(-2.5)) == -2L);
    check("Math.round(0.49999999999999994) == 0", Math.round(dbl(0.49999999999999994)) == 0L);
    check("Math.round(2.5f) == 3", Math.round(flt(2.5f)) == 3);
    check("Math.round(-2.5f) == -2", Math.round(flt(-2.5f)) == -2);
    check("Math.round(NaN) == 0", Math.round(nan) == 0L);
    check("Math.round(1e19) == Long.MAX", Math.round(dbl(1e19)) == Long.MAX_VALUE);
    check("Math.floor(-0.5) == -1", Math.floor(dbl(-0.5)) == -1.0);
    check("Math.ceil(-0.5) == -0.0", Math.ceil(dbl(-0.5)) == 0.0 && 1 / Math.ceil(dbl(-0.5)) < 0);
    check("Math.sqrt(-1) NaN", isNaN(Math.sqrt(dbl(-1))));
    check("Math.pow(2, 0.5)", Math.pow(dbl(2), dbl(0.5)) == 1.4142135623730951);
    check("Math.pow(0, 0) == 1", Math.pow(dbl(0), dbl(0)) == 1.0);
    check("Math.pow(-8, 1.0/3) NaN", isNaN(Math.pow(dbl(-8), dbl(1.0 / 3))));
    check("Math.abs(-0.0) is +0.0", 1 / Math.abs(dbl(-0.0)) > 0);
    check("Math.max(NaN, 1) NaN", isNaN(Math.max(nan, dbl(1))));
    check("Math.log(0) == -Inf", Math.log(dbl(0)) == Double.NEGATIVE_INFINITY);
    check("Math.exp(710) == Inf", Math.exp(dbl(710)) == Double.POSITIVE_INFINITY);
    check("Math.sin(PI) tiny", Math.abs(Math.sin(dbl(Math.PI))) < 1e-15);
    check("Math.atan2(0, -1) == PI", Math.atan2(dbl(0), dbl(-1)) == Math.PI);
    check("Math.toDegrees(PI) == 180", Math.toDegrees(dbl(Math.PI)) == 180.0);
    // f2i/f2l/d2i on exact boundaries
    check("(int)2147483647.0f == MAX", (int) flt(2147483647.0f) == Integer.MAX_VALUE);
    check("(int)-2147483648.0f == MIN", (int) flt(-2147483648.0f) == Integer.MIN_VALUE);
    check("(long)9.223372036854776E18 == MAX", (long) dbl(9.223372036854776E18) == Long.MAX_VALUE);
    // double compare via dcmpg/dcmpl on NaN in both branch senses
    double a = nan;
    boolean lt = a < 1.0;
    boolean gt = a > 1.0;
    boolean ge = a >= 1.0;
    boolean le = a <= 1.0;
    check("NaN in every relational is false", !lt && !gt && !ge && !le);
    check("!(NaN < 1) true", !(a < 1.0));
    float fa = flt(Float.NaN);
    check("float NaN relational false", !(fa < 1f) && !(fa > 1f) && !eqf(fa, fa));
    double[] d = new double[2];
    d[0] += dbl(1.5);
    d[1] = d[0] *= dbl(2);
    check("double[] compound ops", d[0] == 3.0 && d[1] == 3.0);
    float[] fs = new float[1];
    fs[0] -= flt(0.25f);
    check("float[] compound", fs[0] == -0.25f);
    check("float widening in mixed", flt(1.5f) + dbl(0.25) == 1.75);
    check("int/float mixed division", integer(1) / flt(4f) == 0.25f);
    check("int/int then to double stays truncated", (double) (integer(1) / integer(4)) == 0.0);
    check("long to float round", (float) lng(16777217L) == 16777216f);
    check("negative zero float", 1f / flt(-0.0f) < 0);
    check("Float.floatToIntBits(-0.0f)", Float.floatToIntBits(flt(-0.0f)) == 0x80000000);
    check("Float.intBitsToFloat", Float.intBitsToFloat(integer(0x3f800000)) == 1.0f);
  }

  // ---- switches -------------------------------------------------------------------------------

  enum Kind {
    ALPHA,
    BETA,
    GAMMA,
    DELTA
  }

  static String strSwitch(String s) {
    switch (s) {
      case "Aa":
        return "aa";
      case "BB":
        return "bb";
      case "":
        return "empty";
      case "long key with spaces":
        return "long";
      default:
        return "def";
    }
  }

  static int sparse(int k) {
    switch (k) {
      case -1000000:
        return 1;
      case -1:
        return 2;
      case 0:
        return 3;
      case 7:
        return 4;
      case 1000000:
        return 5;
      case Integer.MAX_VALUE:
        return 6;
      case Integer.MIN_VALUE:
        return 7;
      default:
        return 0;
    }
  }

  static int dense(int k) {
    switch (k) {
      case 3:
        return 30;
      case 4:
        return 40;
      case 5:
        return 50;
      case 6:
        return 60;
      case 7:
        return 70;
      case 8:
        return 80;
      default:
        return -1;
    }
  }

  static int fallthrough(int k) {
    int acc = 0;
    switch (k) {
      case 1:
        acc += 1;
      // fall through
      case 2:
        acc += 2;
      // fall through
      case 3:
        acc += 4;
        break;
      case 4:
        acc += 8;
      // fall through
      default:
        acc += 16;
    }
    return acc;
  }

  static String kind(Kind k) {
    switch (k) {
      case ALPHA:
        return "a";
      case GAMMA:
        return "g";
      default:
        return "other";
    }
  }

  static int charSwitch(char c) {
    switch (c) {
      case 'a':
        return 1;
      case 'z':
        return 26;
      case '\n':
        return -1;
      case 'é':
        return 233;
      default:
        return 0;
    }
  }

  static String dyn(String base) {
    sink++;
    return base + "";
  }

  static void switches() {
    check("string switch Aa", strSwitch(dyn("Aa")).equals("aa"));
    check("string switch BB (hash collision)", strSwitch(dyn("BB")).equals("bb"));
    check("string switch empty", strSwitch(dyn("")).equals("empty"));
    check("string switch long", strSwitch(dyn("long key with spaces")).equals("long"));
    check("string switch default", strSwitch(dyn("Ab")).equals("def"));
    check("string switch same-hash default", strSwitch(dyn("AaAa")).equals("def"));
    check("sparse -1000000", sparse(integer(-1000000)) == 1);
    check("sparse -1", sparse(integer(-1)) == 2);
    check("sparse 0", sparse(integer(0)) == 3);
    check("sparse 7", sparse(integer(7)) == 4);
    check("sparse 1000000", sparse(integer(1000000)) == 5);
    check("sparse MAX", sparse(integer(Integer.MAX_VALUE)) == 6);
    check("sparse MIN", sparse(integer(Integer.MIN_VALUE)) == 7);
    check("sparse default", sparse(integer(8)) == 0 && sparse(integer(-2)) == 0);
    check("dense low", dense(integer(3)) == 30);
    check("dense high", dense(integer(8)) == 80);
    check("dense below", dense(integer(2)) == -1);
    check("dense above", dense(integer(9)) == -1);
    check("dense MIN", dense(integer(Integer.MIN_VALUE)) == -1);
    check("fallthrough 1", fallthrough(integer(1)) == 7);
    check("fallthrough 2", fallthrough(integer(2)) == 6);
    check("fallthrough 3", fallthrough(integer(3)) == 4);
    check("fallthrough 4", fallthrough(integer(4)) == 24);
    check("fallthrough default", fallthrough(integer(9)) == 16);
    check("enum switch ALPHA", kind(Kind.ALPHA).equals("a"));
    check("enum switch GAMMA", kind(Kind.GAMMA).equals("g"));
    check(
        "enum switch default", kind(Kind.DELTA).equals("other") && kind(Kind.BETA).equals("other"));
    check("char switch a", charSwitch((char) integer('a')) == 1);
    check("char switch z", charSwitch((char) integer('z')) == 26);
    check("char switch newline", charSwitch((char) integer('\n')) == -1);
    check("char switch e-acute", charSwitch((char) integer(0xe9)) == 233);
    check("char switch default", charSwitch((char) integer('q')) == 0);
    boolean npe = false;
    try {
      strSwitch(null);
    } catch (NullPointerException e) {
      npe = true;
    }
    check("string switch on null throws NPE", npe);
  }

  // ---- control flow ---------------------------------------------------------------------------

  static void controlFlow() {
    int hits = 0;
    outer:
    for (int i = 0; i < 5; i++) {
      for (int j = 0; j < 5; j++) {
        if (j == 3) {
          continue outer;
        }
        if (i == 3) {
          break outer;
        }
        hits++;
      }
    }
    check("labeled continue/break", hits == 9);
    int n = 0;
    do {
      n++;
    } while (n < integer(0));
    check("do-while runs once", n == 1);
    int a = 0;
    int b = 10;
    for (int i = 0, j = 10; i < j; i++, j--) {
      a++;
      b--;
    }
    check("for with two variables", a == 5 && b == 5);
    int count = 0;
    while (true) {
      if (++count >= integer(4)) {
        break;
      }
    }
    check("while(true) break", count == 4);
    check("nested ternary", (integer(5) > 3 ? (integer(2) > 1 ? "x" : "y") : "z").equals("x"));
    sink = 0;
    boolean sc = integer(0) == 1 && integer(1) == 1;
    check("&& short-circuits", !sc && sink == 1);
    sink = 0;
    sc = integer(1) == 1 || integer(2) == 2;
    check("|| short-circuits", sc && sink == 1);
    sink = 0;
    boolean nsc = (integer(0) == 1) & (integer(1) == 1);
    check("& evaluates both", !nsc && sink == 2);
    int loops = 0;
    for (int i = 0; i < 10; i++) {
      switch (i % 3) {
        case 0:
          continue;
        case 1:
          loops += 10;
          break;
        default:
          loops += 1;
      }
    }
    check("switch continue inside loop", loops == 33);
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 3; i++) {
      try {
        if (i == 1) {
          continue;
        }
        sb.append("b").append(i);
      } finally {
        sb.append("f").append(i);
      }
    }
    String trace = sb.toString();
    check("finally runs on continue", trace.equals("b0f0f1b2f2"));
    check("deep recursion 200 (frame cap is 256)", depth(integer(200)) == 200);
    check("mutual recursion", isEven(integer(101)) == false && isEven(integer(100)));
    long fact = factorial(lng(20));
    check("recursive long factorial(20)", fact == 2432902008176640000L);
    check("fib(25) iterative", fib(integer(25)) == 75025);
    int sum = 0;
    for (int v : new int[] {1, 2, 3, 4}) {
      sum += v;
    }
    check("for-each over int[]", sum == 10);
    String cat = "";
    for (String s : new String[] {"a", "b", "c"}) {
      cat += s;
    }
    check("for-each over String[]", cat.equals("abc"));
    int empty = 0;
    for (int v : new int[0]) {
      empty += v;
    }
    check("for-each over empty array", empty == 0);
  }

  static RuntimeException nullEx() {
    return null;
  }

  static int xorSelf() {
    int q = integer(77);
    int q2 = q;
    return q ^ q2;
  }

  static boolean isNaN(double d) {
    double e = d;
    return d != e;
  }

  static boolean eqd(double a, double b) {
    return a == b;
  }

  static boolean eqf(float a, float b) {
    return a == b;
  }

  static int depth(int n) {
    return n == 0 ? 0 : 1 + depth(n - 1);
  }

  static boolean isEven(int n) {
    return n == 0 ? true : isOdd(n - 1);
  }

  static boolean isOdd(int n) {
    return n == 0 ? false : isEven(n - 1);
  }

  static long factorial(long n) {
    return n <= 1 ? 1 : n * factorial(n - 1);
  }

  static int fib(int n) {
    int a = 0;
    int b = 1;
    for (int i = 0; i < n; i++) {
      int t = a + b;
      a = b;
      b = t;
    }
    return a;
  }

  // ---- evaluation order -----------------------------------------------------------------------

  static StringBuilder order;

  static int tag(String t, int v) {
    order.append(t);
    return v;
  }

  static int three(int a, int b, int c) {
    return a * 100 + b * 10 + c;
  }

  static void evalOrder() {
    order = new StringBuilder();
    int r = three(tag("a", 1), tag("b", 2), tag("c", 3));
    String o = order.toString();
    check("args evaluate left to right", o.equals("abc") && r == 123);
    order = new StringBuilder();
    int v = tag("l", 2) * tag("m", 3) + tag("r", 4);
    o = order.toString();
    check("operands left to right", o.equals("lmr") && v == 10);
    int[] arr = new int[3];
    int i = 0;
    arr[i] = i = 2;
    check("array index evaluated before rhs", arr[0] == 2 && arr[1] == 0 && i == 2);
    order = new StringBuilder();
    int[] arr2 = new int[4];
    arr2[tag("i", 1)] = tag("v", 9);
    o = order.toString();
    check("index before value", o.equals("iv") && arr2[1] == 9);
    order = new StringBuilder();
    int[][] grid = new int[tag("x", 2)][tag("y", 3)];
    o = order.toString();
    check(
        "multianewarray dims in order", o.equals("xy") && grid.length == 2 && grid[1].length == 3);
    order = new StringBuilder();
    String s = "" + tag("p", 1) + tag("q", 2);
    o = order.toString();
    check("concat order", o.equals("pq") && s.equals("12"));
    order = new StringBuilder();
    boolean x = tag("f", 0) == 1 ? tag("t", 1) == 1 : tag("e", 2) == 2;
    o = order.toString();
    check("ternary evaluates only one branch", o.equals("fe") && x);
  }

  // ---- try / finally ordering ----------------------------------------------------------------

  static int returnOverride() {
    int x = 1;
    try {
      return x;
    } finally {
      x = 99;
      sink += x;
    }
  }

  @SuppressWarnings("finally")
  static int finallyReturnWins() {
    try {
      throw new IllegalStateException("swallowed");
    } finally {
      return 7;
    }
  }

  static String nested() {
    StringBuilder sb = new StringBuilder();
    try {
      try {
        sb.append("a");
        throw new IllegalArgumentException("inner");
      } finally {
        sb.append("b");
      }
    } catch (IllegalArgumentException e) {
      sb.append("c");
    } finally {
      sb.append("d");
    }
    sb.append("e");
    return sb.toString();
  }

  static String finallyReplaces() {
    try {
      try {
        throw new IllegalArgumentException("first");
      } finally {
        throw new IllegalStateException("second");
      }
    } catch (IllegalStateException e) {
      return "second:" + e.getMessage();
    } catch (IllegalArgumentException e) {
      return "first";
    }
  }

  static int loopFinally() {
    int n = 0;
    for (int i = 0; i < 3; i++) {
      try {
        if (i == 1) {
          break;
        }
      } finally {
        n += 10;
      }
    }
    return n;
  }

  static void tryFinally() {
    sink = 0;
    check("return value captured before finally", returnOverride() == 1 && sink == 99);
    check("return in finally overrides throw", finallyReturnWins() == 7);
    check("nested try/finally order", nested().equals("abcde"));
    check("exception in finally replaces", finallyReplaces().equals("second:second"));
    check("finally on break in loop", loopFinally() == 20);
    StringBuilder sb = new StringBuilder();
    try {
      try {
        sb.append("1");
      } finally {
        sb.append("2");
      }
      sb.append("3");
    } finally {
      sb.append("4");
    }
    check("finally without exception", sb.toString().equals("1234"));
  }

  // ---- exception family -----------------------------------------------------------------------

  static class Base extends Exception {
    Base(String m) {
      super(m);
    }
  }

  static class Derived extends Base {
    Derived(String m) {
      super(m);
    }
  }

  static void exceptions() {
    String which = "";
    try {
      throw new Derived("d");
    } catch (Derived e) {
      which = "derived";
    } catch (Base e) {
      which = "base";
    }
    check("subclass catch first", which.equals("derived"));
    try {
      throw new Derived("d2");
    } catch (Base e) {
      which = "base:" + e.getMessage();
    }
    check("superclass catches subclass", which.equals("base:d2"));
    try {
      if (integer(1) == 1) {
        throw new IllegalStateException("ise");
      }
      throw new IllegalArgumentException("iae");
    } catch (IllegalArgumentException | IllegalStateException e) {
      which = "multi:" + e.getMessage();
    }
    check("multi-catch", which.equals("multi:ise"));
    try {
      throw new ArithmeticException("ae");
    } catch (RuntimeException e) {
      which = "rt";
    }
    check("RuntimeException catches ArithmeticException", which.equals("rt"));
    boolean caught = false;
    try {
      Object o = "s";
      Integer bad = (Integer) o;
      sink += bad;
    } catch (ClassCastException e) {
      caught = true;
    }
    check("ClassCastException", caught);
    caught = false;
    try {
      Object[] objs = new String[1];
      objs[0] = Integer.valueOf(1);
    } catch (ArrayStoreException e) {
      caught = true;
    }
    Log.i(
        TAG,
        "known divergence: ArrayStoreException thrown="
            + caught
            + " (reference arrays carry no element type)");
    caught = false;
    try {
      int[] neg = new int[integer(-1)];
      sink += neg.length;
    } catch (NegativeArraySizeException e) {
      caught = true;
    }
    check("NegativeArraySizeException", caught);
    caught = false;
    try {
      sink += integer(1) % integer(0);
    } catch (ArithmeticException e) {
      caught = true;
    }
    check("int % 0", caught);
    caught = false;
    try {
      sink += (int) (lng(1) / lng(0));
    } catch (ArithmeticException e) {
      caught = true;
    }
    check("long / 0", caught);
    caught = false;
    try {
      sink += (int) (lng(1) % lng(0));
    } catch (ArithmeticException e) {
      caught = true;
    }
    check("long % 0", caught);
    check("double / 0 no exception", dbl(1) / dbl(0) > 0);
    caught = false;
    try {
      String s = null;
      sink += s.length();
    } catch (NullPointerException e) {
      caught = true;
    }
    check("NPE on null receiver", caught);
    caught = false;
    try {
      int[] a = null;
      sink += a.length;
    } catch (NullPointerException e) {
      caught = true;
    }
    check("NPE on null array length", caught);
    caught = false;
    try {
      int[] a = null;
      a[0] = 1;
    } catch (NullPointerException e) {
      caught = true;
    }
    check("NPE on null array store", caught);
    caught = false;
    try {
      throw nullEx();
    } catch (NullPointerException e) {
      caught = true;
    }
    check("throw null is NPE", caught);
    caught = false;
    try {
      String sub = "abc".substring(integer(3), integer(2));
      sink += sub.length();
    } catch (StringIndexOutOfBoundsException e) {
      caught = true;
    } catch (IndexOutOfBoundsException e) {
      caught = true;
    }
    check("substring(3,2) throws", caught);
    caught = false;
    try {
      sink += "abc".charAt(integer(-1));
    } catch (IndexOutOfBoundsException e) {
      caught = true;
    }
    check("charAt(-1) throws", caught);
    caught = false;
    try {
      int[] a = new int[2];
      a[integer(2)] = 1;
    } catch (ArrayIndexOutOfBoundsException e) {
      caught = true;
    }
    check("AIOOBE at length", caught);
    caught = false;
    try {
      int[] a = new int[2];
      sink += a[integer(-1)];
    } catch (ArrayIndexOutOfBoundsException e) {
      caught = true;
    }
    check("AIOOBE at -1", caught);
    caught = false;
    try {
      long[] a = new long[1];
      a[integer(1)] = 1L;
    } catch (ArrayIndexOutOfBoundsException e) {
      caught = true;
    }
    check("AIOOBE long[]", caught);
    caught = false;
    try {
      Object[] a = new Object[1];
      a[integer(5)] = null;
    } catch (ArrayIndexOutOfBoundsException e) {
      caught = true;
    }
    check("AIOOBE Object[]", caught);
    caught = false;
    try {
      byte[] a = new byte[1];
      a[integer(1)] = 1;
    } catch (ArrayIndexOutOfBoundsException e) {
      caught = true;
    }
    check("AIOOBE byte[]", caught);
    caught = false;
    try {
      sink += Integer.parseInt("x1");
    } catch (NumberFormatException e) {
      caught = true;
    }
    check("NumberFormatException is IllegalArgumentException", caught);
    caught = false;
    try {
      sink += Integer.parseInt("");
    } catch (IllegalArgumentException e) {
      caught = true;
    }
    check("parseInt(\"\") caught as IAE", caught);
    // cause chain
    Exception cause = new IllegalStateException("root");
    Exception wrapped = new RuntimeException("wrap", cause);
    check("getCause", wrapped.getCause() == cause);
    check("getMessage", "wrap".equals(wrapped.getMessage()));
    check("no-arg message null", new RuntimeException().getMessage() == null);
    Throwable t = new Derived("dd");
    check("Throwable-typed getMessage", "dd".equals(t.getMessage()));
    check(
        "exception instanceof chain",
        t instanceof Base && t instanceof Exception && t instanceof Throwable);
    check("exception not instanceof RuntimeException", !(t instanceof RuntimeException));
    // exception from a nested call unwinds several frames
    caught = false;
    try {
      throwDeep(integer(5));
    } catch (Derived e) {
      caught = "deep".equals(e.getMessage());
    }
    check("unwind through 5 frames", caught);
    // catch and rethrow
    caught = false;
    try {
      try {
        throw new Derived("rt");
      } catch (Base e) {
        throw e;
      }
    } catch (Derived e) {
      caught = true;
    }
    check("rethrow keeps runtime type", caught);
    // exception thrown from a lambda
    caught = false;
    Section thrower =
        () -> {
          throw new IllegalStateException("lambda");
        };
    try {
      thrower.run();
    } catch (IllegalStateException e) {
      caught = "lambda".equals(e.getMessage());
    }
    check("exception through lambda", caught);
    // Error is not an Exception
    caught = false;
    boolean wrong = false;
    try {
      try {
        throw new OutOfMemoryError();
      } catch (Exception e) {
        wrong = true;
      }
    } catch (Error e) {
      caught = true;
    }
    check("Exception does not catch Error", caught && !wrong);
    // catch Throwable
    caught = false;
    try {
      throw new StackOverflowError();
    } catch (Throwable e) {
      caught = true;
    }
    check("Throwable catches Error", caught);
    // int overflow of array allocation
    caught = false;
    try {
      int[] huge = new int[Integer.MAX_VALUE];
      sink += huge.length;
    } catch (OutOfMemoryError e) {
      caught = true;
    }
    check("new int[MAX] OOM", caught);
  }

  static void throwDeep(int n) throws Derived {
    if (n == 0) {
      throw new Derived("deep");
    } else {
      throwDeep(n - 1);
    }
  }

  // ---- arrays ---------------------------------------------------------------------------------

  static void arrays() {
    int[][][] cube = new int[3][4][5];
    cube[2][3][4] = 7;
    check("3D array dims", cube.length == 3 && cube[0].length == 4 && cube[0][0].length == 5);
    check("3D array store/load", cube[2][3][4] == 7 && cube[0][0][0] == 0);
    int[][] ragged = new int[2][];
    check("ragged rows null", ragged[0] == null && ragged[1] == null);
    ragged[0] = new int[1];
    ragged[1] = new int[3];
    check("ragged lengths", ragged[0].length == 1 && ragged[1].length == 3);
    long[][] lg = new long[2][2];
    lg[1][1] = 5L;
    check("long[][]", lg[1][1] == 5L && lg[0][1] == 0L);
    String[][] sg = new String[2][2];
    check("String[][] default null", sg[1][1] == null);
    boolean[] bs = new boolean[3];
    char[] cs = new char[2];
    double[] ds = new double[2];
    float[] fs = new float[2];
    byte[] bytes = new byte[2];
    short[] shorts = new short[2];
    check(
        "primitive defaults",
        !bs[2] && cs[1] == '\0' && ds[1] == 0.0 && fs[0] == 0f && bytes[1] == 0 && shorts[0] == 0);
    Kind[] ks = new Kind[2];
    check("enum array default null", ks[0] == null);
    Object[] boxed = {1, "a", 2.5, 'c', true, 1L, 1.5f, (byte) 3, (short) 4};
    check(
        "array initializer boxes",
        boxed[0] instanceof Integer
            && boxed[1] instanceof String
            && boxed[2] instanceof Double
            && boxed[3] instanceof Character
            && boxed[4] instanceof Boolean
            && boxed[5] instanceof Long
            && boxed[6] instanceof Float
            && boxed[7] instanceof Byte
            && boxed[8] instanceof Short);
    int[] src = {1, 2, 3, 4, 5};
    int[] cl = src.clone();
    cl[0] = 9;
    check("int[].clone independent", src[0] == 1 && cl[0] == 9 && cl.length == 5);
    int[][] g = {{1, 2}, {3}};
    int[][] gc = g.clone();
    gc[0][0] = 7;
    check("int[][].clone shallow", g[0][0] == 7 && gc[1] == g[1]);
    String[] ss = {"x", "y"};
    String[] sc = ss.clone();
    check("String[].clone", sc[1].equals("y") && sc != ss);
    Object o = src;
    check("instanceof int[]", o instanceof int[]);
    check("not instanceof Object[]", !(o instanceof Object[]));
    Object os = ss;
    check("String[] instanceof Object[]", os instanceof Object[]);
    check("String[] instanceof String[]", os instanceof String[]);
    Log.i(TAG, "known divergence: String[] instanceof Integer[] = " + (os instanceof Integer[]));
    Object nul = null;
    check("null instanceof false", !(nul instanceof String) && !(nul instanceof int[]));
    String cast = (String) nul;
    check("checkcast null ok", cast == null);
    boolean cce = false;
    try {
      Object[] oa = new Object[0];
      String[] bad = (String[]) oa;
      sink += bad.length;
    } catch (ClassCastException e) {
      cce = true;
    }
    Log.i(TAG, "known divergence: (String[]) Object[] threw=" + cce);
    Object[] up = new String[1];
    check("String[] as Object[] element store of String", (up[0] = "s") != null);
    check("array getClass same", src.getClass() == cl.getClass());
    Object srcObj = src;
    Object srcObj2 = src;
    check("array hashCode stable", srcObj.hashCode() == srcObj2.hashCode());
    check("array equals identity", srcObj.equals(src) && !srcObj.equals(cl));
    check("array length after new", new byte[integer(1000)].length == 1000);
    char[] hello = {'h', 'i'};
    check(
        "char[] to String", new String(new byte[] {(byte) hello[0], (byte) hello[1]}).equals("hi"));
    Runnable[] rs = new Runnable[2];
    rs[0] = () -> sink++;
    sink = 0;
    rs[0].run();
    check("interface array element lambda", sink == 1 && rs[1] == null);
    int[] big = new int[5000];
    for (int i = 0; i < big.length; i++) {
      big[i] = i;
    }
    long total = 0;
    for (int i = 0; i < big.length; i++) {
      total += big[i];
    }
    check("5000-element int[] sum", total == 12497500L);
    System.arraycopy(big, 1, big, 0, 4999);
    check("arraycopy overlap shift", big[0] == 1 && big[4998] == 4999 && big[4999] == 4999);
    boolean ase = false;
    try {
      System.arraycopy(new String[] {"s"}, 0, new Integer[1], 0, 1);
    } catch (ArrayStoreException e) {
      ase = true;
    }
    Log.i(TAG, "known divergence: arraycopy type mismatch threw=" + ase);
    boolean aio = false;
    try {
      System.arraycopy(src, 0, new int[2], 0, 3);
    } catch (IndexOutOfBoundsException e) {
      aio = true;
    }
    check("arraycopy overflow IOOBE", aio);
    boolean npe = false;
    try {
      System.arraycopy(null, 0, src, 0, 1);
    } catch (NullPointerException e) {
      npe = true;
    }
    check("arraycopy null NPE", npe);
  }

  // ---- wide locals (more than 255 slots) -------------------------------------------------------

  static void wideLocals() {
    long l0 = lng(0), l1 = 1, l2 = 2, l3 = 3, l4 = 4, l5 = 5, l6 = 6, l7 = 7, l8 = 8, l9 = 9;
    long l10 = 10,
        l11 = 11,
        l12 = 12,
        l13 = 13,
        l14 = 14,
        l15 = 15,
        l16 = 16,
        l17 = 17,
        l18 = 18,
        l19 = 19;
    long l20 = 20,
        l21 = 21,
        l22 = 22,
        l23 = 23,
        l24 = 24,
        l25 = 25,
        l26 = 26,
        l27 = 27,
        l28 = 28,
        l29 = 29;
    long l30 = 30,
        l31 = 31,
        l32 = 32,
        l33 = 33,
        l34 = 34,
        l35 = 35,
        l36 = 36,
        l37 = 37,
        l38 = 38,
        l39 = 39;
    long l40 = 40,
        l41 = 41,
        l42 = 42,
        l43 = 43,
        l44 = 44,
        l45 = 45,
        l46 = 46,
        l47 = 47,
        l48 = 48,
        l49 = 49;
    long l50 = 50,
        l51 = 51,
        l52 = 52,
        l53 = 53,
        l54 = 54,
        l55 = 55,
        l56 = 56,
        l57 = 57,
        l58 = 58,
        l59 = 59;
    long l60 = 60,
        l61 = 61,
        l62 = 62,
        l63 = 63,
        l64 = 64,
        l65 = 65,
        l66 = 66,
        l67 = 67,
        l68 = 68,
        l69 = 69;
    long l70 = 70,
        l71 = 71,
        l72 = 72,
        l73 = 73,
        l74 = 74,
        l75 = 75,
        l76 = 76,
        l77 = 77,
        l78 = 78,
        l79 = 79;
    long l80 = 80,
        l81 = 81,
        l82 = 82,
        l83 = 83,
        l84 = 84,
        l85 = 85,
        l86 = 86,
        l87 = 87,
        l88 = 88,
        l89 = 89;
    long l90 = 90,
        l91 = 91,
        l92 = 92,
        l93 = 93,
        l94 = 94,
        l95 = 95,
        l96 = 96,
        l97 = 97,
        l98 = 98,
        l99 = 99;
    long l100 = 100,
        l101 = 101,
        l102 = 102,
        l103 = 103,
        l104 = 104,
        l105 = 105,
        l106 = 106,
        l107 = 107;
    long l108 = 108,
        l109 = 109,
        l110 = 110,
        l111 = 111,
        l112 = 112,
        l113 = 113,
        l114 = 114,
        l115 = 115;
    long l116 = 116,
        l117 = 117,
        l118 = 118,
        l119 = 119,
        l120 = 120,
        l121 = 121,
        l122 = 122,
        l123 = 123;
    long l124 = 124,
        l125 = 125,
        l126 = 126,
        l127 = 127,
        l128 = 128,
        l129 = 129,
        l130 = 130,
        l131 = 131;
    double d0 = dbl(0.5), d1 = 1.5, d2 = 2.5, d3 = 3.5;
    int tail = integer(1000);
    tail += 7;
    long sum =
        l0 + l1 + l2 + l3 + l4 + l5 + l6 + l7 + l8 + l9 + l10 + l11 + l12 + l13 + l14 + l15 + l16
            + l17 + l18 + l19 + l20 + l21 + l22 + l23 + l24 + l25 + l26 + l27 + l28 + l29 + l30
            + l31 + l32 + l33 + l34 + l35 + l36 + l37 + l38 + l39 + l40 + l41 + l42 + l43 + l44
            + l45 + l46 + l47 + l48 + l49 + l50 + l51 + l52 + l53 + l54 + l55 + l56 + l57 + l58
            + l59 + l60 + l61 + l62 + l63 + l64 + l65 + l66 + l67 + l68 + l69 + l70 + l71 + l72
            + l73 + l74 + l75 + l76 + l77 + l78 + l79 + l80 + l81 + l82 + l83 + l84 + l85 + l86
            + l87 + l88 + l89 + l90 + l91 + l92 + l93 + l94 + l95 + l96 + l97 + l98 + l99 + l100
            + l101 + l102 + l103 + l104 + l105 + l106 + l107 + l108 + l109 + l110 + l111 + l112
            + l113 + l114 + l115 + l116 + l117 + l118 + l119 + l120 + l121 + l122 + l123 + l124
            + l125 + l126 + l127 + l128 + l129 + l130 + l131;
    check("wide long locals sum", sum == 8646L);
    check("wide double locals", d0 + d1 + d2 + d3 == 8.0);
    check("wide int local iinc", tail == 1007);
    l131 += tail;
    check("wide long store", l131 == 1138L);
  }
}
