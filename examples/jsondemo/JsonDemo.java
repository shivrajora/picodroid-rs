// SPDX-License-Identifier: GPL-3.0-only
package jsondemo;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import picodroid.app.Application;
import picodroid.json.JSONArray;
import picodroid.json.JSONException;
import picodroid.json.JSONObject;
import picodroid.util.Log;

/**
 * End-to-end checks of {@code picodroid.json} against Android's {@code org.json} behaviour: parsing
 * (the open-meteo reply picoenvmon consumes, nesting, escapes, number typing), {@code opt}/{@code
 * get} coercion, {@code put}/{@code remove}/{@code accumulate}/{@code append}, the {@link
 * JSONObject#NULL} sentinel, identity semantics through child wrappers, arrays, {@code
 * toString}/{@code quote}/{@code numberToString}/{@code wrap}, the error cases, and a GC-stress
 * loop proving the native node pool is reclaimed. Logs one PASS/FAIL line per check and {@code ===
 * PASSED ===} when all hold. Needs a board with {@code has_json = true}.
 */
public class JsonDemo extends Application {
  private static final String TAG = "JsonDemo";
  private static int fails = 0;

  /** The open-meteo reply picoenvmon's WeatherFetcher consumes, verbatim (note the degree sign). */
  static final String OPEN_METEO =
      "{\"latitude\":37.56252,\"longitude\":-122.307274,"
          + "\"generationtime_ms\":0.1697540283203125,\"utc_offset_seconds\":0,"
          + "\"timezone\":\"GMT\",\"timezone_abbreviation\":\"GMT\",\"elevation\":15.0,"
          + "\"current_units\":{\"time\":\"iso8601\",\"interval\":\"seconds\","
          + "\"temperature_2m\":\"°C\",\"weather_code\":\"wmo code\"},"
          + "\"current\":{\"time\":\"2026-09-04T06:00\",\"interval\":900,"
          + "\"temperature_2m\":17.3,\"weather_code\":3}}";

  private static void check(String what, boolean ok) {
    Log.i(TAG, (ok ? "PASS: " : "FAIL: ") + what);
    if (!ok) {
      fails = fails + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== JsonDemo start ===");
    try {
      runChecks();
    } catch (Throwable e) {
      check("no exception escaped the checks: " + e, false);
    }
    if (fails == 0) {
      Log.i(TAG, "=== PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED (" + fails + ") ===");
    }
  }

  private void runChecks() throws Exception {
    parseChecks();
    numberChecks();
    optChecks();
    putChecks();
    identityChecks();
    arrayChecks();
    serializationChecks();
    wrapChecks();
    errorChecks();
    gcStressChecks();
  }

  private static String firstKey(JSONObject o) {
    return o.keys().next();
  }

  private static void parseChecks() throws JSONException {
    JSONObject o = new JSONObject(OPEN_METEO);
    check("latitude parses as a double", o.getDouble("latitude") == 37.56252);
    JSONObject current = o.getJSONObject("current");
    check("current.weather_code == 3", current.getInt("weather_code") == 3);
    check("current.temperature_2m == 17.3", current.getDouble("temperature_2m") == 17.3);
    check("current.time is a string", "2026-09-04T06:00".equals(current.getString("time")));
    check(
        "units keep the degree sign",
        "°C".equals(o.getJSONObject("current_units").getString("temperature_2m")));
    check("length counts the top-level names", o.length() == 9);
    check("keys() is in insertion order", "latitude".equals(firstKey(o)));
    JSONArray nested = new JSONArray("[1,[2,[3]]]");
    check("nested arrays", nested.getJSONArray(1).getJSONArray(1).getInt(0) == 3);
    JSONObject esc =
        new JSONObject(
            "{\"s\":\"\\u0041\\n\\t\\\"\\\\\\/\",\"e\":\"\\u00e9\",\"lone\":\"\\ud83d\"}");
    check("escapes decode", "A\n\t\"\\/".equals(esc.getString("s")));
    check("\\u00e9 decodes to UTF-8", "é".equals(esc.getString("e")));
    check("a lone surrogate becomes U+FFFD", "\uFFFD".equals(esc.getString("lone")));
    check("whitespace is skipped", new JSONObject(" { \"a\" : [ 1 , 2 ] } ").length() == 1);
  }

  private static void numberChecks() throws JSONException {
    JSONArray n =
        new JSONArray(
            "[2147483647, 2147483648, -2147483649, 1.0, 1e3, 9223372036854775808,"
                + " \"12\", \"abc\", 17.3]");
    check("int max stays an Integer", n.get(0) instanceof Integer && n.getInt(0) == 2147483647);
    check("int max + 1 becomes a Long", n.get(1) instanceof Long && n.getLong(1) == 2147483648L);
    check("int min - 1 becomes a Long", n.get(2) instanceof Long);
    check("1.0 is a Double", n.get(3) instanceof Double);
    check("1e3 is 1000.0", n.getDouble(4) == 1000.0);
    check("beyond long becomes a Double", n.get(5) instanceof Double);
    check("getLong coerces an Integer", n.getLong(0) == 2147483647L);
    check("getDouble coerces an Integer", n.getDouble(0) == 2147483647.0);
    check("getInt coerces a numeric string", n.getInt(6) == 12);
    check("getInt truncates 17.3", n.getInt(8) == 17);
    boolean threw = false;
    try {
      n.getInt(7);
    } catch (JSONException e) {
      threw = true;
    }
    check("getInt on \"abc\" throws", threw);
    check("optInt on \"abc\" falls back", n.optInt(7, 5) == 5);
    JSONObject b = new JSONObject("{\"b\":\"true\",\"c\":false}");
    check("getBoolean coerces \"true\"", b.getBoolean("b") && !b.getBoolean("c"));
    check("getString of a boolean", "false".equals(b.getString("c")));
  }

  private static void optChecks() {
    JSONObject o = new JSONObject();
    check("optInt default is 0", o.optInt("nope") == 0);
    check("optInt fallback", o.optInt("nope", 7) == 7);
    check("optLong fallback", o.optLong("nope", 9L) == 9L);
    check("optString default is empty", o.optString("nope").isEmpty());
    check("optString fallback", "x".equals(o.optString("nope", "x")));
    double d = o.optDouble("nope");
    check("optDouble default is NaN", !(d <= 0.0 || d >= 0.0));
    check("optDouble fallback", o.optDouble("nope", 2.5) == 2.5);
    check("optBoolean fallback", o.optBoolean("nope", true) && !o.optBoolean("nope"));
    check("optJSONObject is null", o.optJSONObject("nope") == null);
    check("optJSONArray is null", o.optJSONArray("nope") == null);
    check("opt is null", o.opt("nope") == null);
    check("has is false", !o.has("nope"));
  }

  private static void putChecks() throws JSONException {
    JSONObject o = new JSONObject();
    o.put("a", 1).put("b", 2L).put("c", 1.5).put("d", true).put("e", "s");
    check("put chain gives length 5", o.length() == 5);
    check("has a", o.has("a"));
    check("get boxes an Integer", o.get("a") instanceof Integer);
    check("get boxes a Long", o.get("b") instanceof Long);
    check("getString of a number", "1.5".equals(o.getString("c")));
    Object removed = o.remove("a");
    check(
        "remove returns the Integer",
        removed instanceof Integer && ((Integer) removed).intValue() == 1);
    check("removed name is gone", !o.has("a") && o.length() == 4);
    o.put("n", JSONObject.NULL);
    check("NULL is present", o.has("n"));
    check("isNull for NULL", o.isNull("n"));
    check("isNull for a missing name", o.isNull("zzz"));
    check("opt returns the NULL sentinel", o.opt("n") == JSONObject.NULL);
    Object nothing = null;
    check("NULL equals null", JSONObject.NULL.equals(nothing));
    check("NULL prints as null", "null".equals(String.valueOf(JSONObject.NULL)));
    o.put("e", (Object) null);
    check("put(name, null) removes", !o.has("e"));
    o.putOpt(null, "x");
    o.putOpt("x", null);
    check("putOpt with a null is a no-op", !o.has("x"));
    o.put("b", "replaced");
    check("put replaces in place", "replaced".equals(o.getString("b")) && "b".equals(firstKey(o)));
    o.accumulate("acc", 1);
    o.accumulate("acc", 2);
    JSONArray acc = o.getJSONArray("acc");
    check("accumulate builds an array", acc.length() == 2 && acc.getInt(1) == 2);
    o.append("app", "x");
    o.append("app", "y");
    check("append builds an array", o.getJSONArray("app").length() == 2);
    boolean threw = false;
    try {
      o.append("b", 1);
    } catch (JSONException e) {
      threw = true;
    }
    check("append onto a non-array throws", threw);
    threw = false;
    try {
      o.put("nan", Double.NaN);
    } catch (JSONException e) {
      threw = true;
    }
    check("NaN is rejected", threw);
    threw = false;
    try {
      o.put(null, 1);
    } catch (JSONException e) {
      threw = true;
    }
    check("a null name is rejected", threw);
    Iterator<String> it = o.keys();
    int count = 0;
    while (it.hasNext()) {
      it.next();
      count = count + 1;
    }
    check("keys() walks every name", count == o.length());
    check("names() has every name", o.names().length() == o.length());
    check("keySet contains", o.keySet().contains("acc"));
    check("names() of an empty object is null", new JSONObject().names() == null);
  }

  private static void identityChecks() throws JSONException {
    JSONObject parent = new JSONObject();
    JSONObject child = new JSONObject();
    parent.put("c", child);
    child.put("k", 1);
    check(
        "child mutation is visible through the parent", parent.getJSONObject("c").getInt("k") == 1);
    parent.getJSONObject("c").put("k2", 2);
    check("parent-side mutation is visible through the child", child.getInt("k2") == 2);
    parent.put("again", child);
    check(
        "the same child under two names",
        "{\"c\":{\"k\":1,\"k2\":2},\"again\":{\"k\":1,\"k2\":2}}".equals(parent.toString()));
    boolean threw = false;
    try {
      child.put("cycle", parent);
    } catch (IllegalArgumentException e) {
      threw = true;
    }
    check("a cycle is refused", threw);
    Object detached = parent.remove("c");
    check(
        "remove returns a live wrapper",
        detached instanceof JSONObject && ((JSONObject) detached).getInt("k") == 1);
    check("the other name still reaches the child", parent.getJSONObject("again").getInt("k") == 1);
  }

  private static void arrayChecks() throws JSONException {
    JSONArray a = new JSONArray();
    a.put(1).put("two").put(3.5).put(true).put((Object) null);
    check("array length 5", a.length() == 5);
    check("null is stored as NULL", a.isNull(4) && a.get(4) == JSONObject.NULL);
    a.put(7, "pad");
    check(
        "put past the end pads with null",
        a.length() == 8 && a.isNull(6) && "pad".equals(a.getString(7)));
    check(
        "join encodes values", "1,\"two\",3.5".equals(new JSONArray("[1,\"two\",3.5]").join(",")));
    ArrayList<Object> list = new ArrayList<Object>();
    list.add("x");
    list.add(Integer.valueOf(2));
    check("JSONArray(Collection)", "[\"x\",2]".equals(new JSONArray(list).toString()));
    check("JSONArray(int[])", "[1,2]".equals(new JSONArray(new int[] {1, 2}).toString()));
    check(
        "JSONArray(Object[]) wraps nulls",
        "[\"a\",null]".equals(new JSONArray(new Object[] {"a", null}).toString()));
    boolean threw = false;
    try {
      new JSONArray((Object) "not an array");
    } catch (JSONException e) {
      threw = true;
    }
    check("JSONArray(non-array) throws", threw);
    check("JSONArray.equals by content", new JSONArray("[1,2]").equals(new JSONArray("[1, 2]")));
    check(
        "JSONArray.hashCode by content",
        new JSONArray("[1,2]").hashCode() == new JSONArray("[1, 2]").hashCode());
    Object r = a.remove(0);
    check(
        "remove(0) shifts",
        r instanceof Integer && "two".equals(a.getString(0)) && a.length() == 7);
    check("remove out of range is null", a.remove(99) == null);
    JSONArray names = new JSONArray();
    names.put("p").put("q");
    JSONObject fromNames = new JSONArray("[1,2]").toJSONObject(names);
    check("toJSONObject", fromNames.getInt("q") == 2);
    check("toJSONArray", fromNames.toJSONArray(names).getInt(0) == 1);
    HashMap<String, Object> m = new HashMap<String, Object>();
    m.put("k", "v");
    check("JSONObject(Map)", "v".equals(new JSONObject(m).getString("k")));
    check(
        "JSONObject(JSONObject, String[])",
        new JSONObject(fromNames, new String[] {"p", "missing"}).length() == 1);
  }

  private static void serializationChecks() throws JSONException {
    JSONObject o = new JSONObject(OPEN_METEO);
    String s = o.toString();
    check("toString round-trips", new JSONObject(s).toString().equals(s));
    check("toString(2) indents", o.toString(2).startsWith("{\n  \"latitude\": 37.56252,\n"));
    check("quote escapes", "\"a\\\"b\\/c\"".equals(JSONObject.quote("a\"b/c")));
    check("quote(null)", "\"\"".equals(JSONObject.quote(null)));
    check(
        "numberToString of an integral double",
        "3".equals(JSONObject.numberToString(Double.valueOf(3.0))));
    check(
        "numberToString of a fraction",
        "3.5".equals(JSONObject.numberToString(Double.valueOf(3.5))));
    check(
        "numberToString of an Integer", "7".equals(JSONObject.numberToString(Integer.valueOf(7))));
    check(
        "an integral double serializes without a fraction",
        "{\"d\":15}".equals(new JSONObject("{\"d\":15.0}").toString()));
    check(
        "strings re-escape",
        "{\"s\":\"a\\nb\"}".equals(new JSONObject("{\"s\":\"a\\nb\"}").toString()));
    check(
        "empty containers",
        "{\"a\":{},\"b\":[]}".equals(new JSONObject("{\"a\":{},\"b\":[]}").toString()));
  }

  private static void wrapChecks() {
    check("wrap(null) is NULL", JSONObject.wrap(null) == JSONObject.NULL);
    check(
        "wrap(list) is a JSONArray", JSONObject.wrap(new ArrayList<Object>()) instanceof JSONArray);
    check(
        "wrap(map) is a JSONObject",
        JSONObject.wrap(new HashMap<String, Object>()) instanceof JSONObject);
    check("wrap(Integer) passes through", JSONObject.wrap(Integer.valueOf(1)) instanceof Integer);
    check("wrap(int[]) is a JSONArray", JSONObject.wrap(new int[] {1}) instanceof JSONArray);
    Object plain =
        new Object() {
          @Override
          public String toString() {
            return "plain";
          }
        };
    check("wrap(other) is its toString", "plain".equals(JSONObject.wrap(plain)));
  }

  private static void errorChecks() throws JSONException {
    String[] bad = {"{", "[1]", "{\"a\":1,}", "{\"a\" 1}", "{'a':1}", "{\"a\":tru}", "{} x"};
    for (String text : bad) {
      boolean threw = false;
      String msg = null;
      try {
        new JSONObject(text);
      } catch (JSONException e) {
        threw = true;
        msg = e.getMessage();
      }
      check(
          "bad JSON throws with a position: " + text,
          threw && msg != null && msg.contains("at character"));
    }
    boolean threw = false;
    try {
      new JSONArray("{}");
    } catch (JSONException e) {
      threw = true;
    }
    check("JSONArray of an object text throws", threw);
    JSONObject o = new JSONObject(OPEN_METEO);
    threw = false;
    try {
      o.getString("missing");
    } catch (JSONException e) {
      threw = "No value for missing".equals(e.getMessage());
    }
    check("getString of a missing name throws", threw);
    threw = false;
    try {
      o.getJSONObject("latitude");
    } catch (JSONException e) {
      threw = true;
    }
    check("getJSONObject of a double throws", threw);
    threw = false;
    try {
      new JSONArray("[1]").get(99);
    } catch (JSONException e) {
      threw = true;
    }
    check("get out of range throws", threw);
    check("opt of a negative index is null", new JSONArray("[1]").opt(-1) == null);
    threw = false;
    try {
      new JSONArray().put(-1, 1);
    } catch (JSONException e) {
      threw = true;
    }
    check("put at a negative index throws", threw);
  }

  private static void gcStressChecks() throws JSONException {
    int before = JSONObject.debugPoolNodes();
    for (int i = 0; i < 300; i++) {
      JSONObject o = new JSONObject(OPEN_METEO);
      if (o.getJSONObject("current").getInt("weather_code") != 3) {
        check("parse in iteration " + i, false);
        return;
      }
    }
    int after = JSONObject.debugPoolNodes();
    Log.i(TAG, "pool nodes before=" + before + " after=" + after);
    check("the pool is reclaimed across 300 parses (after=" + after + ")", after < before + 600);
  }
}
