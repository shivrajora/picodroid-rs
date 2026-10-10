---
title: "Core Java Language Surface"
description: "java.lang, java.util, java.util.zip and java.time classes available in Picodroid apps."
---

`java.lang.*`, `java.util.*` and `java.time.*` types implemented by the Picodroid JVM. See [Java API overview](/api/) for the full API index.

The build checks an app against this surface: `verifyApiContract` rejects a `java.*` class or member the runtime does not serve, naming the call site. The [compatibility matrix](/reference/compatibility-matrix/#java-standard-library) lists the divergences row by row.

## `java.lang.String`

The JVM provides built-in support for `java.lang.String`. All methods work on ASCII strings; multi-byte UTF-8 sequences are passed through unchanged but byte-indexed (not character-indexed).

```java
String s = "Hello, Pico!";

// Length and access
int len   = s.length();          // 12
char ch   = s.charAt(7);         // 'P'
boolean e = s.isEmpty();         // false

// Comparison
boolean eq  = s.equals("Hello, Pico!");          // true
boolean eqi = s.equalsIgnoreCase("hello, pico!"); // true
int     cmp = s.compareTo("Hello, Pico!");        // 0

// Predicates
boolean sw = s.startsWith("Hello");  // true
boolean ew = s.endsWith("Pico!");    // true
boolean co = s.contains("Pico");     // true

// Search
int idx  = s.indexOf(',');         // 6
int idx2 = s.indexOf("Pico");      // 7
int li   = s.lastIndexOf('!');     // 11

// Transforms — return new String values
String sub   = s.substring(7, 11);  // "Pico"
String tail  = s.substring(7);      // "Pico!"
String tr    = "  hi  ".trim();     // "hi"
String upper = "pico".toUpperCase(); // "PICO"
String lower = "PICO".toLowerCase(); // "pico"

// Static factory — valueOf covers int, long, boolean, char, float, double
String vi = String.valueOf(42);       // "42"
String vl = String.valueOf(9000L);    // "9000"
String vb = String.valueOf(true);     // "true"
String vc = String.valueOf('X');      // "X"
String vf = String.valueOf(3.14f);    // "3.14"
String vd = String.valueOf(2.71828);  // "2.71828"
String vo = String.valueOf(obj);      // obj.toString(), or "null"

// Extended methods
String[] parts = "a,b,c".split(",");           // ["a", "b", "c"]
String r       = "foo bar".replace(' ', '_'); // "foo_bar"
String c       = "Hello, ".concat("World");    // "Hello, World"
String j       = String.join(", ", "a", "b", "c");           // "a, b, c"
String j2      = String.join("/", new String[] {"x", "y"});  // "x/y"
char[] chs     = "abc".toCharArray();          // {'a', 'b', 'c'}
int    h       = "abc".hashCode();             // standard Java String hash
String r2      = "a-b".replace("-", "+");      // "a+b" — the CharSequence form
byte[] raw     = "abc".getBytes();             // the string's bytes (UTF-8)
String back    = new String(raw);              // also new String(raw, off, len)

// Formatted strings — String.format(String, Object...)
String msg = String.format("Score: %d (%.1f%%)", 42, 87.5);  // "Score: 42 (87.5%)"
```

`String.format` supports the conversions `%s %d %x %X %o %c %b %f %e %g %n %%` with the flags
`-` `0` `+` `(space)` `,` `#`, plus width and precision (e.g. `%-8s`, `%08.2f`). Floating conversions round
HALF_UP, as Java's `Formatter` does (`%.2f` of `1.005` is `"1.01"`). A bad format throws the
`IllegalFormatException` subclass Java names: `MissingFormatArgumentException`,
`IllegalFormatPrecisionException` (a precision on `%d` `%x` `%o` `%c`),
`IllegalFormatConversionException` or `UnknownFormatConversionException`.

`String.join(delimiter, …)` takes varargs, a `String[]`, or an `ArrayList<String>`
(`Iterable`); the elements must be `String` or `null`.

`split` takes a literal delimiter, not a regular expression, and drops trailing empty strings as
Java does (`"a,b,,".split(",")` is `["a", "b"]`). Strings are built from `byte[]` only: there is
no `String(char[])` constructor, so build one from characters with `StringBuilder.append(char)`.

## `java.lang.StringBuilder`

```java
StringBuilder sb = new StringBuilder();         // empty
StringBuilder sb = new StringBuilder("prefix="); // with initial content

sb.append("text");    // append String
sb.append(42);        // append int
sb.append(3.14f);     // append float  (formats as "3.14")
sb.append(2.71828);   // append double
sb.append(100L);      // append long
sb.append(true);      // append "true" or "false"
sb.append('x');       // append char
sb.append(obj);       // append Object — its toString(), or "null"

int  len = sb.length();    // current content length
char ch  = (char) sb.charAt(2);  // byte at position 2

String s = sb.toString();  // intern result as a String
```

> Every `StringBuilder` owns its buffer, so builders interleave freely and `sb.append(sb)` appends a copy of the current content; a buffer the heap cannot grow throws `OutOfMemoryError` from `append`.

## `java.lang.Math`

Standard math functions. All methods are static. `Math.PI` and `Math.E` are compile-time constants inlined by `javac`.

```java
// Constants (inlined by the compiler — no runtime cost)
double pi = Math.PI;   // 3.141592653589793
double e  = Math.E;    // 2.718281828459045

// abs — int, long, float, double
int    ai = Math.abs(-7);      // 7
long   al = Math.abs(-9000L);  // 9000
float  af = Math.abs(-3.14f);  // 3.14
double ad = Math.abs(-1.0);    // 1.0

// min / max — int, long, float, double
int    lo = Math.min(4, 9);    // 4
double hi = Math.max(1.5, 2.5); // 2.5

// Rounding
double fl = Math.floor(2.9);    // 2.0
double ce = Math.ceil(2.1);     // 3.0
int    ri = Math.round(2.6f);   // 3   (float → int)
long   rl = Math.round(2.5);    // 3   (double → long)

// Powers / roots
double sq = Math.sqrt(2.0);          // ≈ 1.4142135
double pw = Math.pow(2.0, 10.0);     // 1024.0

// Floor division and the exact-arithmetic helpers (int and long forms)
int  fd = Math.floorDiv(-7, 2);      // -4  (rounds toward negative infinity; -7 / 2 is -3)
int  fm = Math.floorMod(-7, 2);      // 1   (takes the divisor's sign; -7 % 2 is -1)
long ex = Math.multiplyExact(1L << 20, 1L << 20);  // ArithmeticException on overflow
int  ie = Math.toIntExact(42L);      // ArithmeticException when the long does not fit

// Trigonometry (arguments in radians)
double s  = Math.sin(Math.PI / 2.0); // ≈ 1.0
double c  = Math.cos(0.0);           // 1.0
double t  = Math.tan(0.0);           // 0.0
double a2 = Math.atan2(1.0, 1.0);   // ≈ PI/4

// Angle conversion
double rad = Math.toRadians(90.0);   // ≈ PI/2
double deg = Math.toDegrees(Math.PI); // 180.0

// Logarithms / exponential
double ln  = Math.log(Math.E);       // ≈ 1.0
double lg  = Math.log10(100.0);      // ≈ 2.0
double ex  = Math.exp(1.0);          // ≈ 2.71828
```

## `java.util.ArrayList`

Dynamic list backed by a per-instance heap buffer.

```java
import java.util.ArrayList;

// Raw type (stores any Object — String, custom objects, null)
ArrayList list = new ArrayList();
list.add("alpha");
list.add("beta");
list.add("gamma");

int sz     = list.size();           // 3
boolean mt = list.isEmpty();        // false

String item    = (String) list.get(1);    // "beta"
String old     = (String) list.set(0, "ALPHA");  // returns "alpha"
String removed = (String) list.remove(2);        // returns "gamma"

boolean found = list.contains("ALPHA");   // true
boolean gone  = list.remove("beta");      // remove(Object): drops the first equal element
Object[] copy = list.toArray();           // always a fresh Object[] of the list's length
list.clear();

// Indexed insert
list.add(0, "first");   // insert at position 0

// Generic type with autoboxing (Integer, Boolean, Long, Float, Double)
ArrayList<Integer> nums = new ArrayList<Integer>();   // or new ArrayList<Integer>(16)
nums.add(10);    // autoboxes int → Integer
nums.add(20);
int n = nums.get(0);          // auto-unboxes Integer → int  (10)
boolean has = nums.contains(20);  // true — value equality for wrappers
```

`list.sort(comparator)` sorts in place; `list.sort(null)` uses the natural ordering. `toArray(T[])` ignores its argument: it neither fills nor returns the array passed in, and the result is an `Object[]`.

> **Autoboxing:** `ArrayList<Integer>` works as expected — `add(42)` and `contains(42)` both box via `Integer.valueOf`. For raw `ArrayList`, store and retrieve Object references (String, custom class instances); do not store bare primitives without explicit boxing (`Integer.valueOf(42)`, etc.).

## `java.util.HashMap` and `java.util.HashSet`

Hash-table-backed associative containers. Keys are compared by `equals()` / `hashCode()`; autoboxed primitives (`Integer`, `Long`, `Boolean`, `String`) all work as keys.

```java
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;

HashMap map = new HashMap();
map.put("one", Integer.valueOf(1));
map.put("two", Integer.valueOf(2));

Integer v   = (Integer) map.get("one");      // 1
boolean has = map.containsKey("two");        // true
int     n   = map.size();                    // 2
Integer d   = (Integer) map.getOrDefault("nope", Integer.valueOf(0));  // 0
boolean hv  = map.containsValue(Integer.valueOf(2));                   // true
map.remove("one");

// Iterate keys / values / entries (keySet(), values() and entrySet() are Iterable)
for (Object k : map.keySet())   { Log.i("TAG", (String) k); }
for (Object val : map.values()) { Log.i("TAG", String.valueOf((Integer) val)); }
for (Map.Entry<String, Integer> e : map.entrySet()) { Log.i("TAG", e.getKey() + "=" + e.getValue()); }

HashSet set = new HashSet();
set.add("a");
set.add("b");
boolean inSet = set.contains("a");           // true
set.remove("a");
```

Both also have `isEmpty()`, `size()` and `clear()`, and the `(int initialCapacity)` and `(int initialCapacity, float loadFactor)` constructors. A `Map.Entry` answers `getKey()` and `getValue()`; there is no `setValue`.

## `java.util.Iterator` and the enhanced for loop

`ArrayList`, `HashMap` (via `keySet()`, `values()`, `entrySet()`), `HashSet`, and any class of your own that implements `Iterable` work with the enhanced `for` loop and an explicit `Iterator`. `LinkedHashMap`/`LinkedHashSet` are accepted as aliases of `HashMap`/`HashSet` (no insertion order — see the [compatibility matrix](/reference/compatibility-matrix/)).

```java
import java.util.ArrayList;
import java.util.Iterator;

ArrayList items = new ArrayList();
items.add("a"); items.add("b"); items.add("c");

// Enhanced for-each
for (Object o : items) {
    Log.i("TAG", (String) o);
}

// Explicit iterator
Iterator it = items.iterator();
while (it.hasNext()) {
    Log.i("TAG", (String) it.next());
}
```

`Iterator.remove()` is served as well.

## `java.util.Arrays` and `java.util.Collections`

Stable mergesort and a small set of list utilities, with `java.util.Comparator` for an ordering of your own. Mirrors the most-used subset of the Java standard library.

```java
import java.lang.Comparable;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;

// Object[] sort — element type must implement Comparable
String[] words = { "echo", "alpha", "delta", "bravo" };
Arrays.sort(words);                    // in-place stable mergesort
String dump = Arrays.toString(words);  // "[alpha, bravo, delta, echo]"

// Primitive sort (int/long/double/float/short/byte/char overloads), fill, copyOf
int[] nums = { 3, 1, 2 };
Arrays.sort(nums);                     // {1, 2, 3}
int[] zeros = new int[4];
Arrays.fill(zeros, 7);                 // {7, 7, 7, 7}
int[] grown = Arrays.copyOf(nums, 5);  // {1, 2, 3, 0, 0}

// Collections — operate on java.util.List (ArrayList implements it)
ArrayList<Integer> nums = new ArrayList<Integer>();
nums.add(3); nums.add(1); nums.add(2);
Collections.sort(nums);     // [1, 2, 3]
Collections.reverse(nums);  // [3, 2, 1]

// Comparator — a lambda, or a class that implements java.util.Comparator
Arrays.sort(words, (a, b) -> b.compareTo(a));        // descending
Collections.sort(nums, (a, b) -> a.compareTo(b));
```

| Method | Description |
|--------|-------------|
| `Arrays.sort(Object[] a)` | In-place stable mergesort. Elements must implement `Comparable`. |
| `Arrays.sort(int[] a)` | In-place sort of a primitive array (`long`/`double`/`float`/`short`/`byte`/`char` overloads too). |
| `Arrays.fill(a, value)` | Fill every element with `value` (primitive overloads). |
| `Arrays.copyOf(a, newLength)` | Copy, truncating or zero-padding to `newLength` (primitive overloads). |
| `Arrays.sort(T[] a, Comparator<? super T> c)` | Stable sort by `c`; `null` means the natural ordering. |
| `Arrays.toString(Object[] a)` | `"[a, b, c]"` rendering using each element's `toString` (primitive-array overloads too). |
| `Collections.sort(List)` | Stable mergesort over a `List`. Elements must implement `Comparable`. |
| `Collections.sort(List<T> list, Comparator<? super T> c)` | Stable sort by `c`; `null` means the natural ordering. |
| `Collections.reverse(List)` | Reverse the list in place. |

## `java.util.Objects`

The null-safe static helpers, shipped as a real class in the SDK so that `equals`, `hashCode` and
`toString` dispatch to your own overrides:

```java
import java.util.Objects;

boolean same = Objects.equals(a, b);            // true when both null, else a.equals(b)
int h        = Objects.hash(name, id, 3);       // 31·h + hashCode over the boxed arguments
int h0       = Objects.hashCode(null);          // 0
String s     = Objects.toString(null, "none");  // "none"; toString(o) is String.valueOf(o)
Objects.requireNonNull(cfg, "cfg");             // throws NullPointerException("cfg")
if (Objects.nonNull(x) && Objects.isNull(y)) { /* ... */ }
```

`hash(Object...)` boxes its arguments; prefer `hashCode(o)` for a single value on a hot path.

## `java.util.zip.CRC32`

The JDK's CRC-32, with its `Checksum` interface: the IEEE polynomial, reflected, so `"123456789"`
checks to `0xCBF43926`. The per-byte loop is native; the running value and the API are a real class
in the SDK, so a `Checksum` reference dispatches like any other object.

```java
import java.util.zip.CRC32;

CRC32 crc = new CRC32();
crc.update(header);            // byte[]
crc.update(body, 0, bodyLen);  // byte[], off, len
crc.update(0x0A);              // one byte, the low eight bits
long value = crc.getValue();   // unsigned 32-bit checksum in a long
crc.reset();                   // back to an empty stream
```

| Method | Description |
|--------|-------------|
| `update(byte[] b)`, `update(byte[] b, int off, int len)` | Feed bytes; the range form throws `ArrayIndexOutOfBoundsException` for a range off the array. |
| `update(int b)` | Feed one byte. One native call per byte: hand whole arrays over on a hot path. |
| `getValue()` | The checksum of everything fed since construction or `reset()`, as a `long` in `0..2^32`. |
| `reset()` | Start over. |

`SharedPreferences` verifies and signs its file with it; a preferences load on the RP2350 lost some
40 ms of interpreted bytecode when the loop moved out of Java.

## `java.time`

The JDK's date-time classes, ported to the SDK so an app stops doing epoch arithmetic by hand:
`LocalDate`, `LocalTime`, `LocalDateTime`, `Instant`, `Duration`, `ZoneOffset`, `ZoneId`,
`Month`, `DayOfWeek`, `Year`, `DateTimeFormatter`, `ChronoUnit`, and `java.util.TimeZone` for
the default zone. The methods keep their JDK signatures, so the code you would write on Android
compiles and runs unchanged:

```java
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.util.TimeZone;

// The wall clock is the platform's: the time service anchors it from the network after the
// join, and the zone is what the user picked in Settings → Date & time. Nothing to set up.
LocalDateTime now = LocalDateTime.now();                       // in the platform zone
// TimeZone.setDefault still overrides the zone for this process, as on Android:
TimeZone.setDefault(TimeZone.getTimeZone("GMT+05:30"));
String clock = now.format(DateTimeFormatter.ofPattern("HH:mm"));
String date  = now.toLocalDate().format(DateTimeFormatter.ofPattern("EEE, d MMM yyyy"));

Instant resetAt = Instant.ofEpochSecond(json.getLong("resets_at"));
Duration left   = Duration.between(Instant.now(), resetAt);
long hours = left.toHours(), minutes = left.toMinutes() % 60;

LocalDate due = LocalDate.of(2026, Month.SEPTEMBER, 23).plusMonths(1);   // 2026-10-23
long days = ChronoUnit.DAYS.between(LocalDate.now(), due);
LocalDateTime local = LocalDateTime.ofInstant(resetAt, ZoneId.systemDefault());
```

What is not there, and what to do instead:

| Missing | Instead |
|---|---|
| Region zones (`ZoneId.of("Europe/London")` throws `DateTimeException`), daylight saving | A fixed offset: `ZoneOffset.ofHours(1)`, `ZoneId.of("UTC+01:00")`. `TimeZone.getDefault()` / `ZoneId.systemDefault()` is the platform zone (UTC until the user picks one in Settings → Date & time; its id reads `UTC` or `GMT+05:30`), stored across reboots and the same for every app; `setDefault` overrides it for the process. |
| `ZonedDateTime`, `OffsetDateTime`, `Period`, `TemporalField` / `ChronoField`, `TemporalAdjusters` | `LocalDateTime.ofInstant(instant, zone)` and `toInstant(offset)` cross between the time-line and local fields; `ChronoUnit.X.between` and `plusX` cover the arithmetic. |
| Locale-aware text, `DateTimeFormatter.ofLocalizedDate`, parsing with a pattern | `ofPattern` with `y u M L d D E a H h m s S` and `'…'` literals (English month and day names); `LocalDate.parse` and friends read ISO-8601. |
| `Clock`, `Instant.now()` before the clock is set | `now()` reads `System.currentTimeMillis()`, which counts from boot until the platform's time service anchors it (seconds after a WiFi board joins) or `SystemClock.setCurrentTimeMillis` does. |

The package is left out of the `testbench_rp2040` image (`framework_class_excludes`, about 67 KB
of class files it does not have room for), where `verifyApiContract --board` rejects an app that uses it.
`examples/timedemo` exercises the port; its expectations were checked against the JDK's own
`java.time` on a host.

## `java.lang.Comparable`

```java
public class Score implements Comparable<Score> {
    int value;
    public int compareTo(Score other) {
        return this.value - other.value;
    }
}
```

Used by `Arrays.sort` and `Collections.sort`. Boxed numerics (`Integer`, `Long`, `Float`, `Double`) and `String` already implement it.

## Interface-typed collections

`List<E>`, `Set<E>`, `Collection<E>`, `Map<K,V>` and `Iterable<E>` all work as declared types, parameter types and return types — `Map<String, String> m = new HashMap<>();` compiles and runs, as do interface-typed fields, `instanceof`, casts and the enhanced `for` loop:

```java
Map<String, Integer> scores = new HashMap<>();   // interface-typed
List<String> names = new ArrayList<>();
Set<String> seen = new HashSet<>();
Collection<String> asCollection = names;         // widening works
Iterable<String> asIterable = seen;

static int total(Collection<Integer> values) { ... }   // interface parameter
static Map<String, Integer> build() { ... }            // interface return
```

These interfaces are **built into the JVM** rather than shipped as SDK source: your app compiles against the JDK's own `java.util` declarations, and at run time the call dispatches on the receiver's actual class (`ArrayList`, `HashMap`, `HashSet`, or a class of your own). `ArrayList` is the only concrete `List` in v1.

The catch: because the compiler sees the JDK's full interfaces, it will also accept members picodroid does **not** implement (`map.forEach`, `list.removeIf`, `Map.putAll`, `new TreeMap<>()`), which fail at run time instead of at build time. Stick to the members listed in the [compatibility matrix](/reference/compatibility-matrix/).

## `java.lang.Class`

Class literals (`MyType.class`) and reflection-lite: `getName()`, `forName(String)` and `newInstance()`. There is no `Field`, `Method` or `Constructor` API, no member discovery and no access check.

```java
Class<?> c = String.class;
String name = c.getName();      // "java.lang.String"
boolean same = (s.getClass() == String.class);  // true — Class instances are interned

// Each evaluation of `T.class` returns the same Class instance
boolean stable = (Direction.class == Direction.class);  // true

// By name, as getName() spells it (so it holds when the shrinker renames the app's classes):
Class<?> k = Class.forName(Gauge.class.getName());       // ClassNotFoundException if not packed
Object g = k.newInstance();                              // the public no-argument constructor;
                                                         // InstantiationException otherwise
```

`toString()` is `"class " + getName()`. `forName` finds a class packed with the app or the framework (every class is in flash already; nothing is loaded) and `newInstance()` runs the no-argument constructor, initialising the class first if it never was. This is what the framework uses to construct a ViewModel (`ViewModelProvider.NewInstanceFactory`), a saved Fragment (the default `FragmentFactory`) and a custom view a layout names (`LayoutInflater`), as Android does, so none of those needs a factory of the app's own. `IllegalAccessException` is declared but never thrown: there are no access checks.

`Object.getClass()` returns the runtime `Class<?>` of any reference. Useful for type-safe equality (`.getClass() == Foo.class`) and for log dispatch keyed by class identity.

## `java.lang.AutoCloseable` and try-with-resources

```java
public interface AutoCloseable {
    void close() throws Exception;
}
```

Any class that implements `AutoCloseable` works in `try`-with-resources — the compiler calls `close()` on exit (normal or exceptional). The `picodroid.pio.*` peripheral handles all implement it, so the idiomatic pattern is:

```java
try (Gpio led = pm.openGpio("GP25")) {
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    led.setValue(true);
} // led.close() runs here.
```

Multiple resources in one `try` close in reverse-declaration order. See [`examples/trywithresourcesdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/trywithresourcesdemo) for a worked example.

## Enums

Java `enum` declarations are supported. Each enum constant is a singleton; `values()`, `valueOf(String)`, `name()`, `ordinal()`, and `switch (myEnum)` all work. `valueOf` throws `IllegalArgumentException` for a name that is not a constant, as in Java.

```java
public enum Direction { NORTH, EAST, SOUTH, WEST }

Direction d = Direction.NORTH;
String name = d.name();        // "NORTH"
int    ord  = d.ordinal();     // 0
Direction w = Direction.valueOf("WEST");
for (Direction dir : Direction.values()) {
    Log.i("TAG", dir.name());
}

switch (d) {
    case NORTH: Log.i("TAG", "up");    break;
    case SOUTH: Log.i("TAG", "down");  break;
    default:    Log.i("TAG", "side");  break;
}
```

## Boxed primitives (wrapper classes)

`Integer`, `Long`, `Short`, `Byte`, `Float`, `Double`, `Boolean`, and `Character` are available as
object wrappers. Each supports `valueOf(primitive)`, the matching unboxing accessor, and `toString()`:

```java
Integer boxed = Integer.valueOf(42);
int     back  = boxed.intValue();         // 42
String  s     = boxed.toString();         // "42"

Boolean   b = Boolean.valueOf(true);     boolean bv = b.booleanValue();
Long      l = Long.valueOf(9000L);       long    lv = l.longValue();
Float     f = Float.valueOf(3.14f);      float   fv = f.floatValue();
Double    d = Double.valueOf(2.71828);   double  dv = d.doubleValue();
Character c = Character.valueOf('X');     char    cv = c.charValue();
```

You rarely call these directly: `ArrayList<Integer>` and `HashMap` keys/values **autobox** through
`valueOf` and auto-unbox through the `*Value()` accessors.

Parsing and comparing:

```java
int    n  = Integer.parseInt("42");          // NumberFormatException for text that is not a number
long   l2 = Long.parseLong("9000");
double d2 = Double.parseDouble("2.5");       // Float.parseFloat, Short.parseShort, Byte.parseByte too
boolean t = Boolean.parseBoolean("true");
Integer b2 = Integer.valueOf("42");          // valueOf(String) on every numeric wrapper
String  s2 = Integer.toString(42);           // the static toString(x) of each numeric wrapper
int     c2 = Integer.compare(3, 7);          // negative; compare(x, y) and compareTo on every wrapper

boolean dg = Character.isDigit('7');         // isLetter, toUpperCase, toLowerCase — ASCII only
```

The numeric wrappers convert through `byteValue()`, `shortValue()`, `intValue()`, `longValue()`,
`floatValue()` and `doubleValue()` as Java does. `Float.isNaN` / `Double.isNaN` / `isInfinite` are
not served: test `f != f` for NaN.

`Float.floatToIntBits(f)` and `Float.intBitsToFloat(i)` round-trip the IEEE-754 bit pattern — for a
bit-exact `equals`/`hashCode`, or to keep a float in an `int` slot.

## Exceptions

`java.lang.Throwable`, `Exception`, and `RuntimeException` are supported, each with the standard
no-arg and `(String message)` constructors. Standard subclasses such as `IllegalArgumentException`
and `IllegalStateException` are also throwable with a message. Define your own by extending
`Exception` (or `RuntimeException`), then `throw` / `catch` as usual — `catch` matches subclasses
of the declared type:

```java
public class AppException extends Exception {
    public AppException(String message) { super(message); }
}

try {
    if (bad) throw new AppException("bad input");
} catch (AppException e) {
    Log.i("TAG", "caught an AppException");
}
```

The runtime throws the standard unchecked exceptions where Java does, and each can be caught by
name: `NullPointerException`, `ArithmeticException`, `ArrayIndexOutOfBoundsException`,
`StringIndexOutOfBoundsException`, `IndexOutOfBoundsException`, `NegativeArraySizeException`,
`ArrayStoreException`, `ClassCastException`, `NumberFormatException`,
`UnsupportedOperationException`, `IllegalMonitorStateException`, `IllegalThreadStateException`,
`java.util.NoSuchElementException` and `java.util.ConcurrentModificationException`, with the
checked `InterruptedException` and `java.io.IOException`, and the errors `OutOfMemoryError`,
`StackOverflowError` and `ExceptionInInitializerError`.

The message and cause passed to a constructor are captured by the runtime: `getMessage()`,
`getCause()`, `addSuppressed()` and `getSuppressed()` read them back, and an uncaught throw prints
them. **Caveat:** there is no stack-trace API (`getStackTrace()` / `printStackTrace()`) — the trace
is printed by the runtime when a throw goes uncaught. See
[`examples/exceptiondemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/exceptiondemo).

## `java.util.Random`

A seedable pseudo-random generator. A fixed seed gives a reproducible stream.

```java
import java.util.Random;

Random r = new Random();          // or new Random(42L) for a fixed seed
r.setSeed(42L);

int     i  = r.nextInt();          // any int
int     i2 = r.nextInt(100);       // 0..99
long    l  = r.nextLong();
boolean b  = r.nextBoolean();
float   f  = r.nextFloat();        // [0.0, 1.0)
double  d  = r.nextDouble();       // [0.0, 1.0)
double  g  = r.nextGaussian();     // mean 0.0, stddev 1.0
byte[]  buf = new byte[8];
r.nextBytes(buf);                  // fill with random bytes
```

See [`examples/randomdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/randomdemo).

## `java.lang.System` and `java.lang.Runnable`

`System.currentTimeMillis()` returns wall-clock milliseconds (see
[System & concurrency](/api/system/#javalangsystemcurrenttimemillis) for the full timing surface, including
`SystemClock`). `System.arraycopy(src, srcPos, dest, destPos, length)` copies between arrays of
the same element type (`ArrayStoreException` otherwise, `IndexOutOfBoundsException` for a range
off either array); an overlapping copy within one array is safe. `java.lang.Runnable` is the
standard `void run()` interface — used by `Thread`, `Executors`, and (historically) view callbacks:

```java
Runnable task = () -> Log.i("TAG", "ran");
task.run();
```

## Lambdas and method references

A lambda or a method reference can stand in for any interface with a single abstract method —
`Runnable`, `Comparator`, a listener, or an interface of your own. Lambdas may capture local
variables. Method references resolve to static methods, bound and unbound instance methods,
constructors and the built-in classes' methods:

```java
interface Fn<A, R> { R apply(A a); }
interface Factory<T> { T create(); }

Fn<Integer, Integer> twice   = MyApp::twice;        // static method
Fn<Integer, Integer> add     = this::instanceAdd;   // bound instance method
Fn<String, Integer>  length  = String::length;      // unbound, on a built-in class
Factory<Counter>     counter = Counter::new;        // constructor
```

There is no `java.util.function` package (`Function`, `Supplier`, `Predicate`, …): declare the
single-method interface you need. See
[`examples/lambdademo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/lambdademo).

---

**See also:** [System & concurrency](/api/system/) · [Peripherals](/api/peripherals/) · [Storage](/api/storage/) · [Networking](/api/networking/) · [Graphics & UI](/api/ui/)
