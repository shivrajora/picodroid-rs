// SPDX-License-Identifier: GPL-3.0-only
package kotlin.collections;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import kotlin.Pair;

/**
 * The non-inline part of {@code kotlin.collections} for maps. {@code mutableMapOf()}, {@code
 * getOrPut}, {@code getOrElse}, {@code forEach}, {@code filter}, {@code mapValues} are inline and
 * use {@code LinkedHashMap} + {@code entrySet()} directly (both served by the JVM; {@code
 * LinkedHashMap} is an alias of {@code HashMap}, so insertion order is not preserved).
 */
public final class MapsKt {
  private MapsKt() {}

  private static final HashMap<Object, Object> EMPTY = new HashMap<Object, Object>();

  @SuppressWarnings("unchecked")
  private static void putAll(Map<Object, Object> target, Map source) {
    for (Object o : source.entrySet()) {
      Map.Entry e = (Map.Entry) o;
      target.put(e.getKey(), e.getValue());
    }
  }

  private static void putPairs(Map<Object, Object> target, Pair[] pairs) {
    for (Pair p : pairs) {
      target.put(p.getFirst(), p.getSecond());
    }
  }

  /** Initial capacity for {@code expectedSize} entries at load factor 0.75 (inline callers). */
  public static int mapCapacity(int expectedSize) {
    if (expectedSize < 0) {
      return expectedSize;
    }
    if (expectedSize < 3) {
      return expectedSize + 1;
    }
    if (expectedSize < (1 << 30)) {
      return (int) ((float) expectedSize / 0.75f + 1.0f);
    }
    return Integer.MAX_VALUE;
  }

  public static Map emptyMap() {
    return EMPTY;
  }

  public static Map mapOf(Pair[] pairs) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(pairs.length));
    putPairs(out, pairs);
    return out;
  }

  public static Map mapOf(Pair pair) {
    HashMap<Object, Object> out = new HashMap<Object, Object>();
    out.put(pair.getFirst(), pair.getSecond());
    return out;
  }

  public static Map mutableMapOf(Pair[] pairs) {
    return mapOf(pairs);
  }

  public static HashMap hashMapOf(Pair[] pairs) {
    HashMap<Object, Object> out = new HashMap<Object, Object>(mapCapacity(pairs.length));
    putPairs(out, pairs);
    return out;
  }

  public static LinkedHashMap linkedMapOf(Pair[] pairs) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(pairs.length));
    putPairs(out, pairs);
    return out;
  }

  public static Object getValue(Map map, Object key) {
    Object value = map.get(key);
    if (value == null && !map.containsKey(key)) {
      throw new NoSuchElementException("Key " + key + " is missing in the map.");
    }
    return value;
  }

  public static Map toMap(Map source) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(source.size()));
    putAll(out, source);
    return out;
  }

  public static Map toMutableMap(Map source) {
    return toMap(source);
  }

  public static List toList(Map source) {
    ArrayList<Object> out = new ArrayList<Object>(source.size());
    for (Object o : source.entrySet()) {
      Map.Entry e = (Map.Entry) o;
      out.add(new Pair(e.getKey(), e.getValue()));
    }
    return out;
  }

  public static Map plus(Map source, Pair pair) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(source.size() + 1));
    putAll(out, source);
    out.put(pair.getFirst(), pair.getSecond());
    return out;
  }

  public static Map plus(Map source, Map other) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(source.size() + other.size()));
    putAll(out, source);
    putAll(out, other);
    return out;
  }

  public static Map minus(Map source, Object key) {
    LinkedHashMap<Object, Object> out =
        new LinkedHashMap<Object, Object>(mapCapacity(source.size()));
    putAll(out, source);
    out.remove(key);
    return out;
  }
}
