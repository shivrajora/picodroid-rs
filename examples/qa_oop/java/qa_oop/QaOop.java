// SPDX-License-Identifier: GPL-3.0-only
package qa_oop;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Objects;
import picodroid.app.Application;
import picodroid.util.Log;

/**
 * QA 2026-09-13: object model semantics — construction order, field shadowing, static hiding,
 * virtual dispatch from constructors, inner/anonymous/local classes, lambdas and method references,
 * enums with bodies, static initialisation order, generics bridges, varargs, and the
 * equals/hashCode/toString/compareTo upcalls the collections rely on.
 */
public class QaOop extends Application {
  private static final String TAG = "QaOop";

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
    Log.i(TAG, "=== QaOop start ===");
    section("construction", () -> construction());
    section("shadowing", () -> shadowing());
    section("dispatch", () -> dispatch());
    section("innerClasses", () -> innerClasses());
    section("lambdas", () -> lambdas());
    section("enums", () -> enums());
    section("staticInit", () -> staticInit());
    section("generics", () -> generics());
    section("varargs", () -> varargs());
    section("objectContracts", () -> objectContracts());
    section("interfaces", () -> interfaces());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  // ---- construction order ---------------------------------------------------------------------

  static StringBuilder trace = new StringBuilder();

  static int mark(String s, int v) {
    trace.append(s);
    return v;
  }

  static class Parent {
    int pf = mark("pf", 1);

    Parent() {
      trace.append("P(");
      hook();
      trace.append(")");
    }

    void hook() {
      trace.append("ph");
    }
  }

  static class Child extends Parent {
    int cf = mark("cf", 5);
    String name = "child";

    Child() {
      super();
      trace.append("C");
    }

    Child(int x) {
      this();
      trace.append("C" + x);
    }

    @Override
    void hook() {
      // called from Parent's constructor: cf and name are still defaults here
      trace.append("ch" + cf + (name == null ? "null" : name));
    }
  }

  static void construction() {
    trace = new StringBuilder();
    Child c = new Child(3);
    String t = trace.toString();
    check("construction order pf,P(,ch0null,),cf,C,C3", t.equals("pfP(ch0null)cfCC3"));
    check("fields set after super()", c.cf == 5 && c.name.equals("child") && c.pf == 1);
    trace = new StringBuilder();
    Parent p = new Parent();
    check("parent alone", trace.toString().equals("pfP(ph)") && p.pf == 1);
  }

  // ---- field shadowing and static hiding ------------------------------------------------------

  static class A {
    int x = 1;
    static int s = 10;

    static String who() {
      return "A";
    }

    int getX() {
      return x;
    }

    String me() {
      return "A" + x;
    }
  }

  static class B extends A {
    int x = 2;
    static int s = 20;

    static String who() {
      return "B";
    }

    @Override
    int getX() {
      return x;
    }

    int superX() {
      return super.x;
    }

    @Override
    String me() {
      return "B" + x + super.me();
    }
  }

  static class C extends B {
    @Override
    String me() {
      return "C" + super.me();
    }
  }

  static void shadowing() {
    B b = new B();
    A ab = b;
    check("B.x shadows A.x", b.x == 2 && ab.x == 1);
    check("super.x reads A.x", b.superX() == 1);
    check("virtual getX reads B.x", ab.getX() == 2);
    check("cast to A reads A field", ((A) b).x == 1);
    check("static hiding by static type", A.who().equals("A") && B.who().equals("B"));
    check("static fields distinct", A.s == 10 && B.s == 20);
    A.s = 11;
    check("static write via subclass name", A.s == 11 && B.s == 20);
    check("three-level super chain", new C().me().equals("CB2A1"));
    b.x = 7;
    ab.x = 8;
    check("writes go to the right slot", b.x == 7 && ((A) b).x == 8 && b.superX() == 8);
  }

  // ---- dispatch -------------------------------------------------------------------------------

  abstract static class Shape {
    abstract double area();

    String describe() {
      return name() + ":" + (int) area();
    }

    String name() {
      return "shape";
    }
  }

  static class Sq extends Shape {
    final double s;

    Sq(double s) {
      this.s = s;
    }

    @Override
    double area() {
      return s * s;
    }

    @Override
    String name() {
      return "sq";
    }
  }

  static class Circ extends Shape {
    final double r;

    Circ(double r) {
      this.r = r;
    }

    @Override
    double area() {
      return Math.PI * r * r;
    }
  }

  interface Greeter {
    String greet();

    default String twice() {
      return greet() + greet();
    }

    static Greeter of(String s) {
      return () -> s;
    }
  }

  static class Loud implements Greeter {
    @Override
    public String greet() {
      return "HI";
    }

    @Override
    public String twice() {
      return "LOUD";
    }
  }

  static class Base2 {
    public String twice() {
      return "base";
    }
  }

  static class Mixed extends Base2 implements Greeter {
    @Override
    public String greet() {
      return "m";
    }
  }

  static void dispatch() {
    Shape[] shapes = {new Sq(3), new Circ(1)};
    check("template method through abstract", shapes[0].describe().equals("sq:9"));
    check("inherited name()", shapes[1].describe().equals("shape:3"));
    Greeter g = Greeter.of("x");
    check("interface static + lambda", g.greet().equals("x"));
    check("default method", g.twice().equals("xx"));
    check("default overridden", new Loud().twice().equals("LOUD"));
    check("class method wins over default", new Mixed().twice().equals("base"));
    Object o = new Loud();
    check("instanceof interface", o instanceof Greeter);
    Greeter cast = (Greeter) o;
    check("invokeinterface after checkcast", cast.greet().equals("HI"));
    Greeter anon =
        new Greeter() {
          @Override
          public String greet() {
            return "anon";
          }
        };
    check("anonymous default", anon.twice().equals("anonanon"));
  }

  // ---- inner, local, anonymous classes --------------------------------------------------------

  private int outerField = 40;

  class Inner {
    int v = 2;

    int sum() {
      return outerField + v;
    }

    int viaOuterThis() {
      return QaOop.this.outerField;
    }

    void bump() {
      outerField++;
    }
  }

  static class Nested {
    static int count = 0;
    final int id;

    Nested() {
      id = ++count;
    }
  }

  void innerClasses() {
    Inner in = new Inner();
    check("inner reads outer field", in.sum() == 42);
    check("Outer.this", in.viaOuterThis() == 40);
    in.bump();
    check("inner writes outer field", outerField == 41);
    Inner in2 = this.new Inner();
    check("explicit outer.new", in2.sum() == 43);
    Nested n1 = new Nested();
    Nested n2 = new Nested();
    check("static nested counter", n1.id + 1 == n2.id);
    final int[] box = {0};
    int local = 5;
    class Local {
      int get() {
        return local + box[0];
      }
    }
    Local lc = new Local();
    box[0] = 10;
    check("local class captures", lc.get() == 15);
    Runnable r =
        new Runnable() {
          int calls = 0;

          @Override
          public void run() {
            calls++;
            box[0] += local + calls;
            outerField += 100;
          }
        };
    r.run();
    r.run();
    check("anonymous with state captures array + outer", box[0] == 23 && outerField == 241);
    outerField = 40;
  }

  // ---- lambdas and method references ----------------------------------------------------------

  interface Fn<A, B> {
    B apply(A a);
  }

  interface Factory<T> {
    T make();
  }

  interface IntOp {
    int op(int a, int b);
  }

  interface Supplier0 {
    Object get();
  }

  static int twice(int x) {
    return x * 2;
  }

  int instanceAdd(int x) {
    return x + outerField;
  }

  static class Counter {
    int n;

    int next() {
      return ++n;
    }

    static Counter create() {
      return new Counter();
    }
  }

  static Fn<Integer, Fn<Integer, Integer>> adder() {
    return a -> b -> a + b;
  }

  void lambdas() {
    Fn<Integer, Integer> dbl = x -> x * 2;
    check("lambda boxing", dbl.apply(21) == 42);
    Fn<Integer, Integer> sref = QaOop::twice;
    check("static method ref", sref.apply(5) == 10);
    Fn<Integer, Integer> iref = this::instanceAdd;
    check("bound instance method ref", iref.apply(2) == 42);
    Fn<String, Integer> unbound = String::length;
    check("unbound instance method ref", unbound.apply("abcd") == 4);
    Factory<Counter> ctor = Counter::new;
    Counter c1 = ctor.make();
    Counter c2 = ctor.make();
    check("constructor ref makes fresh objects", c1 != c2 && c1.next() == 1 && c2.next() == 1);
    Factory<Counter> sf = Counter::create;
    check("static factory ref", sf.make().next() == 1);
    Fn<Integer, Integer> curried = adder().apply(10);
    check("curried lambda", curried.apply(5) == 15);
    IntOp op = (a, b) -> a - b;
    check("two-arg lambda", op.op(10, 3) == 7);
    int[] counter = {0};
    Runnable inc = () -> counter[0]++;
    for (int i = 0; i < 5; i++) {
      inc.run();
    }
    check("lambda mutates captured array", counter[0] == 5);
    ArrayList<Runnable> rs = new ArrayList<>();
    for (int i = 0; i < 3; i++) {
      final int k = i;
      rs.add(() -> counter[0] += k * 10);
    }
    for (Runnable r : rs) {
      r.run();
    }
    check("loop-captured copies", counter[0] == 35);
    Supplier0 s = () -> this;
    check("lambda captures this", s.get() == this);
    Fn<Object, String> str = Object::toString;
    check("Object::toString ref on String", str.apply("q").equals("q"));
    Fn<Integer, String> vo = String::valueOf;
    check("String::valueOf(int) ref", vo.apply(7).equals("7"));
    Comparator<String> byLen = (a, b) -> a.length() - b.length();
    String[] words = {"ccc", "a", "bb"};
    Arrays.sort(words, byLen);
    check("lambda comparator", words[0].equals("a") && words[2].equals("ccc"));
    Comparator<String> rev = (a, b) -> b.compareTo(a);
    Arrays.sort(words, rev);
    check("reverse comparator", words[0].equals("ccc") && words[2].equals("a"));
    Runnable nested =
        () -> {
          Runnable inner = () -> counter[0] = 100;
          inner.run();
        };
    nested.run();
    check("nested lambdas", counter[0] == 100);
    Fn<Integer, Integer>[] table = newFnArray(3);
    for (int i = 0; i < 3; i++) {
      final int m = i + 1;
      table[i] = x -> x * m;
    }
    check("lambda array", table[0].apply(7) == 7 && table[2].apply(7) == 21);
    Object asObj = dbl;
    check("lambda instanceof its interface", asObj instanceof Fn);
    check("lambda not instanceof Runnable", !(asObj instanceof Runnable));
    Fn<Integer, Integer> same = dbl;
    check("lambda identity kept", same == dbl);
  }

  @SuppressWarnings("unchecked")
  static Fn<Integer, Integer>[] newFnArray(int n) {
    return (Fn<Integer, Integer>[]) new Fn[n];
  }

  // ---- enums ----------------------------------------------------------------------------------

  enum Op {
    ADD("+") {
      @Override
      int apply(int a, int b) {
        return a + b;
      }
    },
    MUL("*") {
      @Override
      int apply(int a, int b) {
        return a * b;
      }
    },
    NEG("-") {
      @Override
      int apply(int a, int b) {
        return -a;
      }

      @Override
      public String toString() {
        return "negate";
      }
    };

    final String sym;

    Op(String sym) {
      this.sym = sym;
    }

    abstract int apply(int a, int b);
  }

  enum Plain {
    ONE,
    TWO,
    THREE;

    static int total = 0;

    static {
      for (Plain p : values()) {
        total += p.ordinal() + 1;
      }
    }
  }

  interface Named {
    String label();
  }

  enum Tagged implements Named {
    X,
    Y;

    @Override
    public String label() {
      return "tag-" + name();
    }
  }

  static Op addAgain() {
    sink++;
    return Op.ADD;
  }

  static void enums() {
    check("constant-specific body ADD", Op.ADD.apply(2, 3) == 5);
    check("constant-specific body MUL", Op.MUL.apply(2, 3) == 6);
    check("constant-specific body NEG", Op.NEG.apply(2, 3) == -2);
    check("enum field", Op.MUL.sym.equals("*"));
    check(
        "enum toString override",
        Op.NEG.toString().equals("negate") && Op.NEG.name().equals("NEG"));
    check("values() length", Op.values().length == 3);
    Op[] va = Op.values();
    Op[] vb = Op.values();
    check("values() returns a fresh array", va != vb);
    Op[] vs = Op.values();
    vs[0] = null;
    check("values() copy is independent", Op.values()[0] == Op.ADD);
    check("valueOf", Op.valueOf("MUL") == Op.MUL);
    boolean iae = false;
    try {
      Op bad = Op.valueOf("NOPE");
      sink += bad.ordinal();
    } catch (IllegalArgumentException e) {
      iae = true;
    }
    check("valueOf bad name throws IAE", iae);
    check("compareTo by ordinal", Op.ADD.compareTo(Op.NEG) < 0 && Op.NEG.compareTo(Op.ADD) > 0);
    check(
        "enum equals/hashCode stable",
        addAgain().equals(Op.ADD) && addAgain().hashCode() == Op.ADD.hashCode());
    check("enum static init sees values", Plain.total == 6);
    check("enum implements interface", Tagged.Y.label().equals("tag-Y"));
    Named n = Tagged.X;
    check("enum through interface ref", n.label().equals("tag-X"));
    HashMap<Op, String> byOp = new HashMap<>();
    byOp.put(Op.ADD, "a");
    byOp.put(Op.NEG, "n");
    check("enum as map key", "n".equals(byOp.get(Op.NEG)) && byOp.get(Op.MUL) == null);
    HashSet<Plain> set = new HashSet<>();
    set.add(Plain.ONE);
    set.add(Plain.ONE);
    set.add(Plain.TWO);
    check(
        "enum set dedups",
        set.size() == 2 && set.contains(Plain.TWO) && !set.contains(Plain.THREE));
    String sw = "";
    for (Op o : Op.values()) {
      switch (o) {
        case ADD:
          sw += "a";
          break;
        case MUL:
          sw += "m";
          break;
        case NEG:
          sw += "n";
          break;
      }
    }
    check("switch over enum with bodies", sw.equals("amn"));
    Object o = Op.ADD;
    check("enum instanceof Enum", o instanceof Enum && o instanceof Comparable);
    Enum<?> e = Op.MUL;
    check("Enum-typed name()", e.name().equals("MUL") && e.ordinal() == 1);
  }

  // ---- static initialisation order -----------------------------------------------------------

  static class Fwd {
    static int a = f();
    static int b = 2;

    static int f() {
      return b;
    }
  }

  static class Lazy1 {
    static {
      trace.append("L1");
    }

    static int v = 1;
  }

  static class Lazy2 {
    static {
      trace.append("L2");
    }

    static int w = Lazy1.v + 1;
  }

  interface Consts {
    int K = 7;
    Object O = mark("IO", 1) == 1 ? new Object() : null;
  }

  static class Holder {
    static final Holder INSTANCE = new Holder();
    static int seq = 5;
    final int seen;

    Holder() {
      seen = seq;
    }
  }

  static void staticInit() {
    check("forward static reference reads default", Fwd.a == 0 && Fwd.b == 2);
    trace = new StringBuilder();
    int w = Lazy2.w;
    check("clinit chain order", trace.toString().equals("L2L1") && w == 2);
    trace = new StringBuilder();
    int k = Consts.K;
    check("interface constant inlined, no clinit", k == 7 && trace.length() == 0);
    Object o = Consts.O;
    check(
        "interface non-constant field triggers clinit", o != null && trace.toString().equals("IO"));
    check("singleton created before later statics", Holder.INSTANCE.seen == 0 && Holder.seq == 5);
    check("static nested counter persists", Nested.count >= 2);
  }

  // ---- generics -------------------------------------------------------------------------------

  static class Box<T extends Comparable<T>> implements Comparable<Box<T>> {
    final T v;

    Box(T v) {
      this.v = v;
    }

    @Override
    public int compareTo(Box<T> o) {
      return v.compareTo(o.v);
    }

    T get() {
      return v;
    }
  }

  static class IntBox extends Box<Integer> {
    IntBox(int v) {
      super(v);
    }

    @Override
    Integer get() {
      return v + 1;
    }
  }

  static <T extends Comparable<T>> T maxOf(T a, T b) {
    return a.compareTo(b) >= 0 ? a : b;
  }

  static <T> T first(ArrayList<T> list) {
    return list.get(0);
  }

  static class Pair<K, V> {
    final K k;
    final V v;

    Pair(K k, V v) {
      this.k = k;
      this.v = v;
    }
  }

  static void generics() {
    check("bounded generic method", maxOf("apple", "banana").equals("banana"));
    check("bounded generic method ints", maxOf(3, 9) == 9);
    Box<String> bs = new Box<>("z");
    Box<String> bt = new Box<>("a");
    check("Comparable bridge on generic class", bs.compareTo(bt) > 0);
    ArrayList<Box<String>> boxes = new ArrayList<>();
    boxes.add(bs);
    boxes.add(bt);
    Collections.sort(boxes);
    check("Collections.sort via bridge method", boxes.get(0) == bt);
    Box<Integer> ib = new IntBox(4);
    check("covariant override through bridge", ib.get() == 5);
    Comparable<Box<Integer>> cmp = ib;
    check("bridge compareTo on subclass", cmp.compareTo(new IntBox(4)) == 0);
    ArrayList<String> strs = new ArrayList<>();
    strs.add("f");
    check("generic static method", first(strs).equals("f"));
    Pair<String, ArrayList<Integer>> p = new Pair<>("k", new ArrayList<Integer>());
    p.v.add(1);
    check("nested generic field", p.k.equals("k") && p.v.get(0) == 1);
    HashMap<String, ArrayList<Integer>> multi = new HashMap<>();
    for (int i = 0; i < 6; i++) {
      String key = i % 2 == 0 ? "even" : "odd";
      ArrayList<Integer> l = multi.get(key);
      if (l == null) {
        l = new ArrayList<>();
        multi.put(key, l);
      }
      l.add(i);
    }
    check("map of lists", multi.get("even").size() == 3 && multi.get("odd").get(2) == 5);
    Object raw = strs;
    @SuppressWarnings("unchecked")
    ArrayList<Integer> wrong = (ArrayList<Integer>) raw;
    boolean cce = false;
    try {
      int v = wrong.get(0);
      sink += v;
    } catch (ClassCastException e) {
      cce = true;
    }
    check("heap pollution surfaces as CCE at use", cce);
  }

  // ---- varargs --------------------------------------------------------------------------------

  static int sum(int... xs) {
    int s = 0;
    for (int x : xs) {
      s += x;
    }
    return s;
  }

  static int count(Object... os) {
    return os == null ? -1 : os.length;
  }

  static String joinAll(String sep, String... parts) {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < parts.length; i++) {
      if (i > 0) {
        sb.append(sep);
      }
      sb.append(parts[i]);
    }
    return sb.toString();
  }

  static void varargs() {
    check("varargs none", sum() == 0);
    check("varargs one", sum(4) == 4);
    check("varargs many", sum(1, 2, 3, 4) == 10);
    check("varargs array", sum(new int[] {5, 6}) == 11);
    check("Object... with mixed", count(1, "a", null) == 3);
    check("Object... with one null element (cast)", count((Object) null) == 1);
    check("Object... with null array", count((Object[]) null) == -1);
    check("varargs with leading param", joinAll("-", "a", "b", "c").equals("a-b-c"));
    check("varargs with leading param only", joinAll("-").equals(""));
    check("String.format is varargs", String.format("%d-%s", 1, "x").equals("1-x"));
    check("String.join varargs", String.join(",", "a", "b").equals("a,b"));
    check("String.join none", String.join(",").equals(""));
  }

  // ---- equals / hashCode / toString / compareTo upcalls ----------------------------------------

  static class Key {
    final String a;
    final int b;

    Key(String a, int b) {
      this.a = a;
      this.b = b;
    }

    @Override
    public boolean equals(Object o) {
      if (!(o instanceof Key)) {
        return false;
      }
      Key k = (Key) o;
      return k.b == b && k.a.equals(a);
    }

    @Override
    public int hashCode() {
      return 1; // pathological: every key collides
    }

    @Override
    public String toString() {
      return "Key(" + a + "," + b + ")";
    }
  }

  static class Ver implements Comparable<Ver> {
    final int major;
    final int minor;
    final int seq;

    Ver(int major, int minor, int seq) {
      this.major = major;
      this.minor = minor;
      this.seq = seq;
    }

    @Override
    public int compareTo(Ver o) {
      if (major != o.major) {
        return major - o.major;
      }
      return minor - o.minor;
    }
  }

  static class NoOverrides {}

  static void objectContracts() {
    HashMap<Key, Integer> m = new HashMap<>();
    for (int i = 0; i < 50; i++) {
      m.put(new Key("k" + (i % 10), i), i);
    }
    check("colliding keys all stored", m.size() == 50);
    check(
        "colliding key lookup", m.get(new Key("k3", 13)) == 13 && m.get(new Key("k3", 14)) == null);
    check(
        "colliding key remove",
        m.remove(new Key("k0", 0)) == 0 && m.size() == 49 && !m.containsKey(new Key("k0", 0)));
    HashSet<Key> set = new HashSet<>();
    set.add(new Key("a", 1));
    set.add(new Key("a", 1));
    set.add(new Key("a", 2));
    check(
        "set uses equals",
        set.size() == 2 && set.contains(new Key("a", 2)) && !set.contains(new Key("b", 1)));
    ArrayList<Key> list = new ArrayList<>();
    list.add(new Key("x", 9));
    check(
        "ArrayList.contains uses equals",
        list.contains(new Key("x", 9)) && !list.contains(new Key("x", 8)));
    check("ArrayList.remove(Object) uses equals", list.remove(new Key("x", 9)) && list.isEmpty());
    Key k = new Key("t", 5);
    check("toString via concat", ("" + k).equals("Key(t,5)"));
    check("toString via valueOf", String.valueOf(k).equals("Key(t,5)"));
    check("toString via format", String.format("[%s]", k).equals("[Key(t,5)]"));
    check("toString via Objects.toString", Objects.toString(k).equals("Key(t,5)"));
    StringBuilder sb = new StringBuilder();
    sb.append(k);
    check("toString via StringBuilder.append(Object)", sb.toString().equals("Key(t,5)"));
    check(
        "toString via Arrays.toString",
        Arrays.toString(new Object[] {k, null}).equals("[Key(t,5), null]"));
    Object nul = null;
    Object nul2 = null;
    check(
        "Objects.equals nulls",
        Objects.equals(nul, nul2) && !Objects.equals(k, nul) && Objects.equals(k, new Key("t", 5)));
    check("Objects.hashCode(null)", Objects.hashCode(nul) == 0 && Objects.hashCode(k) == 1);
    check(
        "Objects.hash",
        Objects.hash(1, "a") == 1089 && Objects.hash() == 1 && Objects.hash(nul) == 31);
    boolean npe = false;
    try {
      Objects.requireNonNull(nul, "needed");
    } catch (NullPointerException e) {
      npe = "needed".equals(e.getMessage());
    }
    check("requireNonNull message", npe);
    NoOverrides n1 = new NoOverrides();
    NoOverrides n2 = new NoOverrides();
    NoOverrides n1Again = n1;
    check("default equals is identity", n1.equals(n1Again) && !n1.equals(n2));
    check("default hashCode stable", n1.hashCode() == n1Again.hashCode());
    String ts = n1.toString();
    check(
        "default toString has class name",
        ts.startsWith("qa_oop.QaOop$NoOverrides@") || ts.startsWith("qa_oop.QaOop.NoOverrides@"));
    // Comparable with stable sort over a larger input
    ArrayList<Ver> vers = new ArrayList<>();
    for (int i = 0; i < 60; i++) {
      vers.add(new Ver(i % 3, (i * 7) % 5, i));
    }
    Collections.sort(vers);
    boolean ordered = true;
    boolean stable = true;
    for (int i = 1; i < vers.size(); i++) {
      Ver p = vers.get(i - 1);
      Ver q = vers.get(i);
      if (p.compareTo(q) > 0) {
        ordered = false;
      }
      if (p.compareTo(q) == 0 && p.seq > q.seq) {
        stable = false;
      }
    }
    check("Collections.sort ordered", ordered);
    check("Collections.sort stable", stable);
    Ver[] arr = new Ver[60];
    for (int i = 0; i < 60; i++) {
      arr[i] = new Ver((59 - i) % 4, i % 2, i);
    }
    Arrays.sort(arr);
    ordered = true;
    stable = true;
    for (int i = 1; i < arr.length; i++) {
      if (arr[i - 1].compareTo(arr[i]) > 0) {
        ordered = false;
      }
      if (arr[i - 1].compareTo(arr[i]) == 0 && arr[i - 1].seq > arr[i].seq) {
        stable = false;
      }
    }
    check("Arrays.sort ordered", ordered);
    check("Arrays.sort stable", stable);
    Arrays.sort(arr, (x, y) -> y.seq - x.seq);
    check("Arrays.sort comparator descending", arr[0].seq == 59 && arr[59].seq == 0);
    vers.sort((x, y) -> x.seq - y.seq);
    check("ArrayList.sort(Comparator)", vers.get(0).seq == 0 && vers.get(59).seq == 59);
    vers.sort(null);
    check("ArrayList.sort(null) natural order", vers.get(0).major == 0 && vers.get(59).major == 2);
  }

  // ---- interfaces: diamond defaults, super.default, generic interface -------------------------

  interface Left {
    default String pick() {
      return "L";
    }
  }

  interface Right {
    default String pick() {
      return "R";
    }
  }

  static class Both implements Left, Right {
    @Override
    public String pick() {
      return Left.super.pick() + Right.super.pick();
    }
  }

  interface Visitor<R> {
    R visitNum(int n);

    R visitAdd(Node a, Node b);
  }

  interface Node {
    <R> R accept(Visitor<R> v);
  }

  static class Num implements Node {
    final int n;

    Num(int n) {
      this.n = n;
    }

    @Override
    public <R> R accept(Visitor<R> v) {
      return v.visitNum(n);
    }
  }

  static class Add implements Node {
    final Node a;
    final Node b;

    Add(Node a, Node b) {
      this.a = a;
      this.b = b;
    }

    @Override
    public <R> R accept(Visitor<R> v) {
      return v.visitAdd(a, b);
    }
  }

  static void interfaces() {
    check("diamond resolved with X.super", new Both().pick().equals("LR"));
    Node expr = new Add(new Num(2), new Add(new Num(3), new Num(4)));
    Visitor<Integer> eval =
        new Visitor<Integer>() {
          @Override
          public Integer visitNum(int n) {
            return n;
          }

          @Override
          public Integer visitAdd(Node a, Node b) {
            return a.accept(this) + b.accept(this);
          }
        };
    check("generic visitor eval", expr.accept(eval) == 9);
    Visitor<String> show =
        new Visitor<String>() {
          @Override
          public String visitNum(int n) {
            return String.valueOf(n);
          }

          @Override
          public String visitAdd(Node a, Node b) {
            return "(" + a.accept(this) + "+" + b.accept(this) + ")";
          }
        };
    check("generic visitor show", expr.accept(show).equals("(2+(3+4))"));
    Left l = new Both();
    check("default via interface-typed ref", l.pick().equals("LR"));
    Left plain = new Left() {};
    check("empty anonymous uses default", plain.pick().equals("L"));
  }
}
