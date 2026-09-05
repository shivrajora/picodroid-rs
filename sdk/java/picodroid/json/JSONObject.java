// SPDX-License-Identifier: GPL-3.0-only
package picodroid.json;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;

/**
 * A modifiable set of name/value mappings, mirroring {@code org.json.JSONObject} (Android ships it
 * as {@code org.json}; the picodroid namespace rule makes it {@code picodroid.json}). Names are
 * unique, non-null strings; values are {@link JSONObject}, {@link JSONArray}, {@code String},
 * {@code Boolean}, {@code Integer}, {@code Long}, {@code Double} or {@link #NULL}. Insertion order
 * is kept, as Android's is.
 *
 * <p><b>Storage.</b> The document lives in a native node pool; this object holds only the index of
 * its node. Values are materialized into Java objects on {@code get}/{@code opt}, and {@link
 * #getJSONObject}/{@link #getJSONArray} return a <em>new</em> wrapper over the shared node each
 * call: mutations through it are visible in the parent, as on Android, but two such wrappers are
 * not {@code ==}. Nodes are reclaimed once no wrapper reaches them. Any other kind of object put
 * here is stored as its {@code toString()}.
 *
 * <p><b>Divergences from Android.</b> The parser is strict RFC 8259 (no single quotes, comments or
 * unquoted keys); {@link #keySet()} is an unordered copy ({@link #keys()} is ordered); a very large
 * or very small double prints as {@code 1e21} rather than {@code 1.0E21}; {@code JSONTokener} and
 * {@code JSONStringer} do not exist. Available only on boards whose {@code board.toml} sets {@code
 * has_json = true}.
 */
public class JSONObject {
  // Value kinds reported by nativeKind — keep in step with picodroid-core/src/json/mod.rs.
  static final int K_NULL = 0;
  static final int K_BOOL = 1;
  static final int K_INT = 2;
  static final int K_LONG = 3;
  static final int K_DOUBLE = 4;
  static final int K_STRING = 5;
  static final int K_OBJECT = 6;
  static final int K_ARRAY = 7;

  /** Native status: the pool is full (nodes or bytes). */
  static final int ST_EXHAUSTED = -1;

  /** Native status: the link would make a container reach itself. */
  static final int ST_CYCLE = -2;

  /**
   * A sentinel value used to explicitly define a name with no value, distinct from a missing name.
   * Equal to {@code null} and to itself, and prints as {@code "null"}, as Android's does.
   */
  public static final Object NULL =
      new Object() {
        @Override
        @SuppressWarnings("ReferenceEquality")
        public boolean equals(Object o) {
          return o == this || o == null;
        }

        @Override
        public int hashCode() {
          return 0;
        }

        @Override
        public String toString() {
          return "null";
        }
      };

  /** Node index in the native pool; bound to this wrapper for as long as it is reachable. */
  final int handle;

  /** Creates an empty object. */
  public JSONObject() {
    int h = nativeNewObject(this);
    if (h < 0) {
      throw new OutOfMemoryError("JSON pool exhausted");
    }
    handle = h;
  }

  /**
   * Parses {@code json}, which must be a JSON object.
   *
   * @throws JSONException if the text is not a valid JSON object
   */
  public JSONObject(String json) throws JSONException {
    if (json == null) {
      throw new NullPointerException("json == null");
    }
    int h = nativeParse(this, json, false);
    if (h < 0) {
      throw new JSONException(nativeLastError());
    }
    handle = h;
  }

  /**
   * Creates an object with the entries of {@code copyFrom}. Keys must be non-null strings; values
   * go through {@link #wrap}.
   */
  public JSONObject(Map copyFrom) {
    this();
    Map<?, ?> map = copyFrom;
    for (Map.Entry<?, ?> entry : map.entrySet()) {
      String key = (String) entry.getKey();
      if (key == null) {
        throw new NullPointerException("key == null");
      }
      putRaw(key, wrap(entry.getValue()));
    }
  }

  /** Creates an object with the named entries of {@code copyFrom}; names it lacks are skipped. */
  public JSONObject(JSONObject copyFrom, String[] names) throws JSONException {
    this();
    for (String name : names) {
      Object value = copyFrom.opt(name);
      if (value != null) {
        putRaw(name, value);
      }
    }
  }

  /** Wraps an existing node (a child reached through its parent). */
  JSONObject(int node) {
    handle = node;
    nativeBind(this, node);
  }

  /** Returns the number of name/value mappings. */
  public int length() {
    return nativeLength(handle);
  }

  // ── put ─────────────────────────────────────────────────────────────────

  /** Maps {@code name} to {@code value}, clobbering any existing mapping. */
  public JSONObject put(String name, boolean value) throws JSONException {
    status(nativePutBool(handle, checkName(name), value));
    return this;
  }

  /**
   * Maps {@code name} to {@code value}, clobbering any existing mapping.
   *
   * @throws JSONException if {@code value} is NaN or infinite
   */
  public JSONObject put(String name, double value) throws JSONException {
    status(nativePutDouble(handle, checkName(name), checkDouble(value)));
    return this;
  }

  /** Maps {@code name} to {@code value}, clobbering any existing mapping. */
  public JSONObject put(String name, int value) throws JSONException {
    status(nativePutInt(handle, checkName(name), value));
    return this;
  }

  /** Maps {@code name} to {@code value}, clobbering any existing mapping. */
  public JSONObject put(String name, long value) throws JSONException {
    status(nativePutLong(handle, checkName(name), value));
    return this;
  }

  /**
   * Maps {@code name} to {@code value}, clobbering any existing mapping. A {@code null} value
   * removes the mapping; use {@link #NULL} to store an explicit null.
   *
   * @throws JSONException if {@code name} is null or {@code value} is a NaN or infinite double
   */
  public JSONObject put(String name, Object value) throws JSONException {
    checkName(name);
    if (value == null) {
      nativeRemove(handle, name);
      return this;
    }
    if (value instanceof Double) {
      checkDouble(((Double) value).doubleValue());
    } else if (value instanceof Float) {
      checkDouble(((Float) value).floatValue());
    }
    putRaw(name, value);
    return this;
  }

  /** Equivalent to {@code put(name, value)} when both are non-null; does nothing otherwise. */
  public JSONObject putOpt(String name, Object value) throws JSONException {
    if (name == null || value == null) {
      return this;
    }
    return put(name, value);
  }

  /**
   * Appends {@code value} to the array already mapped to {@code name}. If there is no such mapping
   * this is the same as {@link #put(String, Object)}; if the existing value is not an array it is
   * replaced by an array holding the old value and the new one.
   */
  public JSONObject accumulate(String name, Object value) throws JSONException {
    Object current = opt(checkName(name));
    if (current == null) {
      return put(name, value);
    }
    if (current instanceof JSONArray) {
      ((JSONArray) current).checkedPut(value);
    } else {
      JSONArray array = new JSONArray();
      array.checkedPut(current);
      array.checkedPut(value);
      putRaw(name, array);
    }
    return this;
  }

  /**
   * Appends {@code value} to the array mapped to {@code name}, creating the array if there is no
   * mapping yet.
   *
   * @throws JSONException if {@code name} is mapped to something other than an array
   */
  public JSONObject append(String name, Object value) throws JSONException {
    Object current = opt(checkName(name));
    JSONArray array;
    if (current instanceof JSONArray) {
      array = (JSONArray) current;
    } else if (current == null) {
      array = new JSONArray();
      putRaw(name, array);
    } else {
      throw new JSONException("Key " + name + " is not a JSONArray");
    }
    array.checkedPut(value);
    return this;
  }

  /** Stores {@code value} under {@code name} by its type; the caller has validated both. */
  void putRaw(String name, Object value) {
    int r;
    if (isNullSentinel(value)) {
      r = nativePutNull(handle, name);
    } else if (value instanceof JSONObject) {
      r = nativePutNode(handle, name, ((JSONObject) value).handle);
    } else if (value instanceof JSONArray) {
      r = nativePutNode(handle, name, ((JSONArray) value).handle);
    } else if (value instanceof Boolean) {
      r = nativePutBool(handle, name, ((Boolean) value).booleanValue());
    } else if (value instanceof Integer) {
      r = nativePutInt(handle, name, ((Integer) value).intValue());
    } else if (value instanceof Long) {
      r = nativePutLong(handle, name, ((Long) value).longValue());
    } else if (value instanceof Double) {
      r = nativePutDouble(handle, name, ((Double) value).doubleValue());
    } else if (value instanceof Float) {
      r = nativePutDouble(handle, name, ((Float) value).floatValue());
    } else if (value instanceof Number) {
      r = nativePutDouble(handle, name, Double.parseDouble(value.toString()));
    } else if (value instanceof String) {
      r = nativePutString(handle, name, (String) value);
    } else {
      r = nativePutString(handle, name, String.valueOf(value));
    }
    status(r);
  }

  /** Whether {@code value} is {@code null} or the {@link #NULL} sentinel. */
  static boolean isNullSentinel(Object value) {
    return value == null || NULL.equals(value);
  }

  /** Turns a native put/set status into the matching unchecked error. */
  static void status(int r) {
    if (r == ST_CYCLE) {
      throw new IllegalArgumentException("A JSON value cannot contain itself");
    }
    if (r < 0) {
      throw new OutOfMemoryError("JSON pool exhausted");
    }
  }

  static String checkName(String name) throws JSONException {
    if (name == null) {
      throw new JSONException("Names must be non-null");
    }
    return name;
  }

  /**
   * Rejects NaN and infinities, which JSON cannot represent. Spelled with comparisons because
   * {@code Double.isNaN}/{@code isInfinite} are not served by the runtime: a NaN is ordered against
   * nothing, and {@code Double.MAX_VALUE} is a compile-time constant.
   */
  static double checkDouble(double d) throws JSONException {
    boolean ordered = d <= 0.0 || d >= 0.0;
    if (!ordered || d > Double.MAX_VALUE || d < -Double.MAX_VALUE) {
      throw new JSONException("Forbidden numeric value: " + d);
    }
    return d;
  }

  // ── structure ───────────────────────────────────────────────────────────

  /**
   * Removes the named mapping, returning its value or {@code null} if there was none.
   *
   * <p>The value is boxed <em>before</em> the mapping is unlinked: boxing allocates, an allocation
   * may collect, and a collection sweeps any node no wrapper reaches. The returned object is what
   * keeps the node alive from here on.
   */
  public Object remove(String name) {
    if (name == null) {
      return null;
    }
    Object value = opt(name);
    if (value != null) {
      nativeRemove(handle, name);
    }
    return value;
  }

  /**
   * Returns true if this object has no mapping for {@code name} or if it has a mapping whose value
   * is {@link #NULL}.
   */
  public boolean isNull(String name) {
    if (name == null) {
      return true;
    }
    int node = nativeChild(handle, name);
    return node < 0 || nativeKind(node) == K_NULL;
  }

  /** Returns true if this object has a mapping for {@code name}, even one to {@link #NULL}. */
  public boolean has(String name) {
    return name != null && nativeChild(handle, name) >= 0;
  }

  // ── get / opt ───────────────────────────────────────────────────────────

  /**
   * Returns the value mapped by {@code name}.
   *
   * @throws JSONException if no such mapping exists
   */
  public Object get(String name) throws JSONException {
    Object result = opt(name);
    if (result == null) {
      throw new JSONException("No value for " + name);
    }
    return result;
  }

  /** Returns the value mapped by {@code name}, or null if no such mapping exists. */
  public Object opt(String name) {
    if (name == null) {
      return null;
    }
    int node = nativeChild(handle, name);
    return node < 0 ? null : box(node);
  }

  /**
   * Returns the value mapped by {@code name} if it is a boolean or can be coerced to one.
   *
   * @throws JSONException if the mapping is missing or cannot be coerced
   */
  public boolean getBoolean(String name) throws JSONException {
    Object object = get(name);
    Boolean result = toBoolean(object);
    if (result == null) {
      throw typeMismatch(name, object, "boolean");
    }
    return result.booleanValue();
  }

  /** Returns the value mapped by {@code name} coerced to a boolean, or false otherwise. */
  public boolean optBoolean(String name) {
    return optBoolean(name, false);
  }

  /** Returns the value mapped by {@code name} coerced to a boolean, or {@code fallback}. */
  public boolean optBoolean(String name, boolean fallback) {
    Boolean result = toBoolean(opt(name));
    return result != null ? result.booleanValue() : fallback;
  }

  /**
   * Returns the value mapped by {@code name} if it is a double or can be coerced to one.
   *
   * @throws JSONException if the mapping is missing or cannot be coerced
   */
  public double getDouble(String name) throws JSONException {
    Object object = get(name);
    Double result = toDouble(object);
    if (result == null) {
      throw typeMismatch(name, object, "double");
    }
    return result.doubleValue();
  }

  /** Returns the value mapped by {@code name} coerced to a double, or {@code NaN} otherwise. */
  public double optDouble(String name) {
    return optDouble(name, Double.NaN);
  }

  /** Returns the value mapped by {@code name} coerced to a double, or {@code fallback}. */
  public double optDouble(String name, double fallback) {
    Double result = toDouble(opt(name));
    return result != null ? result.doubleValue() : fallback;
  }

  /**
   * Returns the value mapped by {@code name} if it is an int or can be coerced to one.
   *
   * @throws JSONException if the mapping is missing or cannot be coerced
   */
  public int getInt(String name) throws JSONException {
    Object object = get(name);
    Integer result = toInteger(object);
    if (result == null) {
      throw typeMismatch(name, object, "int");
    }
    return result.intValue();
  }

  /** Returns the value mapped by {@code name} coerced to an int, or 0 otherwise. */
  public int optInt(String name) {
    return optInt(name, 0);
  }

  /** Returns the value mapped by {@code name} coerced to an int, or {@code fallback}. */
  public int optInt(String name, int fallback) {
    Integer result = toInteger(opt(name));
    return result != null ? result.intValue() : fallback;
  }

  /**
   * Returns the value mapped by {@code name} if it is a long or can be coerced to one.
   *
   * @throws JSONException if the mapping is missing or cannot be coerced
   */
  public long getLong(String name) throws JSONException {
    Object object = get(name);
    Long result = toLong(object);
    if (result == null) {
      throw typeMismatch(name, object, "long");
    }
    return result.longValue();
  }

  /** Returns the value mapped by {@code name} coerced to a long, or 0 otherwise. */
  public long optLong(String name) {
    return optLong(name, 0L);
  }

  /** Returns the value mapped by {@code name} coerced to a long, or {@code fallback}. */
  public long optLong(String name, long fallback) {
    Long result = toLong(opt(name));
    return result != null ? result.longValue() : fallback;
  }

  /**
   * Returns the value mapped by {@code name} if it exists, coercing it to a string if necessary.
   *
   * @throws JSONException if no such mapping exists
   */
  public String getString(String name) throws JSONException {
    Object object = get(name);
    String result = toString(object);
    if (result == null) {
      throw typeMismatch(name, object, "String");
    }
    return result;
  }

  /** Returns the value mapped by {@code name} coerced to a string, or the empty string. */
  public String optString(String name) {
    return optString(name, "");
  }

  /** Returns the value mapped by {@code name} coerced to a string, or {@code fallback}. */
  public String optString(String name, String fallback) {
    String result = toString(opt(name));
    return result != null ? result : fallback;
  }

  /**
   * Returns the value mapped by {@code name} if it is a {@link JSONArray}.
   *
   * @throws JSONException if the mapping is missing or is not an array
   */
  public JSONArray getJSONArray(String name) throws JSONException {
    Object object = get(name);
    if (object instanceof JSONArray) {
      return (JSONArray) object;
    }
    throw typeMismatch(name, object, "JSONArray");
  }

  /** Returns the value mapped by {@code name} if it is a {@link JSONArray}, or null. */
  public JSONArray optJSONArray(String name) {
    Object object = opt(name);
    return object instanceof JSONArray ? (JSONArray) object : null;
  }

  /**
   * Returns the value mapped by {@code name} if it is a {@link JSONObject}.
   *
   * @throws JSONException if the mapping is missing or is not an object
   */
  public JSONObject getJSONObject(String name) throws JSONException {
    Object object = get(name);
    if (object instanceof JSONObject) {
      return (JSONObject) object;
    }
    throw typeMismatch(name, object, "JSONObject");
  }

  /** Returns the value mapped by {@code name} if it is a {@link JSONObject}, or null. */
  public JSONObject optJSONObject(String name) {
    Object object = opt(name);
    return object instanceof JSONObject ? (JSONObject) object : null;
  }

  /**
   * Returns an array holding the values mapped by each of {@code names}, or null if {@code names}
   * is null or empty.
   */
  public JSONArray toJSONArray(JSONArray names) throws JSONException {
    if (names == null) {
      return null;
    }
    int length = names.length();
    if (length == 0) {
      return null;
    }
    JSONArray result = new JSONArray();
    for (int i = 0; i < length; i++) {
      result.put(opt(toString(names.opt(i))));
    }
    return result;
  }

  // ── names ───────────────────────────────────────────────────────────────

  /** Returns an iterator over the names, in insertion order. */
  public Iterator<String> keys() {
    return keyList().iterator();
  }

  /** Returns the names as a set. Unlike Android's this is a copy, and unordered. */
  public Set<String> keySet() {
    ArrayList<String> list = keyList();
    HashSet<String> set = new HashSet<String>();
    for (int i = 0; i < list.size(); i++) {
      set.add(list.get(i));
    }
    return set;
  }

  private ArrayList<String> keyList() {
    int n = length();
    ArrayList<String> list = new ArrayList<String>();
    for (int i = 0; i < n; i++) {
      list.add(nativeKeyAt(handle, i));
    }
    return list;
  }

  /** Returns an array containing the names, in insertion order, or null if this object is empty. */
  public JSONArray names() {
    int n = length();
    if (n == 0) {
      return null;
    }
    JSONArray result = new JSONArray();
    for (int i = 0; i < n; i++) {
      result.put(nativeKeyAt(handle, i));
    }
    return result;
  }

  // ── serialization ───────────────────────────────────────────────────────

  /** Encodes this object as a compact JSON string, or null if it is nested too deeply. */
  @Override
  public String toString() {
    return nativeToString(handle, 0);
  }

  /**
   * Encodes this object as a human-readable JSON string, indenting nested structures by {@code
   * indentSpaces} per level.
   *
   * @throws JSONException if the document is nested too deeply
   */
  public String toString(int indentSpaces) throws JSONException {
    String s = nativeToString(handle, indentSpaces);
    if (s == null) {
      throw new JSONException("JSON nested too deeply");
    }
    return s;
  }

  /**
   * Encodes {@code number} as a JSON string: integral doubles print without a fraction.
   *
   * @throws JSONException if {@code number} is null, NaN or infinite
   */
  public static String numberToString(Number number) throws JSONException {
    if (number == null) {
      throw new JSONException("Number must be non-null");
    }
    if (number instanceof Double) {
      return doubleToString(checkDouble(((Double) number).doubleValue()));
    }
    if (number instanceof Float) {
      return doubleToString(checkDouble(((Float) number).floatValue()));
    }
    return number.toString();
  }

  private static String doubleToString(double d) {
    long asLong = (long) d;
    if (d == (double) asLong) {
      return Long.toString(asLong);
    }
    return Double.toString(d);
  }

  /** Encodes {@code data} as a quoted, escaped JSON string literal; null becomes {@code ""}. */
  public static String quote(String data) {
    return data == null ? "\"\"" : nativeQuote(data);
  }

  /**
   * Wraps the given object if necessary: {@code null} becomes {@link #NULL}; a {@link JSONObject},
   * {@link JSONArray}, {@link #NULL}, primitive wrapper or {@code String} is returned as is; a
   * {@code Collection} or array becomes a {@link JSONArray}; a {@code Map} becomes a {@link
   * JSONObject}; anything else becomes its {@code toString()}.
   */
  public static Object wrap(Object o) {
    if (o == null) {
      return NULL;
    }
    if (o instanceof JSONArray || o instanceof JSONObject || isNullSentinel(o)) {
      return o;
    }
    if (o instanceof Collection) {
      return new JSONArray((Collection) o);
    }
    if (o instanceof Map) {
      return new JSONObject((Map) o);
    }
    if (o instanceof Boolean
        || o instanceof Integer
        || o instanceof Long
        || o instanceof Double
        || o instanceof Float
        || o instanceof String
        || o instanceof Character
        || o instanceof Short
        || o instanceof Byte) {
      return o;
    }
    if (JSONArray.isArray(o)) {
      try {
        return new JSONArray(o);
      } catch (JSONException e) {
        return null;
      }
    }
    return o.toString();
  }

  /** Live nodes in the native pool, for the diagnostics in {@code examples/jsondemo}. */
  public static int debugPoolNodes() {
    return nativePoolNodes();
  }

  // ── boxing and coercion (Android's internal JSON helper class) ──────────

  /** Materializes {@code node} as the Java object Android would hand out. */
  static Object box(int node) {
    switch (nativeKind(node)) {
      case K_NULL:
        return NULL;
      case K_BOOL:
        return Boolean.valueOf(nativeBoolValue(node));
      case K_INT:
        return Integer.valueOf(nativeIntValue(node));
      case K_LONG:
        return Long.valueOf(nativeLongValue(node));
      case K_DOUBLE:
        return Double.valueOf(nativeDoubleValue(node));
      case K_STRING:
        return nativeStringValue(node);
      case K_OBJECT:
        return new JSONObject(node);
      case K_ARRAY:
        return new JSONArray(node);
      default:
        return null;
    }
  }

  static Boolean toBoolean(Object value) {
    if (value instanceof Boolean) {
      return (Boolean) value;
    }
    if (value instanceof String) {
      String s = (String) value;
      if ("true".equalsIgnoreCase(s)) {
        return Boolean.valueOf(true);
      }
      if ("false".equalsIgnoreCase(s)) {
        return Boolean.valueOf(false);
      }
    }
    return null;
  }

  static Double toDouble(Object value) {
    if (value instanceof Double) {
      return (Double) value;
    }
    if (value instanceof Integer) {
      return Double.valueOf(((Integer) value).intValue());
    }
    if (value instanceof Long) {
      return Double.valueOf((double) ((Long) value).longValue());
    }
    if (value instanceof Float) {
      return Double.valueOf(((Float) value).floatValue());
    }
    if (value instanceof String) {
      try {
        return Double.valueOf((String) value);
      } catch (NumberFormatException e) {
        return null;
      }
    }
    return null;
  }

  static Integer toInteger(Object value) {
    if (value instanceof Integer) {
      return (Integer) value;
    }
    if (value instanceof Long) {
      return Integer.valueOf((int) ((Long) value).longValue());
    }
    if (value instanceof Double) {
      return Integer.valueOf((int) ((Double) value).doubleValue());
    }
    if (value instanceof Float) {
      return Integer.valueOf((int) ((Float) value).floatValue());
    }
    if (value instanceof String) {
      try {
        return Integer.valueOf((int) Double.parseDouble((String) value));
      } catch (NumberFormatException e) {
        return null;
      }
    }
    return null;
  }

  static Long toLong(Object value) {
    if (value instanceof Long) {
      return (Long) value;
    }
    if (value instanceof Integer) {
      return Long.valueOf(((Integer) value).intValue());
    }
    if (value instanceof Double) {
      return Long.valueOf((long) ((Double) value).doubleValue());
    }
    if (value instanceof Float) {
      return Long.valueOf((long) ((Float) value).floatValue());
    }
    if (value instanceof String) {
      try {
        return Long.valueOf((long) Double.parseDouble((String) value));
      } catch (NumberFormatException e) {
        return null;
      }
    }
    return null;
  }

  static String toString(Object value) {
    if (value instanceof String) {
      return (String) value;
    }
    if (value != null) {
      return String.valueOf(value);
    }
    return null;
  }

  static JSONException typeMismatch(Object indexOrName, Object actual, String requiredType) {
    if (actual == null) {
      return new JSONException("Value at " + indexOrName + " is null.");
    }
    return new JSONException(
        "Value "
            + actual
            + " at "
            + indexOrName
            + " of type "
            + actual.getClass().getName()
            + " cannot be converted to "
            + requiredType);
  }

  // ── natives (picodroid-core/src/native_handler/json.rs) ─────────────────
  // Every overload has its own name so dispatch never needs a descriptor. Node
  // indices are plain ints; a wrapper passes itself so the pool can bind it in
  // the same call, before any Java allocation can run a collection.

  static native int nativeNewObject(Object self);

  static native int nativeNewArray(Object self);

  static native void nativeBind(Object self, int node);

  static native int nativeParse(Object self, String text, boolean wantArray);

  static native String nativeLastError();

  static native int nativeKind(int node);

  static native int nativeLength(int node);

  static native int nativeChild(int node, String name);

  static native int nativeChildAt(int node, int index);

  static native String nativeKeyAt(int node, int index);

  static native boolean nativeBoolValue(int node);

  static native int nativeIntValue(int node);

  static native long nativeLongValue(int node);

  static native double nativeDoubleValue(int node);

  static native String nativeStringValue(int node);

  static native int nativePutNull(int node, String name);

  static native int nativePutBool(int node, String name, boolean value);

  static native int nativePutInt(int node, String name, int value);

  static native int nativePutLong(int node, String name, long value);

  static native int nativePutDouble(int node, String name, double value);

  static native int nativePutString(int node, String name, String value);

  static native int nativePutNode(int node, String name, int child);

  static native int nativeSetNull(int node, int index);

  static native int nativeSetBool(int node, int index, boolean value);

  static native int nativeSetInt(int node, int index, int value);

  static native int nativeSetLong(int node, int index, long value);

  static native int nativeSetDouble(int node, int index, double value);

  static native int nativeSetString(int node, int index, String value);

  static native int nativeSetNode(int node, int index, int child);

  static native void nativeRemove(int node, String name);

  static native void nativeRemoveAt(int node, int index);

  static native String nativeToString(int node, int indent);

  static native String nativeQuote(String data);

  static native int nativePoolNodes();
}
