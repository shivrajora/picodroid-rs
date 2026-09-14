// SPDX-License-Identifier: GPL-3.0-only
package qa_coll;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Random;
import picodroid.app.Application;
import picodroid.os.Runtime;
import picodroid.util.Log;

/**
 * QA 2026-09-13: the served java.util / java.lang surface under load and at its edges — identity
 * hash codes across a compacting GC, HashMap growth and views, ArrayList growth, boxed keys of
 * different types, string edge cases, String.format corner cases, interleaved StringBuilders,
 * wrapper parsing and printing, and java.util.Random's exact stream.
 */
public class QaColl extends Application {
  private static final String TAG = "QaColl";

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

  static void checkEq(String name, String expected, String actual) {
    if (expected.equals(actual)) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name + " expected [" + expected + "] got [" + actual + "]");
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
    Log.i(TAG, "=== QaColl start ===");
    section("identityAcrossGc", () -> identityAcrossGc());
    section("hashMapGrowth", () -> hashMapGrowth());
    section("mapViews", () -> mapViews());
    section("boxedKeys", () -> boxedKeys());
    section("arrayLists", () -> arrayLists());
    section("sets", () -> sets());
    section("arraysUtil", () -> arraysUtil());
    section("strings", () -> strings());
    section("formatting", () -> formatting());
    section("builders", () -> builders());
    section("wrappers", () -> wrappers());
    section("random", () -> random());
    section("oomRecovery", () -> oomRecovery());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  static class Node {
    final int id;
    Node next;

    Node(int id) {
      this.id = id;
    }
  }

  static void churn(int rounds) {
    for (int i = 0; i < rounds; i++) {
      byte[] junk = new byte[512];
      junk[0] = (byte) i;
      sink += junk[0];
      String s = "garbage-" + i;
      sink += s.length();
    }
  }

  // ---- identity hash codes must survive a compacting collection -----------------------------

  static void identityAcrossGc() {
    int n = 150;
    Node[] keep = new Node[n];
    int[] hashes = new int[n];
    HashMap<Node, Integer> byIdentity = new HashMap<>();
    HashSet<Object> idSet = new HashSet<>();
    for (int i = 0; i < n; i++) {
      churn(3); // interleave garbage so survivors move when the heap compacts
      keep[i] = new Node(i);
      hashes[i] = keep[i].hashCode();
      byIdentity.put(keep[i], i);
      idSet.add(keep[i]);
    }
    boolean distinctish = true;
    int dup = 0;
    for (int i = 1; i < n; i++) {
      if (hashes[i] == hashes[0]) {
        dup++;
      }
    }
    if (dup > n / 4) {
      distinctish = false;
    }
    check("identity hashes not all equal", distinctish);
    int before = Runtime.gcCount();
    churn(2000);
    int after = Runtime.gcCount();
    check("garbage churn ran collections", after > before);
    boolean stable = true;
    for (int i = 0; i < n; i++) {
      if (keep[i].hashCode() != hashes[i]) {
        stable = false;
      }
    }
    check("identity hashCode stable across GC", stable);
    boolean found = true;
    for (int i = 0; i < n; i++) {
      Integer v = byIdentity.get(keep[i]);
      if (v == null || v != i) {
        found = false;
      }
    }
    check("identity-keyed HashMap lookup after GC", found);
    boolean inSet = true;
    for (int i = 0; i < n; i++) {
      if (!idSet.contains(keep[i])) {
        inSet = false;
      }
    }
    check("identity HashSet contains after GC", inSet && idSet.size() == n);
    check("identity map size", byIdentity.size() == n);
    // objects that died must not disturb lookups of survivors after more churn
    for (int i = 0; i < n; i += 2) {
      byIdentity.remove(keep[i]);
    }
    churn(1500);
    boolean half = byIdentity.size() == n / 2;
    for (int i = 1; i < n; i += 2) {
      if (byIdentity.get(keep[i]) != i) {
        half = false;
      }
    }
    check("survivors found after removals + GC", half);
    // linked structure survives
    Node head = new Node(-1);
    Node cur = head;
    for (int i = 0; i < 300; i++) {
      churn(1);
      cur.next = new Node(i);
      cur = cur.next;
    }
    churn(1500);
    int walked = 0;
    for (Node p = head.next; p != null; p = p.next) {
      if (p.id != walked) {
        walked = -1;
        break;
      }
      walked++;
    }
    check("300-node linked list intact after GC", walked == 300);
  }

  // ---- HashMap growth ------------------------------------------------------------------------

  static void hashMapGrowth() {
    HashMap<Integer, Integer> m = new HashMap<>();
    for (int i = 0; i < 300; i++) {
      m.put(i, i * 3);
    }
    check("300 entries size", m.size() == 300);
    boolean ok = true;
    for (int i = 0; i < 300; i++) {
      Integer v = m.get(i);
      if (v == null || v != i * 3) {
        ok = false;
        break;
      }
    }
    check("300 entries all found", ok);
    check("absent key null", m.get(300) == null && m.get(-1) == null);
    for (int i = 0; i < 300; i += 2) {
      Integer old = m.remove(i);
      if (old == null || old != i * 3) {
        ok = false;
      }
    }
    check("remove returns old value", ok && m.size() == 150);
    int cnt = 0;
    long sum = 0;
    for (Map.Entry<Integer, Integer> e : m.entrySet()) {
      cnt++;
      sum += e.getKey() + e.getValue();
    }
    check("entrySet after removals", cnt == 150 && sum == 90000L);
    check("put replaces and returns old", m.put(1, -1) == 3 && m.get(1) == -1 && m.size() == 150);
    m.clear();
    check("clear", m.isEmpty() && m.size() == 0 && m.get(1) == null);
    m.put(5, 5);
    check("reuse after clear", m.size() == 1 && m.get(5) == 5);
    HashMap<String, Integer> sm = new HashMap<>();
    for (int i = 0; i < 500; i++) {
      sm.put("key-" + i, i);
    }
    ok = true;
    for (int i = 0; i < 500; i++) {
      if (sm.get("key-" + i) != i) {
        ok = false;
      }
    }
    check("dynamic string keys", ok && sm.size() == 500 && sm.get("key-42") == 42);
    check("literal lookup of dynamic key", sm.containsKey("key-499") && !sm.containsKey("key-500"));
    HashMap<String, String> nulls = new HashMap<>();
    nulls.put("k", null);
    check(
        "null value: containsKey true, get null",
        nulls.containsKey("k") && nulls.get("k") == null && nulls.size() == 1);
    check("getOrDefault with present null", nulls.getOrDefault("k", "d") == null);
    check("getOrDefault absent", "d".equals(nulls.getOrDefault("zz", "d")));
    nulls.put(null, "nv");
    check("null key", nulls.containsKey(null) && "nv".equals(nulls.get(null)) && nulls.size() == 2);
    check(
        "containsValue null",
        nulls.containsValue(null) && nulls.containsValue("nv") && !nulls.containsValue("x"));
    check("remove null key", "nv".equals(nulls.remove(null)) && nulls.size() == 1);
    HashMap<Integer, String> sized = new HashMap<>(4);
    for (int i = 0; i < 100; i++) {
      sized.put(i, "v" + i);
    }
    check("small initial capacity grows", sized.size() == 100 && "v77".equals(sized.get(77)));
    HashMap<Integer, String> lf = new HashMap<>(16, 0.5f);
    lf.put(1, "a");
    check("load-factor ctor", "a".equals(lf.get(1)));
  }

  // ---- map views ------------------------------------------------------------------------------

  static void mapViews() {
    HashMap<String, Integer> m = new HashMap<>();
    for (int i = 0; i < 10; i++) {
      m.put("k" + i, i);
    }
    check(
        "keySet size",
        m.keySet().size() == 10 && m.values().size() == 10 && m.entrySet().size() == 10);
    check("keySet contains", m.keySet().contains("k3") && !m.keySet().contains("k10"));
    check("values contains", m.values().contains(7) && !m.values().contains(70));
    int keys = 0;
    for (String k : m.keySet()) {
      keys += k.length();
    }
    check("keySet iteration", keys == 20);
    int vals = 0;
    for (Integer v : m.values()) {
      vals += v;
    }
    check("values iteration", vals == 45);
    Iterator<String> it = m.keySet().iterator();
    int removed = 0;
    while (it.hasNext()) {
      String k = it.next();
      if (k.endsWith("1") || k.endsWith("2")) {
        it.remove();
        removed++;
      }
    }
    check("keySet iterator remove", removed == 2 && m.size() == 8 && !m.containsKey("k1"));
    Iterator<Map.Entry<String, Integer>> eit = m.entrySet().iterator();
    while (eit.hasNext()) {
      Map.Entry<String, Integer> e = eit.next();
      if (e.getValue() == 5) {
        eit.remove();
      }
    }
    check("entrySet iterator remove", m.size() == 7 && !m.containsKey("k5"));
    Iterator<Integer> vit = m.values().iterator();
    vit.next();
    vit.remove();
    check("values iterator remove", m.size() == 6);
    boolean cme = false;
    try {
      for (String k : m.keySet()) {
        if (k.equals("k9")) {
          m.put("new", 1);
        }
      }
    } catch (java.util.ConcurrentModificationException e) {
      cme = true;
    }
    check("CME on put during keySet iteration", cme || m.size() == 7);
    HashMap<String, Integer> empty = new HashMap<>();
    check(
        "empty views",
        !empty.keySet().iterator().hasNext()
            && !empty.values().iterator().hasNext()
            && empty.entrySet().size() == 0);
    boolean nse = false;
    try {
      empty.keySet().iterator().next();
    } catch (java.util.NoSuchElementException e) {
      nse = true;
    }
    check("next() on empty throws NoSuchElementException", nse);
    for (Map.Entry<String, Integer> e : m.entrySet()) {
      check("entry key/value consistent", m.get(e.getKey()).equals(e.getValue()));
      break;
    }
  }

  // ---- boxed keys of different types --------------------------------------------------------

  static void boxedKeys() {
    HashMap<Object, String> m = new HashMap<>();
    m.put(1, "int");
    m.put(1L, "long");
    m.put(1.0, "double");
    m.put(1.0f, "float");
    m.put("1", "string");
    m.put('1', "char");
    m.put(true, "bool");
    m.put((short) 1, "short");
    m.put((byte) 1, "byte");
    check("nine distinct boxed keys", m.size() == 9);
    check("Integer key", "int".equals(m.get(1)));
    check("Long key", "long".equals(m.get(1L)) && !"long".equals(m.get(1)));
    check("Double key", "double".equals(m.get(1.0)));
    check("Float key", "float".equals(m.get(1.0f)));
    check("String key", "string".equals(m.get("1")));
    check("Character key", "char".equals(m.get('1')));
    check("Boolean key", "bool".equals(m.get(true)) && m.get(false) == null);
    check("Short key", "short".equals(m.get((short) 1)));
    check("Byte key", "byte".equals(m.get((byte) 1)));
    check("Integer.equals(Long) false", !Integer.valueOf(1).equals(Long.valueOf(1)));
    check("Long.equals(Integer) false", !Long.valueOf(1).equals(Integer.valueOf(1)));
    check("Double.equals NaN true", Double.valueOf(Double.NaN).equals(Double.valueOf(Double.NaN)));
    check("Double.equals 0.0/-0.0 false", !Double.valueOf(0.0).equals(Double.valueOf(-0.0)));
    check("Float.equals NaN true", Float.valueOf(Float.NaN).equals(Float.valueOf(Float.NaN)));
    HashMap<Double, String> dm = new HashMap<>();
    dm.put(Double.NaN, "nan");
    dm.put(-0.0, "negzero");
    dm.put(0.0, "zero");
    check(
        "NaN and signed zeros as keys",
        dm.size() == 3 && "nan".equals(dm.get(Double.NaN)) && "negzero".equals(dm.get(-0.0)));
    HashMap<Long, Integer> lm = new HashMap<>();
    lm.put(1L << 40, 1);
    lm.put(-1L, 2);
    lm.put(Long.MIN_VALUE, 3);
    check(
        "big long keys",
        lm.get(1L << 40) == 1
            && lm.get(-1L) == 2
            && lm.get(Long.MIN_VALUE) == 3
            && lm.get(0L) == null);
    check(
        "Long.hashCode",
        Long.valueOf(1L << 40).hashCode() == 256 && Long.valueOf(-1L).hashCode() == 0);
    check("Double.hashCode(1.5)", Double.valueOf(1.5).hashCode() == 1073217536);
    check(
        "Boolean.hashCode",
        Boolean.valueOf(true).hashCode() == 1231 && Boolean.valueOf(false).hashCode() == 1237);
    check("Character.hashCode", Character.valueOf('a').hashCode() == 97);
    check("Integer.hashCode", Integer.valueOf(-5).hashCode() == -5);
    check("String.hashCode", "hello".hashCode() == 99162322 && "".hashCode() == 0);
    String longish = "The quick brown fox jumps over the lazy dog 0123456789";
    String tripled = longish + longish + longish;
    check("String.hashCode overflow wraps", tripled.hashCode() == -1785716650);
    check("hash collision strings", "Aa".hashCode() == "BB".hashCode() && !"Aa".equals("BB"));
    HashMap<String, Integer> cm = new HashMap<>();
    cm.put("Aa", 1);
    cm.put("BB", 2);
    cm.put("AaAa", 3);
    cm.put("BBBB", 4);
    cm.put("AaBB", 5);
    check(
        "colliding string keys distinct",
        cm.size() == 5 && cm.get("BB") == 2 && cm.get("AaBB") == 5 && cm.get("BBAa") == null);
  }

  // ---- ArrayList ------------------------------------------------------------------------------

  static void arrayLists() {
    ArrayList<Integer> l = new ArrayList<>();
    for (int i = 0; i < 1000; i++) {
      l.add(i);
    }
    check("1000 adds", l.size() == 1000 && l.get(999) == 999);
    long sum = 0;
    for (int v : l) {
      sum += v;
    }
    check("for-each sum", sum == 499500L);
    l.add(0, -1);
    check("add(0) shifts", l.get(0) == -1 && l.get(1) == 0 && l.size() == 1001);
    l.add(l.size(), 9999);
    check("add(size) appends", l.get(1001) == 9999);
    Integer old = l.set(0, 100);
    check("set returns old", old == -1 && l.get(0) == 100);
    Integer rem = l.remove(0);
    check("remove(int) returns element", rem == 100 && l.get(0) == 0);
    boolean removedObj = l.remove(Integer.valueOf(9999));
    check("remove(Object) boxed", removedObj && l.size() == 1000 && !l.contains(9999));
    check("remove(Object) absent", !l.remove(Integer.valueOf(123456)));
    int idx = 0;
    Iterator<Integer> it = l.iterator();
    while (it.hasNext()) {
      int v = it.next();
      if (v % 2 == 1) {
        it.remove();
      }
      idx++;
    }
    check("iterator remove odds", idx == 1000 && l.size() == 500 && l.get(1) == 2);
    boolean cme = false;
    try {
      for (Integer v : l) {
        if (v == 4) {
          l.add(1);
        }
      }
    } catch (java.util.ConcurrentModificationException e) {
      cme = true;
    }
    check("CME on add during for-each", cme);
    l.clear();
    check("clear", l.isEmpty() && l.size() == 0);
    l.add(null);
    check("add null", l.size() == 1 && l.get(0) == null && l.contains(null));
    check("remove(null)", l.remove(null) && l.isEmpty());
    ArrayList<String> s = new ArrayList<>(2);
    s.add("b");
    s.add("a");
    s.add("c");
    Object[] arr = s.toArray();
    check("toArray Object[]", arr.length == 3 && "b".equals(arr[0]));
    Collections.sort(s);
    check("Collections.sort strings", s.get(0).equals("a") && s.get(2).equals("c"));
    Collections.reverse(s);
    check("reverse odd size", s.get(0).equals("c") && s.get(1).equals("b") && s.get(2).equals("a"));
    s.add("d");
    Collections.reverse(s);
    check("reverse even size", s.get(0).equals("d") && s.get(3).equals("c"));
    ArrayList<ArrayList<Integer>> nested = new ArrayList<>();
    for (int i = 0; i < 10; i++) {
      ArrayList<Integer> row = new ArrayList<>();
      for (int j = 0; j <= i; j++) {
        row.add(j);
      }
      nested.add(row);
    }
    check("nested lists", nested.get(9).size() == 10 && nested.get(4).get(4) == 4);
    boolean ioobe = false;
    try {
      s.get(4);
    } catch (IndexOutOfBoundsException e) {
      ioobe = true;
    }
    check("get(size) throws", ioobe);
    ioobe = false;
    try {
      s.add(6, "x");
    } catch (IndexOutOfBoundsException e) {
      ioobe = true;
    }
    check("add(size+2) throws", ioobe);
    ArrayList<Integer> nums = new ArrayList<>();
    Random r = new Random(1);
    for (int i = 0; i < 500; i++) {
      nums.add(r.nextInt(1000) - 500);
    }
    Collections.sort(nums);
    boolean ordered = true;
    for (int i = 1; i < nums.size(); i++) {
      if (nums.get(i - 1) > nums.get(i)) {
        ordered = false;
      }
    }
    check("sort 500 random ints", ordered && nums.size() == 500);
    ArrayList<Object> mixed = new ArrayList<>();
    mixed.add("s");
    mixed.add(1);
    mixed.add(null);
    mixed.add(2.5);
    check(
        "mixed list contains",
        mixed.contains(1) && mixed.contains(2.5) && mixed.contains(null) && !mixed.contains(1L));
  }

  // ---- HashSet --------------------------------------------------------------------------------

  static void sets() {
    HashSet<Integer> s = new HashSet<>();
    boolean allNew = true;
    for (int i = 0; i < 1000; i++) {
      if (!s.add(i)) {
        allNew = false;
      }
    }
    check("1000 adds new", allNew && s.size() == 1000);
    check("add duplicate false", !s.add(500) && s.size() == 1000);
    check("remove present/absent", s.remove(500) && !s.remove(500) && s.size() == 999);
    int cnt = 0;
    for (Integer v : s) {
      cnt++;
      sink += v;
    }
    check("iteration count", cnt == 999);
    Iterator<Integer> it = s.iterator();
    while (it.hasNext()) {
      if (it.next() < 900) {
        it.remove();
      }
    }
    check("iterator remove", s.size() == 100 && s.contains(950) && !s.contains(100));
    HashSet<String> ss = new HashSet<>();
    ss.add("a" + "b");
    ss.add(new String(new byte[] {'a', 'b'}));
    check("string set dedups by content", ss.size() == 1 && ss.contains("ab"));
    HashSet<Object> mixed = new HashSet<>();
    mixed.add(1);
    mixed.add(1L);
    mixed.add("1");
    mixed.add(null);
    check(
        "mixed types distinct + null",
        mixed.size() == 4 && mixed.contains(null) && mixed.contains(1L));
    check("remove null from set", mixed.remove(null) && mixed.size() == 3);
    mixed.clear();
    check("set clear", mixed.isEmpty() && !mixed.contains(1));
    HashSet<Integer> sized = new HashSet<>(2);
    for (int i = 0; i < 64; i++) {
      sized.add(i * i);
    }
    check("tiny initial capacity grows", sized.size() == 64 && sized.contains(49 * 49));
    HashSet<Integer> lf = new HashSet<>(8, 0.75f);
    lf.add(3);
    check("set load factor ctor", lf.contains(3));
  }

  // ---- Arrays utilities -----------------------------------------------------------------------

  static void arraysUtil() {
    long[] ls = {5, -1, Long.MIN_VALUE, 3, Long.MAX_VALUE, 0};
    Arrays.sort(ls);
    check("sort long[]", ls[0] == Long.MIN_VALUE && ls[1] == -1 && ls[5] == Long.MAX_VALUE);
    double[] ds = {
      1.5, Double.NaN, -0.0, 0.0, -3.25, Double.NEGATIVE_INFINITY, Double.POSITIVE_INFINITY
    };
    Arrays.sort(ds);
    boolean nanLast = Double.compare(ds[6], Double.NaN) == 0;
    check("sort double[] NaN last", nanLast);
    check(
        "sort double[] -0.0 before 0.0",
        ds[0] == Double.NEGATIVE_INFINITY && ds[1] == -3.25 && 1 / ds[2] < 0 && 1 / ds[3] > 0);
    float[] fs = {2f, -1f, Float.NaN, 0f};
    Arrays.sort(fs);
    check(
        "sort float[]",
        fs[0] == -1f && fs[1] == 0f && fs[2] == 2f && Float.compare(fs[3], Float.NaN) == 0);
    char[] cs = {'z', 'a', 'M', '0'};
    Arrays.sort(cs);
    check("sort char[]", cs[0] == '0' && cs[1] == 'M' && cs[2] == 'a' && cs[3] == 'z');
    byte[] bs = {5, -128, 127, 0};
    Arrays.sort(bs);
    check("sort byte[]", bs[0] == -128 && bs[3] == 127);
    short[] shs = {300, -300, 0};
    Arrays.sort(shs);
    check("sort short[]", shs[0] == -300 && shs[2] == 300);
    int[] is = new int[1000];
    Random r = new Random(9);
    for (int i = 0; i < is.length; i++) {
      is[i] = r.nextInt();
    }
    Arrays.sort(is);
    boolean ordered = true;
    for (int i = 1; i < is.length; i++) {
      if (is[i - 1] > is[i]) {
        ordered = false;
      }
    }
    check("sort 1000 random ints", ordered);
    String[] ss = {"b", "B", "a", "A", "aa", ""};
    Arrays.sort(ss);
    checkEq("sort String[] lexicographic", "[, A, B, a, aa, b]", Arrays.toString(ss));
    int[] shrink = Arrays.copyOf(new int[] {1, 2, 3}, 2);
    check("copyOf shrink", shrink.length == 2 && shrink[1] == 2);
    int[] zero = Arrays.copyOf(new int[] {1}, 0);
    check("copyOf zero", zero.length == 0);
    long[] lg = new long[3];
    Arrays.fill(lg, -7L);
    check("fill long[]", lg[0] == -7L && lg[2] == -7L);
    checkEq("toString int[]", "[1, 2, 3]", Arrays.toString(new int[] {1, 2, 3}));
    checkEq("toString empty", "[]", Arrays.toString(new int[0]));
    checkEq("toString null", "null", Arrays.toString((int[]) null));
    checkEq(
        "toString float[]",
        "[1.5, -0.0, NaN]",
        Arrays.toString(new float[] {1.5f, -0.0f, Float.NaN}));
    checkEq("toString double[]", "[1.0E10, 0.001]", Arrays.toString(new double[] {1e10, 0.001}));
    checkEq("toString char[]", "[a, b]", Arrays.toString(new char[] {'a', 'b'}));
    checkEq("toString boolean[]", "[true, false]", Arrays.toString(new boolean[] {true, false}));
    checkEq(
        "toString long[]", "[-9223372036854775808]", Arrays.toString(new long[] {Long.MIN_VALUE}));
    checkEq(
        "toString Object[] nested", "[a, null, 1]", Arrays.toString(new Object[] {"a", null, 1}));
    Integer[] boxed = {3, 1, 2};
    Arrays.sort(boxed);
    check("sort Integer[] natural", boxed[0] == 1 && boxed[2] == 3);
    Arrays.sort(boxed, (a, b) -> b - a);
    check("sort Integer[] comparator", boxed[0] == 3 && boxed[2] == 1);
    Arrays.sort(boxed, null);
    check("sort Object[] null comparator", boxed[0] == 1);
    String[] one = {"x"};
    Arrays.sort(one);
    String[] none = {};
    Arrays.sort(none);
    check("sort size 1 and 0", one[0].equals("x") && none.length == 0);
    boolean npe = false;
    try {
      Arrays.sort((int[]) null);
    } catch (NullPointerException e) {
      npe = true;
    }
    check("sort(null) NPE", npe);
  }

  // ---- String edge cases ----------------------------------------------------------------------

  static String dyn(String s) {
    sink++;
    return s + "";
  }

  static void strings() {
    String[] parts = dyn("a,b,,c,").split(",");
    check(
        "split drops trailing empties",
        parts.length == 4 && parts[2].equals("") && parts[3].equals("c"));
    parts = dyn(",a").split(",");
    check("split keeps leading empty", parts.length == 2 && parts[0].equals(""));
    parts = dyn("abc").split(",");
    check("split no match", parts.length == 1 && parts[0].equals("abc"));
    parts = dyn("").split(",");
    check("split empty string", parts.length == 1 && parts[0].equals(""));
    parts = dyn("a::b::c").split("::");
    check("split multi-char", parts.length == 3 && parts[1].equals("b"));
    parts = dyn(",,,").split(",");
    check("split all separators", parts.length == 0);
    check("substring(len)", dyn("abc").substring(3).equals(""));
    check("substring(i,i)", dyn("abc").substring(1, 1).equals(""));
    check("substring full", dyn("abc").substring(0, 3).equals("abc"));
    check("indexOf empty", dyn("abc").indexOf("") == 0);
    check("lastIndexOf empty", dyn("abc").lastIndexOf("") == 3);
    check("indexOf past end", dyn("abc").indexOf("c", 5) == -1);
    check("indexOf negative from", dyn("abc").indexOf("a", -3) == 0);
    check("lastIndexOf char from", dyn("abcabc").lastIndexOf('b', 3) == 1);
    check("indexOf char not found", dyn("abc").indexOf('z') == -1);
    check("contains empty", dyn("abc").contains(""));
    check("startsWith/endsWith empty", dyn("abc").startsWith("") && dyn("abc").endsWith(""));
    check("startsWith longer", !dyn("ab").startsWith("abc"));
    check("endsWith longer", !dyn("ab").endsWith("abc"));
    checkEq("replace empty target", "-a-b-", dyn("ab").replace("", "-"));
    checkEq("replace char same", "aaa", dyn("aaa").replace('a', 'a'));
    checkEq("replace all occurrences", "xbxbx", dyn("ababa").replace("a", "x"));
    checkEq("replace longer", "[a][a]", dyn("aa").replace("a", "[a]"));
    checkEq("replace to empty", "b", dyn("aba").replace("a", ""));
    checkEq("trim all spaces", "", dyn("    ").trim());
    checkEq("trim tabs and newlines", "x", dyn("\t\n x \r\n").trim());
    checkEq("toUpperCase mixed", "ABC123-Z", dyn("abc123-z").toUpperCase());
    checkEq("toLowerCase mixed", "abc123-z", dyn("ABC123-Z").toLowerCase());
    check(
        "compareTo shorter prefix",
        dyn("abc").compareTo("ab") == 1 && dyn("ab").compareTo("abc") == -1);
    check("compareTo empty", dyn("").compareTo("a") == -1 && dyn("a").compareTo("") == 1);
    check("compareTo char diff", dyn("a").compareTo("c") == -2 && dyn("B").compareTo("a") == -31);
    check("compareTo equal", dyn("same").compareTo("same") == 0);
    check("equalsIgnoreCase null", !dyn("a").equalsIgnoreCase(null));
    check(
        "equalsIgnoreCase lengths",
        !dyn("ab").equalsIgnoreCase("abc") && dyn("aBc").equalsIgnoreCase("AbC"));
    Object nulObj = null;
    check("equals null", !dyn("a").equals(nulObj));
    check("isEmpty", dyn("").isEmpty() && !dyn(" ").isEmpty());
    check("length unicode bytes", dyn("é").length() == 2);
    checkEq("concat", "ab", dyn("a").concat("b"));
    checkEq("concat empty", "a", dyn("a").concat(""));
    ArrayList<String> joinList = new ArrayList<>();
    checkEq("join empty list", "", String.join(",", joinList));
    joinList.add("x");
    joinList.add(null);
    joinList.add("z");
    checkEq("join with null", "x,null,z", String.join(",", joinList));
    checkEq("join String[]", "a|b", String.join("|", new String[] {"a", "b"}));
    checkEq("join empty separator", "ab", String.join("", "a", "b"));
    check("toCharArray", dyn("hi").toCharArray().length == 2 && dyn("hi").toCharArray()[1] == 'i');
    check("toCharArray empty", dyn("").toCharArray().length == 0);
    byte[] bytes = dyn("héllo").getBytes();
    check("getBytes utf8 length", bytes.length == 6);
    // Strings are byte-backed ASCII: a non-ASCII byte round-trips as '?' (compatibility matrix).
    Log.i(TAG, "known divergence: new String(utf8 bytes) = " + new String(bytes));
    checkEq("new String(bytes, off, len)", "llo", new String(bytes, 3, 3));
    checkEq("valueOf char", "\n", String.valueOf('\n'));
    checkEq("valueOf null object", "null", String.valueOf((Object) null));
    checkEq("valueOf boolean", "false", String.valueOf(false));
    checkEq("valueOf long MIN", "-9223372036854775808", String.valueOf(Long.MIN_VALUE));
    checkEq("valueOf float", "0.1", String.valueOf(0.1f));
    checkEq("valueOf double small", "1.0E-4", String.valueOf(1e-4));
    checkEq("valueOf double 0.001", "0.001", String.valueOf(0.001));
    checkEq("valueOf double 1e7", "1.0E7", String.valueOf(1e7));
    checkEq("valueOf double 9999999", "9999999.0", String.valueOf(9999999.0));
    checkEq("valueOf double 123456789", "1.23456789E8", String.valueOf(123456789.0));
    checkEq("valueOf -0.0", "-0.0", String.valueOf(-0.0));
    checkEq("valueOf NaN", "NaN", String.valueOf(0.0 / 0.0));
    checkEq("valueOf -Inf", "-Infinity", String.valueOf(-1.0 / 0.0));
    // Java prints the subnormal minimum as 4.9E-324; the shortest round-trip form is 5.0E-324.
    Log.i(TAG, "known divergence: valueOf(Double.MIN_VALUE) = " + String.valueOf(Double.MIN_VALUE));
    checkEq("valueOf MAX_VALUE", "1.7976931348623157E308", String.valueOf(Double.MAX_VALUE));
    checkEq("valueOf 100.0", "100.0", String.valueOf(100.0));
    checkEq("valueOf 1e21", "1.0E21", String.valueOf(1e21));
    checkEq("valueOf float 1e-5", "1.0E-5", String.valueOf(1e-5f));
    checkEq("valueOf float 1234567", "1234567.0", String.valueOf(1234567f));
    checkEq("valueOf float 1e10", "1.0E10", String.valueOf(1e10f));
    checkEq("valueOf float 1/3", "0.33333334", String.valueOf(1f / 3f));
    checkEq("valueOf double 1/3", "0.3333333333333333", String.valueOf(1.0 / 3.0));
    checkEq("valueOf 2/3", "0.6666666666666666", String.valueOf(2.0 / 3.0));
    checkEq("valueOf 1e16", "1.0E16", String.valueOf(1e16));
    checkEq("valueOf 12345.6789", "12345.6789", String.valueOf(12345.6789));
    String big = "";
    for (int i = 0; i < 200; i++) {
      big += (char) ('a' + (i % 26));
    }
    check("200-char concat loop", big.length() == 200 && big.charAt(199) == 'r');
    check("long string indexOf", big.indexOf("yzab") == 24 && big.lastIndexOf("yzab") == 180);
    String h = dyn("hello");
    check("hashCode cached equal", h.hashCode() == "hello".hashCode());
    Object o = h;
    check("String equals via Object", o.equals("hello") && !o.equals(Integer.valueOf(1)));
    CharSequence cs = h;
    check("CharSequence.length/charAt", cs.length() == 5 && cs.charAt(1) == 'e');
    checkEq("CharSequence.toString", "hello", cs.toString());
  }

  // ---- String.format --------------------------------------------------------------------------

  static void formatting() {
    checkEq("%5s|%-5s|", "   ab|ab   |", String.format("%5s|%-5s|", "ab", "ab"));
    checkEq("%05d negative", "-0042", String.format("%05d", -42));
    checkEq("%x negative", "ffffffd6", String.format("%x", -42));
    checkEq("%X", "FF", String.format("%X", 255));
    checkEq("%o negative", "37777777726", String.format("%o", -42));
    checkEq("%x long negative", "ffffffffffffffd6", String.format("%x", -42L));
    checkEq("%,d negative", "-1,234,567", String.format("%,d", -1234567));
    checkEq("%,d small", "999", String.format("%,d", 999));
    checkEq("%+.2f", "+3.14", String.format("%+.2f", 3.14159));
    checkEq("%.2f negative", "-3.14", String.format("%.2f", -3.14159));
    checkEq("%e zero", "0.000000e+00", String.format("%e", 0.0));
    checkEq("%.3e negative", "-1.235e+04", String.format("%.3e", -12345.678));
    checkEq("%.0f 2.5 HALF_UP", "3", String.format("%.0f", 2.5));
    checkEq("%.0f 3.5", "4", String.format("%.0f", 3.5));
    checkEq("%.1f 0.05", "0.1", String.format("%.1f", 0.05));
    checkEq("%.2f 1.005 (Java HALF_UP on shortest digits)", "1.01", String.format("%.2f", 1.005));
    checkEq("%.2f 0.125", "0.13", String.format("%.2f", 0.125));
    checkEq("%.3f 2.0005", "2.001", String.format("%.3f", 2.0005));
    checkEq("%f NaN", "NaN", String.format("%f", Double.NaN));
    checkEq("%.1f -Inf", "-Infinity", String.format("%.1f", Double.NEGATIVE_INFINITY));
    checkEq("%f big", "12345678901234568000000.000000", String.format("%f", 1.2345678901234567e22));
    checkEq("%.10f", "0.1000000000", String.format("%.10f", 0.1));
    checkEq("%s Integer", "7", String.format("%s", Integer.valueOf(7)));
    checkEq("%s Long", "70000000000", String.format("%s", 70000000000L));
    checkEq("%s Double", "2.5", String.format("%s", 2.5));
    checkEq("%s Float", "1.5", String.format("%s", 1.5f));
    checkEq("%s Boolean", "true", String.format("%s", true));
    checkEq("%s Character", "c", String.format("%s", 'c'));
    checkEq("%s null", "null", String.format("%s", (Object) null));
    checkEq("%S upper", "ABC", String.format("%S", "abc"));
    checkEq("%d Long", "-9223372036854775808", String.format("%d", Long.MIN_VALUE));
    checkEq("%d Short", "-5", String.format("%d", (short) -5));
    checkEq("%d Byte", "127", String.format("%d", (byte) 127));
    checkEq("%c int", "A", String.format("%c", 65));
    checkEq("%c char", "z", String.format("%c", 'z'));
    checkEq("%b null", "false", String.format("%b", (Object) null));
    checkEq("%b non-boolean", "true", String.format("%b", "x"));
    checkEq("%b Boolean false", "false", String.format("%b", Boolean.valueOf(false)));
    checkEq("%%", "100%", String.format("%d%%", 100));
    checkEq("%n", "a\nb", String.format("a%nb"));
    checkEq("%10.3f", "     3.142", String.format("%10.3f", 3.14159));
    checkEq("%-10d|", "42        |", String.format("%-10d|", 42));
    checkEq("%08.3f negative", "-003.142", String.format("%08.3f", -3.14159));
    checkEq("%,.2f", "1,234,567.89", String.format("%,.2f", 1234567.891));
    checkEq("%5c", "    x", String.format("%5c", 'x'));
    checkEq("%.3s precision", "abc", String.format("%.3s", "abcdef"));
    checkEq("%g", "12345.7", String.format("%g", 12345.678));
    checkEq("%g small", "0.000100000", String.format("%g", 0.0001));
    checkEq("%e", "1.234568e+04", String.format("%e", 12345.678));
    checkEq("%x Byte", "ff", String.format("%x", (byte) -1));
    checkEq("%x Short", "ffff", String.format("%x", (short) -1));
    checkEq("%#x", "0x2a", String.format("%#x", 42));
    checkEq("no args", "plain", String.format("plain"));
    checkEq("multiple", "1 a 2.50 true", String.format("%d %s %.2f %b", 1, "a", 2.5, true));
    checkEq("%d int MIN", "-2147483648", String.format("%d", Integer.MIN_VALUE));
    checkEq("%.1f 0.25 HALF_UP", "0.3", String.format("%.1f", 0.25));
    checkEq("%.1f 0.35 (Java HALF_UP on shortest digits)", "0.4", String.format("%.1f", 0.35));
    checkEq("%.0f 0.5", "1", String.format("%.0f", 0.5));
    checkEq("%.0f -0.5", "-1", String.format("%.0f", -0.5));
    checkEq("%.0f 1e15", "1000000000000000", String.format("%.0f", 1e15));
    checkEq("%d width larger", "  -7", String.format("%4d", -7));
    checkEq("%s width narrower", "abcdef", String.format("%3s", "abcdef"));
    boolean ife = false;
    try {
      String bad = String.format(dyn("%d"), "notanumber");
      sink += bad.length();
    } catch (java.util.IllegalFormatException e) {
      ife = true;
    } catch (IllegalArgumentException e) {
      ife = true;
    }
    check("%d with String throws", ife);
    ife = false;
    try {
      String bad = String.format(dyn("%q"), 1);
      sink += bad.length();
    } catch (java.util.IllegalFormatException e) {
      ife = true;
    } catch (IllegalArgumentException e) {
      ife = true;
    }
    check("unknown conversion throws", ife);
    ife = false;
    try {
      String bad = String.format(dyn("%.2d"), 1);
      sink += bad.length();
    } catch (java.util.IllegalFormatException e) {
      ife = true;
    } catch (IllegalArgumentException e) {
      ife = true;
    }
    check("precision on %d throws", ife);
    ife = false;
    try {
      String bad = String.format(dyn("%s %s"), "one");
      sink += bad.length();
    } catch (java.util.IllegalFormatException e) {
      ife = true;
    } catch (IllegalArgumentException e) {
      ife = true;
    }
    check("missing argument throws", ife);
    String fmt = dyn("%d");
    checkEq("extra args ignored", "1", String.format(fmt, 1, 2, 3));
  }

  // ---- StringBuilder --------------------------------------------------------------------------

  static void builders() {
    StringBuilder a = new StringBuilder();
    StringBuilder b = new StringBuilder();
    a.append("x");
    b.append("y");
    a.append("z");
    b.append(1);
    String as = a.toString();
    String bs = b.toString();
    checkEq("interleaved builder a", "xz", as);
    checkEq("interleaved builder b", "y1", bs);
    StringBuilder c = new StringBuilder("init");
    c.append(' ')
        .append(2.5)
        .append(' ')
        .append(3L)
        .append(' ')
        .append(true)
        .append(' ')
        .append(1.5f);
    checkEq("chained appends", "init 2.5 3 true 1.5", c.toString());
    StringBuilder d = new StringBuilder();
    d.append((String) null).append('|').append((Object) null);
    checkEq("append nulls", "null|null", d.toString());
    StringBuilder e = new StringBuilder("ab");
    e.append(e);
    checkEq("self append", "abab", e.toString());
    StringBuilder f = new StringBuilder();
    f.append(new StringBuilder("q"));
    f.append((CharSequence) "r");
    checkEq("append CharSequence", "qr", f.toString());
    check("length/charAt", f.length() == 2 && f.charAt(1) == 'r');
    String s1 = f.toString();
    String s2 = f.toString();
    check("toString twice equal", s1.equals(s2));
    f.append("s");
    check("earlier toString unaffected", s1.equals("qr") && f.toString().equals("qrs"));
    StringBuilder g = new StringBuilder(64);
    for (int i = 0; i < 300; i++) {
      g.append(i % 10);
    }
    check("300 appends", g.length() == 300 && g.charAt(299) == '9');
    StringBuilder h = new StringBuilder();
    check("empty toString", h.toString().equals("") && h.length() == 0);
    StringBuilder concatInLoop = new StringBuilder();
    for (int i = 0; i < 5; i++) {
      concatInLoop.append("i=" + i + ";");
    }
    checkEq("concat inside append", "i=0;i=1;i=2;i=3;i=4;", concatInLoop.toString());
    StringBuilder outer = new StringBuilder("o");
    String inner = "p" + new StringBuilder("q").append("r").toString();
    outer.append(inner);
    checkEq("nested builder in concat", "opqr", outer.toString());
    Appendable ap = new StringBuilder();
    try {
      ap.append('k').append("lm");
    } catch (java.io.IOException ex) {
      sink++;
    }
    checkEq("Appendable interface", "klm", ap.toString());
    boolean oob = false;
    try {
      sink += g.charAt(300);
    } catch (IndexOutOfBoundsException ex) {
      oob = true;
    }
    check("charAt OOB throws", oob);
  }

  // ---- wrapper parsing / printing -----------------------------------------------------------

  static void wrappers() {
    check("parseInt +5", Integer.parseInt(dyn("+5")) == 5);
    check("parseInt -0", Integer.parseInt(dyn("-0")) == 0);
    check("parseInt MIN", Integer.parseInt(dyn("-2147483648")) == Integer.MIN_VALUE);
    check("parseInt MAX", Integer.parseInt(dyn("2147483647")) == Integer.MAX_VALUE);
    check("parseInt leading zeros", Integer.parseInt(dyn("007")) == 7);
    boolean nfe = false;
    try {
      sink += Integer.parseInt(dyn("2147483648"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseInt overflow NFE", nfe);
    nfe = false;
    try {
      sink += Integer.parseInt(dyn(" 5"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseInt space NFE", nfe);
    nfe = false;
    try {
      sink += Integer.parseInt(dyn("5.0"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseInt decimal NFE", nfe);
    nfe = false;
    try {
      sink += Integer.parseInt(dyn("-"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseInt lone sign NFE", nfe);
    nfe = false;
    try {
      sink += Integer.parseInt(null);
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseInt null NFE", nfe);
    check("parseLong MIN", Long.parseLong(dyn("-9223372036854775808")) == Long.MIN_VALUE);
    check("parseLong MAX", Long.parseLong(dyn("9223372036854775807")) == Long.MAX_VALUE);
    nfe = false;
    try {
      sink += (int) Long.parseLong(dyn("9223372036854775808"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseLong overflow NFE", nfe);
    check("parseDouble 1e3", Double.parseDouble(dyn("1e3")) == 1000.0);
    check("parseDouble trims", Double.parseDouble(dyn("  1.5  ")) == 1.5);
    check("parseDouble 1.", Double.parseDouble(dyn("1.")) == 1.0);
    check("parseDouble .5", Double.parseDouble(dyn(".5")) == 0.5);
    check("parseDouble -0", 1 / Double.parseDouble(dyn("-0")) < 0);
    check("parseDouble NaN", Double.compare(Double.parseDouble(dyn("NaN")), Double.NaN) == 0);
    check("parseDouble Infinity", Double.parseDouble(dyn("Infinity")) == Double.POSITIVE_INFINITY);
    check(
        "parseDouble -Infinity", Double.parseDouble(dyn("-Infinity")) == Double.NEGATIVE_INFINITY);
    check("parseDouble 1e400", Double.parseDouble(dyn("1e400")) == Double.POSITIVE_INFINITY);
    check("parseDouble 1e-400", Double.parseDouble(dyn("1e-400")) == 0.0);
    check("parseDouble d suffix", Double.parseDouble(dyn("2.5d")) == 2.5);
    check("parseDouble f suffix", Double.parseDouble(dyn("2.5f")) == 2.5);
    check("parseDouble 0.1 exact", Double.parseDouble(dyn("0.1")) == 0.1);
    check(
        "parseDouble long mantissa",
        Double.parseDouble(dyn("3.141592653589793238462643383279")) == Math.PI);
    check("parseDouble 1e23", Double.parseDouble(dyn("1e23")) == 1e23);
    check(
        "parseDouble 8.98846567431158e307",
        Double.parseDouble(dyn("8.98846567431158e307")) == 8.98846567431158e307);
    nfe = false;
    try {
      sink += (int) Double.parseDouble(dyn("abc"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseDouble abc NFE", nfe);
    nfe = false;
    try {
      sink += (int) Double.parseDouble(dyn(""));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseDouble empty NFE", nfe);
    nfe = false;
    try {
      sink += (int) Double.parseDouble(dyn("1e"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseDouble 1e NFE", nfe);
    check("parseFloat overflow", Float.parseFloat(dyn("3.5e38")) == Float.POSITIVE_INFINITY);
    check("parseFloat 0.1", Float.parseFloat(dyn("0.1")) == 0.1f);
    check("parseFloat 16777217", Float.parseFloat(dyn("16777217")) == 16777216f);
    check(
        "parseBoolean TRUE",
        Boolean.parseBoolean(dyn("TRUE")) && Boolean.parseBoolean(dyn("true")));
    check(
        "parseBoolean yes/null", !Boolean.parseBoolean(dyn("yes")) && !Boolean.parseBoolean(null));
    nfe = false;
    try {
      sink += Byte.parseByte(dyn("128"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseByte 128 NFE", nfe);
    check("parseByte -128", Byte.parseByte(dyn("-128")) == -128);
    check(
        "parseShort",
        Short.parseShort(dyn("-32768")) == -32768 && Short.parseShort(dyn("32767")) == 32767);
    nfe = false;
    try {
      sink += Short.parseShort(dyn("32768"));
    } catch (NumberFormatException e) {
      nfe = true;
    }
    check("parseShort 32768 NFE", nfe);
    checkEq("Integer.toString MIN", "-2147483648", Integer.toString(Integer.MIN_VALUE));
    checkEq("Long.toString", "1234567890123", Long.toString(1234567890123L));
    checkEq("Double.toString 1e21", "1.0E21", Double.toString(1e21));
    checkEq("Double.toString 1e-3", "0.001", Double.toString(1e-3));
    checkEq("Double.toString 0.1+0.2", "0.30000000000000004", Double.toString(0.1 + 0.2));
    checkEq("Float.toString 3.4028235E38", "3.4028235E38", Float.toString(Float.MAX_VALUE));
    Log.i(TAG, "known divergence: Float.toString(MIN_VALUE) = " + Float.toString(Float.MIN_VALUE));
    checkEq("Float.toString 100", "100.0", Float.toString(100f));
    checkEq("Boolean.toString", "true", Boolean.toString(true));
    checkEq("Character.toString", "q", Character.toString('q'));
    checkEq("Byte.toString", "-1", Byte.toString((byte) -1));
    checkEq("Short.toString", "-300", Short.toString((short) -300));
    checkEq("Integer.valueOf(String)", "42", Integer.valueOf(dyn("42")).toString());
    checkEq("Long.valueOf(String)", "-7", Long.valueOf(dyn("-7")).toString());
    checkEq("Double.valueOf(String)", "2.5", Double.valueOf(dyn("2.5")).toString());
    checkEq("Float.valueOf(String)", "0.25", Float.valueOf(dyn("0.25")).toString());
    checkEq("Boolean.valueOf(String)", "true", Boolean.valueOf(dyn("true")).toString());
    check(
        "Integer.compare",
        Integer.compare(-1, 1) < 0
            && Integer.compare(5, 5) == 0
            && Integer.compare(Integer.MAX_VALUE, Integer.MIN_VALUE) > 0);
    check("Long.compare", Long.compare(Long.MIN_VALUE, Long.MAX_VALUE) < 0);
    check("Character.compare", Character.compare('a', 'b') < 0);
    check(
        "Boolean.compare", Boolean.compare(true, false) > 0 && Boolean.compare(false, false) == 0);
    check(
        "Double.compare NaN",
        Double.compare(Double.NaN, Double.POSITIVE_INFINITY) > 0 && Double.compare(-0.0, 0.0) < 0);
    check(
        "Integer.compareTo",
        Integer.valueOf(3).compareTo(4) < 0 && Integer.valueOf(4).compareTo(4) == 0);
    check("Long.compareTo", Long.valueOf(-1L).compareTo(1L) < 0);
    check("Double.compareTo NaN", Double.valueOf(Double.NaN).compareTo(1.0) > 0);
    check("String.compareTo via Comparable", ((Comparable<String>) "a").compareTo("b") < 0);
    Comparable<Integer> ci = 5;
    check("Integer via Comparable ref", ci.compareTo(6) < 0);
    check(
        "intValue narrowing",
        Long.valueOf(1L << 33).intValue() == 0 && Integer.valueOf(300).byteValue() == 44);
    check("shortValue", Integer.valueOf(70000).shortValue() == 4464);
    check("floatValue of long", Long.valueOf(16777217L).floatValue() == 16777216f);
    check("doubleValue of int", Integer.valueOf(7).doubleValue() == 7.0);
    check(
        "Double.intValue truncates",
        Double.valueOf(-3.9).intValue() == -3
            && Double.valueOf(1e20).longValue() == Long.MAX_VALUE);
    check("Float.intValue NaN", Float.valueOf(Float.NaN).intValue() == 0);
    check(
        "Character methods",
        Character.isDigit('7')
            && !Character.isDigit('a')
            && Character.isLetter('q')
            && !Character.isLetter('_')
            && !Character.isLetter('7'));
    check(
        "Character case",
        Character.toUpperCase('1') == '1'
            && Character.toLowerCase('Z') == 'z'
            && Character.toUpperCase('é') == 'é');
    check("Character.charValue", Character.valueOf('x').charValue() == 'x');
    check(
        "Character.equals",
        Character.valueOf('x').equals(Character.valueOf('x'))
            && !Character.valueOf('x').equals("x"));
    check(
        "Boolean.equals",
        Boolean.valueOf(true).equals(Boolean.valueOf(true))
            && !Boolean.valueOf(true).equals(Boolean.valueOf(false)));
    Object nul = null;
    check("Integer.equals(null)", !Integer.valueOf(1).equals(nul));
    check("Float.floatToIntBits NaN canonical", Float.floatToIntBits(Float.NaN) == 0x7fc00000);
    check("Float.floatToIntBits 1.0", Float.floatToIntBits(1.0f) == 0x3f800000);
    check("Float.intBitsToFloat MIN", Float.intBitsToFloat(1) == Float.MIN_VALUE);
    Number n = Integer.valueOf(9);
    check(
        "Number.intValue via interface",
        n.intValue() == 9 && n.longValue() == 9L && n.doubleValue() == 9.0);
    Number nd = Double.valueOf(2.5);
    check("Number from Double", nd.intValue() == 2 && nd.floatValue() == 2.5f);
    Number nl = Long.valueOf(-2L);
    check("Number from Long", nl.intValue() == -2 && nl.shortValue() == -2 && nl.byteValue() == -2);
  }

  // ---- java.util.Random exact stream (java.util.Random contract) --------------------------------

  // ---- oomRecovery: a map the arena cannot hold ends in a catchable OutOfMemoryError, and the app
  // keeps working once it lets go of the map. Last, so the fragmentation it leaves behind cannot
  // disturb the other sections.
  static void oomRecovery() {
    HashMap<Integer, Integer> big = new HashMap<>();
    int reached = 0;
    boolean oom = false;
    try {
      for (int i = 0; i < 4000; i++) {
        big.put(i, i);
        reached = i + 1;
      }
    } catch (OutOfMemoryError e) {
      // The heap is genuinely full here: release the map before logging allocates anything.
      big.clear();
      oom = true;
    }
    Log.i(
        TAG,
        "big map: "
            + (oom ? "OutOfMemoryError caught at " + reached : "held " + reached)
            + " entries");
    check("big map ends cleanly", oom || reached == 4000);
    big = null;
    // Recovery: ordinary work succeeds again once the map is garbage.
    HashMap<Integer, String> small = new HashMap<>();
    for (int i = 0; i < 100; i++) {
      small.put(i, "v" + i);
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 200; i++) {
      sb.append("0123456789");
    }
    ArrayList<Integer> list = new ArrayList<>();
    for (int i = 0; i < 500; i++) {
      list.add(i);
    }
    check(
        "allocation works after recovery",
        small.size() == 100
            && "v42".equals(small.get(42))
            && sb.length() == 2000
            && list.size() == 500
            && list.get(499) == 499);
  }

  static void random() {
    Random r = new Random(42);
    int a = r.nextInt();
    int b = r.nextInt();
    check("seed 42 nextInt stream", a == -1170105035 && b == 234785527);
    r = new Random(42);
    check(
        "seed 42 nextInt(100) stream",
        r.nextInt(100) == 30 && r.nextInt(100) == 63 && r.nextInt(100) == 48);
    r = new Random(42);
    check("seed 42 nextInt(16) power of two", r.nextInt(16) == 11);
    r = new Random(42);
    check("seed 42 nextLong", r.nextLong() == -5025562857975149833L);
    r = new Random(42);
    check("seed 42 nextBoolean", r.nextBoolean());
    r = new Random(42);
    check("seed 42 nextDouble", r.nextDouble() == 0.7275636800328681);
    r = new Random(42);
    check("seed 42 nextFloat", r.nextFloat() == (float) 0.7275636792182922);
    r = new Random(7);
    check(
        "seed 7 nextInt(10) stream",
        r.nextInt(10) == 6
            && r.nextInt(10) == 4
            && r.nextInt(10) == 5
            && r.nextInt(10) == 4
            && r.nextInt(10) == 0);
    r.setSeed(42);
    check("setSeed restarts stream", r.nextInt() == -1170105035);
    Random r2 = new Random(42);
    Random r3 = new Random(42);
    boolean same = true;
    for (int i = 0; i < 100; i++) {
      if (r2.nextInt(1000) != r3.nextInt(1000)) {
        same = false;
      }
    }
    check("two generators same seed agree", same);
    Random unseeded = new Random();
    boolean inRange = true;
    for (int i = 0; i < 1000; i++) {
      int v = unseeded.nextInt(7);
      if (v < 0 || v >= 7) {
        inRange = false;
      }
      double d = unseeded.nextDouble();
      if (d < 0.0 || d >= 1.0) {
        inRange = false;
      }
      float f = unseeded.nextFloat();
      if (f < 0f || f >= 1f) {
        inRange = false;
      }
    }
    check("bounded outputs in range", inRange);
    boolean iae = false;
    try {
      sink += unseeded.nextInt(0);
    } catch (IllegalArgumentException e) {
      iae = true;
    }
    check("nextInt(0) throws IAE", iae);
    iae = false;
    try {
      sink += unseeded.nextInt(-5);
    } catch (IllegalArgumentException e) {
      iae = true;
    }
    check("nextInt(-5) throws IAE", iae);
    byte[] buf = new byte[16];
    new Random(3).nextBytes(buf);
    byte[] buf2 = new byte[16];
    new Random(3).nextBytes(buf2);
    boolean eq = true;
    boolean nonzero = false;
    for (int i = 0; i < 16; i++) {
      if (buf[i] != buf2[i]) {
        eq = false;
      }
      if (buf[i] != 0) {
        nonzero = true;
      }
    }
    check("nextBytes deterministic", eq && nonzero);
    double gsum = 0;
    for (int i = 0; i < 500; i++) {
      gsum += unseeded.nextGaussian();
    }
    check("nextGaussian mean near 0", Math.abs(gsum / 500) < 0.3);
    int bucket0 = 0;
    Random rb = new Random(11);
    for (int i = 0; i < 4000; i++) {
      if (rb.nextInt(4) == 0) {
        bucket0++;
      }
    }
    check("nextInt(4) roughly uniform", bucket0 > 800 && bucket0 < 1200);
  }
}
