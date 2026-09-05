// SPDX-License-Identifier: GPL-3.0-only
package picodroid.json;

import java.util.Collection;

/**
 * A dense indexed sequence of values, mirroring {@code org.json.JSONArray}. Values are {@link
 * JSONObject}, {@link JSONArray}, {@code String}, {@code Boolean}, {@code Integer}, {@code Long},
 * {@code Double} or {@link JSONObject#NULL}; anything else is stored as its {@code toString()}.
 *
 * <p>Storage and lifetime are as described on {@link JSONObject}: this wrapper holds only a native
 * node index, {@link #getJSONArray}/{@link #getJSONObject} hand out fresh wrappers over shared
 * nodes, and the array constructor takes the array types it can recognise without reflection
 * ({@code Object[]} and the {@code int}/{@code long}/{@code double}/{@code float}/{@code boolean}
 * primitive arrays).
 */
public class JSONArray {
  /** Node index in the native pool; bound to this wrapper for as long as it is reachable. */
  final int handle;

  /** Creates an empty array. */
  public JSONArray() {
    int h = JSONObject.nativeNewArray(this);
    if (h < 0) {
      throw new OutOfMemoryError("JSON pool exhausted");
    }
    handle = h;
  }

  /**
   * Creates an array with the elements of {@code copyFrom}, each passed through {@link
   * JSONObject#wrap}.
   */
  public JSONArray(Collection copyFrom) {
    this();
    if (copyFrom != null) {
      Collection<?> items = copyFrom;
      for (Object o : items) {
        putRawAt(-1, JSONObject.wrap(o));
      }
    }
  }

  /**
   * Parses {@code json}, which must be a JSON array.
   *
   * @throws JSONException if the text is not a valid JSON array
   */
  public JSONArray(String json) throws JSONException {
    if (json == null) {
      throw new NullPointerException("json == null");
    }
    int h = JSONObject.nativeParse(this, json, true);
    if (h < 0) {
      throw new JSONException(JSONObject.nativeLastError());
    }
    handle = h;
  }

  /**
   * Creates an array with the elements of the given Java array.
   *
   * @throws JSONException if {@code array} is not an {@code Object[]} or a supported primitive
   *     array
   */
  public JSONArray(Object array) throws JSONException {
    this();
    if (array instanceof Object[]) {
      for (Object o : (Object[]) array) {
        putRawAt(-1, JSONObject.wrap(o));
      }
    } else if (array instanceof int[]) {
      for (int v : (int[]) array) {
        putRawAt(-1, Integer.valueOf(v));
      }
    } else if (array instanceof long[]) {
      for (long v : (long[]) array) {
        putRawAt(-1, Long.valueOf(v));
      }
    } else if (array instanceof double[]) {
      for (double v : (double[]) array) {
        putRawAt(-1, Double.valueOf(v));
      }
    } else if (array instanceof float[]) {
      for (float v : (float[]) array) {
        putRawAt(-1, Float.valueOf(v));
      }
    } else if (array instanceof boolean[]) {
      for (boolean v : (boolean[]) array) {
        putRawAt(-1, Boolean.valueOf(v));
      }
    } else {
      throw new JSONException("Not a primitive array: " + array.getClass());
    }
  }

  /** Wraps an existing node (a child reached through its parent). */
  JSONArray(int node) {
    handle = node;
    JSONObject.nativeBind(this, node);
  }

  /** Whether {@link #JSONArray(Object)} accepts {@code o}. */
  static boolean isArray(Object o) {
    return o instanceof Object[]
        || o instanceof int[]
        || o instanceof long[]
        || o instanceof double[]
        || o instanceof float[]
        || o instanceof boolean[];
  }

  /** Returns the number of values in this array. */
  public int length() {
    return JSONObject.nativeLength(handle);
  }

  // ── put ─────────────────────────────────────────────────────────────────

  /** Appends {@code value} to the end of this array. */
  public JSONArray put(boolean value) {
    JSONObject.status(JSONObject.nativeSetBool(handle, -1, value));
    return this;
  }

  /**
   * Appends {@code value} to the end of this array.
   *
   * @throws JSONException if {@code value} is NaN or infinite
   */
  public JSONArray put(double value) throws JSONException {
    JSONObject.status(JSONObject.nativeSetDouble(handle, -1, JSONObject.checkDouble(value)));
    return this;
  }

  /** Appends {@code value} to the end of this array. */
  public JSONArray put(int value) {
    JSONObject.status(JSONObject.nativeSetInt(handle, -1, value));
    return this;
  }

  /** Appends {@code value} to the end of this array. */
  public JSONArray put(long value) {
    JSONObject.status(JSONObject.nativeSetLong(handle, -1, value));
    return this;
  }

  /**
   * Appends {@code value} to the end of this array; {@code null} is stored as {@link
   * JSONObject#NULL}.
   */
  public JSONArray put(Object value) {
    putRawAt(-1, value);
    return this;
  }

  /**
   * Sets the value at {@code index}, padding the array with {@link JSONObject#NULL} if it is
   * shorter than that.
   */
  public JSONArray put(int index, boolean value) throws JSONException {
    JSONObject.status(JSONObject.nativeSetBool(handle, checkIndex(index), value));
    return this;
  }

  /**
   * Sets the value at {@code index}, padding the array with {@link JSONObject#NULL} if it is
   * shorter than that.
   *
   * @throws JSONException if {@code value} is NaN or infinite
   */
  public JSONArray put(int index, double value) throws JSONException {
    JSONObject.status(
        JSONObject.nativeSetDouble(handle, checkIndex(index), JSONObject.checkDouble(value)));
    return this;
  }

  /**
   * Sets the value at {@code index}, padding the array with {@link JSONObject#NULL} if it is
   * shorter than that.
   */
  public JSONArray put(int index, int value) throws JSONException {
    JSONObject.status(JSONObject.nativeSetInt(handle, checkIndex(index), value));
    return this;
  }

  /**
   * Sets the value at {@code index}, padding the array with {@link JSONObject#NULL} if it is
   * shorter than that.
   */
  public JSONArray put(int index, long value) throws JSONException {
    JSONObject.status(JSONObject.nativeSetLong(handle, checkIndex(index), value));
    return this;
  }

  /**
   * Sets the value at {@code index}, padding the array with {@link JSONObject#NULL} if it is
   * shorter than that.
   *
   * @throws JSONException if {@code value} is a NaN or infinite double
   */
  public JSONArray put(int index, Object value) throws JSONException {
    checkIndex(index);
    checkedPut(index, value);
    return this;
  }

  /** Android's {@code checkedPut}: appends after the double check. */
  void checkedPut(Object value) throws JSONException {
    checkedPut(-1, value);
  }

  private void checkedPut(int index, Object value) throws JSONException {
    if (value instanceof Double) {
      JSONObject.checkDouble(((Double) value).doubleValue());
    } else if (value instanceof Float) {
      JSONObject.checkDouble(((Float) value).floatValue());
    }
    putRawAt(index, value);
  }

  /** Stores {@code value} at {@code index} ({@code -1} appends) by its type. */
  private void putRawAt(int index, Object value) {
    int r;
    if (JSONObject.isNullSentinel(value)) {
      r = JSONObject.nativeSetNull(handle, index);
    } else if (value instanceof JSONObject) {
      r = JSONObject.nativeSetNode(handle, index, ((JSONObject) value).handle);
    } else if (value instanceof JSONArray) {
      r = JSONObject.nativeSetNode(handle, index, ((JSONArray) value).handle);
    } else if (value instanceof Boolean) {
      r = JSONObject.nativeSetBool(handle, index, ((Boolean) value).booleanValue());
    } else if (value instanceof Integer) {
      r = JSONObject.nativeSetInt(handle, index, ((Integer) value).intValue());
    } else if (value instanceof Long) {
      r = JSONObject.nativeSetLong(handle, index, ((Long) value).longValue());
    } else if (value instanceof Double) {
      r = JSONObject.nativeSetDouble(handle, index, ((Double) value).doubleValue());
    } else if (value instanceof Float) {
      r = JSONObject.nativeSetDouble(handle, index, ((Float) value).floatValue());
    } else if (value instanceof Number) {
      r = JSONObject.nativeSetDouble(handle, index, Double.parseDouble(value.toString()));
    } else if (value instanceof String) {
      r = JSONObject.nativeSetString(handle, index, (String) value);
    } else {
      r = JSONObject.nativeSetString(handle, index, String.valueOf(value));
    }
    JSONObject.status(r);
  }

  private static int checkIndex(int index) throws JSONException {
    if (index < 0) {
      throw new JSONException("Index " + index + " must be non-negative");
    }
    return index;
  }

  // ── get / opt ───────────────────────────────────────────────────────────

  /**
   * Returns true if this array has no value at {@code index}, or if its value is {@link
   * JSONObject#NULL}.
   */
  public boolean isNull(int index) {
    if (index < 0) {
      return true;
    }
    int node = JSONObject.nativeChildAt(handle, index);
    return node < 0 || JSONObject.nativeKind(node) == JSONObject.K_NULL;
  }

  /**
   * Returns the value at {@code index}.
   *
   * @throws JSONException if this array has no value at {@code index}
   */
  public Object get(int index) throws JSONException {
    Object value = opt(index);
    if (value == null) {
      throw new JSONException("Index " + index + " out of range [0.." + length() + ")");
    }
    return value;
  }

  /** Returns the value at {@code index}, or null if the array has no value at {@code index}. */
  public Object opt(int index) {
    if (index < 0) {
      return null;
    }
    int node = JSONObject.nativeChildAt(handle, index);
    return node < 0 ? null : JSONObject.box(node);
  }

  /**
   * Returns the value at {@code index} if it is a boolean or can be coerced to one.
   *
   * @throws JSONException if the value is missing or cannot be coerced
   */
  public boolean getBoolean(int index) throws JSONException {
    Object object = get(index);
    Boolean result = JSONObject.toBoolean(object);
    if (result == null) {
      throw JSONObject.typeMismatch(Integer.valueOf(index), object, "boolean");
    }
    return result.booleanValue();
  }

  /** Returns the value at {@code index} coerced to a boolean, or false otherwise. */
  public boolean optBoolean(int index) {
    return optBoolean(index, false);
  }

  /** Returns the value at {@code index} coerced to a boolean, or {@code fallback}. */
  public boolean optBoolean(int index, boolean fallback) {
    Boolean result = JSONObject.toBoolean(opt(index));
    return result != null ? result.booleanValue() : fallback;
  }

  /**
   * Returns the value at {@code index} if it is a double or can be coerced to one.
   *
   * @throws JSONException if the value is missing or cannot be coerced
   */
  public double getDouble(int index) throws JSONException {
    Object object = get(index);
    Double result = JSONObject.toDouble(object);
    if (result == null) {
      throw JSONObject.typeMismatch(Integer.valueOf(index), object, "double");
    }
    return result.doubleValue();
  }

  /** Returns the value at {@code index} coerced to a double, or {@code NaN} otherwise. */
  public double optDouble(int index) {
    return optDouble(index, Double.NaN);
  }

  /** Returns the value at {@code index} coerced to a double, or {@code fallback}. */
  public double optDouble(int index, double fallback) {
    Double result = JSONObject.toDouble(opt(index));
    return result != null ? result.doubleValue() : fallback;
  }

  /**
   * Returns the value at {@code index} if it is an int or can be coerced to one.
   *
   * @throws JSONException if the value is missing or cannot be coerced
   */
  public int getInt(int index) throws JSONException {
    Object object = get(index);
    Integer result = JSONObject.toInteger(object);
    if (result == null) {
      throw JSONObject.typeMismatch(Integer.valueOf(index), object, "int");
    }
    return result.intValue();
  }

  /** Returns the value at {@code index} coerced to an int, or 0 otherwise. */
  public int optInt(int index) {
    return optInt(index, 0);
  }

  /** Returns the value at {@code index} coerced to an int, or {@code fallback}. */
  public int optInt(int index, int fallback) {
    Integer result = JSONObject.toInteger(opt(index));
    return result != null ? result.intValue() : fallback;
  }

  /**
   * Returns the value at {@code index} if it is a long or can be coerced to one.
   *
   * @throws JSONException if the value is missing or cannot be coerced
   */
  public long getLong(int index) throws JSONException {
    Object object = get(index);
    Long result = JSONObject.toLong(object);
    if (result == null) {
      throw JSONObject.typeMismatch(Integer.valueOf(index), object, "long");
    }
    return result.longValue();
  }

  /** Returns the value at {@code index} coerced to a long, or 0 otherwise. */
  public long optLong(int index) {
    return optLong(index, 0L);
  }

  /** Returns the value at {@code index} coerced to a long, or {@code fallback}. */
  public long optLong(int index, long fallback) {
    Long result = JSONObject.toLong(opt(index));
    return result != null ? result.longValue() : fallback;
  }

  /**
   * Returns the value at {@code index} if it exists, coercing it to a string if necessary.
   *
   * @throws JSONException if no such value exists
   */
  public String getString(int index) throws JSONException {
    Object object = get(index);
    String result = JSONObject.toString(object);
    if (result == null) {
      throw JSONObject.typeMismatch(Integer.valueOf(index), object, "String");
    }
    return result;
  }

  /** Returns the value at {@code index} coerced to a string, or the empty string. */
  public String optString(int index) {
    return optString(index, "");
  }

  /** Returns the value at {@code index} coerced to a string, or {@code fallback}. */
  public String optString(int index, String fallback) {
    String result = JSONObject.toString(opt(index));
    return result != null ? result : fallback;
  }

  /**
   * Returns the value at {@code index} if it is a {@link JSONArray}.
   *
   * @throws JSONException if the value is missing or is not an array
   */
  public JSONArray getJSONArray(int index) throws JSONException {
    Object object = get(index);
    if (object instanceof JSONArray) {
      return (JSONArray) object;
    }
    throw JSONObject.typeMismatch(Integer.valueOf(index), object, "JSONArray");
  }

  /** Returns the value at {@code index} if it is a {@link JSONArray}, or null. */
  public JSONArray optJSONArray(int index) {
    Object object = opt(index);
    return object instanceof JSONArray ? (JSONArray) object : null;
  }

  /**
   * Returns the value at {@code index} if it is a {@link JSONObject}.
   *
   * @throws JSONException if the value is missing or is not an object
   */
  public JSONObject getJSONObject(int index) throws JSONException {
    Object object = get(index);
    if (object instanceof JSONObject) {
      return (JSONObject) object;
    }
    throw JSONObject.typeMismatch(Integer.valueOf(index), object, "JSONObject");
  }

  /** Returns the value at {@code index} if it is a {@link JSONObject}, or null. */
  public JSONObject optJSONObject(int index) {
    Object object = opt(index);
    return object instanceof JSONObject ? (JSONObject) object : null;
  }

  // ── structure ───────────────────────────────────────────────────────────

  /**
   * Removes and returns the value at {@code index}, or null if the array has no value there. The
   * value is boxed before the slot is unlinked, for the reason given on {@link JSONObject#remove}.
   */
  public Object remove(int index) {
    Object value = opt(index);
    if (value != null) {
      JSONObject.nativeRemoveAt(handle, index);
    }
    return value;
  }

  /**
   * Returns a new object whose values are the values in this array, and whose names are the values
   * in {@code names}, or null if either array is empty.
   */
  public JSONObject toJSONObject(JSONArray names) throws JSONException {
    JSONObject result = new JSONObject();
    int length = Math.min(names == null ? 0 : names.length(), length());
    if (length == 0) {
      return null;
    }
    for (int i = 0; i < length; i++) {
      String name = JSONObject.toString(names.opt(i));
      result.put(name, opt(i));
    }
    return result;
  }

  /**
   * Returns a new string by alternating this array's values with {@code separator}. Each value is
   * encoded as JSON, so strings are quoted.
   */
  public String join(String separator) throws JSONException {
    StringBuilder sb = new StringBuilder();
    int n = length();
    for (int i = 0; i < n; i++) {
      if (i > 0) {
        sb.append(separator);
      }
      String value = JSONObject.nativeToString(JSONObject.nativeChildAt(handle, i), 0);
      if (value == null) {
        throw new JSONException("JSON nested too deeply");
      }
      sb.append(value);
    }
    return sb.toString();
  }

  // ── serialization ───────────────────────────────────────────────────────

  /** Encodes this array as a compact JSON string, or null if it is nested too deeply. */
  @Override
  public String toString() {
    return JSONObject.nativeToString(handle, 0);
  }

  /**
   * Encodes this array as a human-readable JSON string, indenting nested structures by {@code
   * indentSpaces} per level.
   *
   * @throws JSONException if the document is nested too deeply
   */
  public String toString(int indentSpaces) throws JSONException {
    String s = JSONObject.nativeToString(handle, indentSpaces);
    if (s == null) {
      throw new JSONException("JSON nested too deeply");
    }
    return s;
  }

  /** Two arrays are equal when they encode to the same JSON text, as on Android. */
  @Override
  public boolean equals(Object o) {
    if (!(o instanceof JSONArray)) {
      return false;
    }
    String mine = toString();
    return mine != null && mine.equals(((JSONArray) o).toString());
  }

  @Override
  public int hashCode() {
    String s = toString();
    return s == null ? 0 : s.hashCode();
  }
}
