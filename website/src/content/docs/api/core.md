---
title: "Core Java Language Surface"
description: "java.lang and java.util classes available in Picodroid apps."
---

`java.lang.*` and `java.util.*` types implemented by the Picodroid JVM. See [Java API overview](/api/) for the full API index.

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

// Static factory
String vi = String.valueOf(42);       // "42"
String vl = String.valueOf(9000L);    // "9000"
String vb = String.valueOf(true);     // "true"

// Extended methods
String[] parts = "a,b,c".split(",");           // ["a", "b", "c"]
String r       = "foo bar".replace(' ', '_'); // "foo_bar"
String c       = "Hello, ".concat("World");    // "Hello, World"
char[] chs     = "abc".toCharArray();          // {'a', 'b', 'c'}
int    h       = "abc".hashCode();             // standard Java String hash
```

> **StringBuilder interaction:** `+` string concatenation compiles to a compiler-generated `StringBuilder` that shares the JVM's single internal buffer. If you build a `StringBuilder` manually and then log `"prefix=" + sb.toString()`, the compiler's `StringBuilder` will clear the buffer before `sb.toString()` is evaluated. Capture the result first:
>
> ```java
> String result = sb.toString();   // snapshot the buffer
> Log.i(TAG, "prefix=" + result);  // safe to concatenate now
> ```

## `java.lang.StringBuilder`

```java
StringBuilder sb = new StringBuilder();         // empty
StringBuilder sb = new StringBuilder("prefix="); // with initial content

sb.append("text");    // append String
sb.append(42);        // append int
sb.append(3.14f);     // append float  (formats as "3.14")
sb.append(100L);      // append long
sb.append(true);      // append "true" or "false"
sb.append('x');       // append char

int  len = sb.length();    // current content length
char ch  = (char) sb.charAt(2);  // byte at position 2

String s = sb.toString();  // intern result as a String
```

> **Single shared buffer:** all `StringBuilder` instances in the JVM share one underlying buffer. Creating a new `StringBuilder` (including the compiler-generated one for `+` concatenation) clears that buffer. Build one `StringBuilder` at a time and call `toString()` before starting another.

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
list.clear();

// Indexed insert
list.add(0, "first");   // insert at position 0

// Generic type with autoboxing (Integer, Boolean, Long, Float, Double)
ArrayList<Integer> nums = new ArrayList<Integer>();
nums.add(10);    // autoboxes int → Integer
nums.add(20);
int n = nums.get(0);          // auto-unboxes Integer → int  (10)
boolean has = nums.contains(20);  // true — value equality for wrappers
```

> **Autoboxing:** `ArrayList<Integer>` works as expected — `add(42)` and `contains(42)` both box via `Integer.valueOf`. For raw `ArrayList`, store and retrieve Object references (String, custom class instances); do not store bare primitives without explicit boxing (`Integer.valueOf(42)`, etc.).

## `java.util.HashMap` and `java.util.HashSet`

Hash-table-backed associative containers. Keys are compared by `equals()` / `hashCode()`; autoboxed primitives (`Integer`, `Long`, `Boolean`, `String`) all work as keys.

```java
import java.util.HashMap;
import java.util.HashSet;

HashMap map = new HashMap();
map.put("one", Integer.valueOf(1));
map.put("two", Integer.valueOf(2));

Integer v   = (Integer) map.get("one");      // 1
boolean has = map.containsKey("two");        // true
int     n   = map.size();                    // 2
map.remove("one");

HashSet set = new HashSet();
set.add("a");
set.add("b");
boolean inSet = set.contains("a");           // true
```

## `java.util.Iterator` and the enhanced for loop

`ArrayList`, `HashMap` (via `keySet()`), and `HashSet` implement `Iterable`, so the enhanced `for` loop and explicit `Iterator` both work.

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

## `java.util.Arrays` and `java.util.Collections`

Stable mergesort and a small set of list utilities. Mirrors the most-used subset of the Java standard library.

```java
import java.lang.Comparable;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;

// Object[] sort — element type must implement Comparable
String[] words = { "echo", "alpha", "delta", "bravo" };
Arrays.sort(words);                    // in-place stable mergesort
String dump = Arrays.toString(words);  // "[alpha, bravo, delta, echo]"

// Collections — operate on java.util.List (ArrayList implements it)
ArrayList<Integer> nums = new ArrayList<Integer>();
nums.add(3); nums.add(1); nums.add(2);
Collections.sort(nums);     // [1, 2, 3]
Collections.reverse(nums);  // [3, 2, 1]
```

| Method | Description |
|--------|-------------|
| `Arrays.sort(Object[] a)` | In-place stable mergesort. Elements must implement `Comparable`. |
| `Arrays.toString(Object[] a)` | `"[a, b, c]"` rendering using each element's `toString`. |
| `Collections.sort(List)` | Stable mergesort over a `List`. Elements must implement `Comparable`. |
| `Collections.reverse(List)` | Reverse the list in place. |

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

## `java.util.List`

A minimal `List<E>` interface (`size`, `get`, `set`, `add`, `contains`, `isEmpty`, `clear`) — implemented by `ArrayList`. Provided so `Collections.sort` / `reverse` can accept any list type. There are no other concrete `List` implementations in v1.

## `java.lang.Class`

Class literals (`MyType.class`) and reflection-lite. `Class<?>` is the only reflective surface — there's no `Field` or `Method` API in v1.

```java
Class<?> c = String.class;
String name = c.getName();      // "java.lang.String"
boolean same = (s.getClass() == String.class);  // true — Class instances are interned

// Each evaluation of `T.class` returns the same Class instance
boolean stable = (Direction.class == Direction.class);  // true
```

`Object.getClass()` returns the runtime `Class<?>` of any reference. Useful for type-safe equality (`.getClass() == Foo.class`) and for log dispatch keyed by class identity.

## `java.lang.AutoCloseable` and try-with-resources

```java
public interface AutoCloseable {
    void close();
}
```

Any class that implements `AutoCloseable` works in `try`-with-resources — the compiler calls `close()` on exit (normal or exceptional). The `picodroid.pio.*` peripheral handles all implement it, so the idiomatic pattern is:

```java
try (Gpio led = pm.openGpio("GP25")) {
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    led.setValue(true);
} // led.close() runs here — releases the pin back to the PeripheralManager.
```

Multiple resources in one `try` close in reverse-declaration order. See [`examples/trywithresourcesdemo/`](https://github.com/shivrajora/picodroid-rs/tree/main/examples/trywithresourcesdemo) for a worked example.

## Enums

Java `enum` declarations are supported. Each enum constant is a singleton; `values()`, `name()`, `ordinal()`, and `switch (myEnum)` all work.

```java
public enum Direction { NORTH, EAST, SOUTH, WEST }

Direction d = Direction.NORTH;
String name = d.name();        // "NORTH"
int    ord  = d.ordinal();     // 0
for (Direction dir : Direction.values()) {
    Log.i("TAG", dir.name());
}

switch (d) {
    case NORTH: Log.i("TAG", "up");    break;
    case SOUTH: Log.i("TAG", "down");  break;
    default:    Log.i("TAG", "side");  break;
}
```

---

**See also:** [System & concurrency](/api/system/) · [Peripherals](/api/peripherals/) · [Storage](/api/storage/) · [Networking](/api/networking/) · [Graphics & UI](/api/ui/)
