---
title: "JSON"
description: "org.json-compatible JSONObject, JSONArray and JSONException, backed by a native node pool."
---

`picodroid.json.*` — `JSONObject`, `JSONArray` and `JSONException` with the method names, signatures and semantics of Android's `org.json` (Android ships the package as `org.json`; the picodroid namespace rule makes it `picodroid.json`). See [Java API overview](/api/) for the full API index.

JSON is a board capability, like networking: a board opts in with `has_json = true` in its [`board.toml`](/reference/porting-guide/#boardtoml-reference). Every RP2350 board ships it. `testbench_rp2040` leaves it off, which drops the three classes from that board's embedded SDK and compiles the native parser out, so the board pays nothing for it — and an app that references `picodroid.json` fails that board's `verifyApiContract` at build time (`EXCLUDED ON BOARD testbench_rp2040`) rather than on the device.

## Quick example

The `picoenvmon` example fetches the current weather from open-meteo and reads two fields:

```java
import picodroid.json.JSONException;
import picodroid.json.JSONObject;

String body = "{\"current\":{\"temperature_2m\":17.3,\"weather_code\":3}}";
try {
  JSONObject current = new JSONObject(body).getJSONObject("current");
  double celsius = current.getDouble("temperature_2m");   // 17.3
  int code = current.getInt("weather_code");              // 3
  String missing = current.optString("wind", "n/a");      // "n/a"
} catch (JSONException e) {
  // not JSON, not an object, missing name, or a value that cannot be coerced
}

JSONObject reading = new JSONObject();
reading.put("temp", 21.5).put("ok", true).put("tags", new JSONArray().put("a").put("b"));
String text = reading.toString();   // {"temp":21.5,"ok":true,"tags":["a","b"]}
```

## How it works

A document lives in a native node pool, not on the Java heap. A `JSONObject` or `JSONArray` holds only the index of its node; values are materialized into Java objects on `get`/`opt` (`Integer`, `Long`, `Double`, `Boolean`, `String`, or a fresh `JSONObject`/`JSONArray` wrapper over the child node). Putting a `JSONObject`/`JSONArray` links its node into the parent, so later mutation through either wrapper is visible in the other — Android's identity semantics — but two wrappers obtained by `getJSONObject` are not `==`. Any other kind of object put into a document is stored as its `toString()`.

Nodes are reclaimed by the garbage collector: after every collection the runtime drops the bindings of wrappers that died and frees every node no surviving wrapper reaches. Nothing is freed while a wrapper can still see it, including the value `remove` returns.

The pool is capped at 2048 nodes and 16 KiB of string and key bytes across all live documents. A parse that would exceed the cap throws `JSONException` (`JSON pool exhausted`); a `put` throws `OutOfMemoryError`. Nesting is capped at 32 containers on any path, in the parser and the serializer alike.

## `JSONObject`

| Member | Notes |
|---|---|
| `JSONObject()`, `JSONObject(String json)`, `JSONObject(Map copyFrom)`, `JSONObject(JSONObject copyFrom, String[] names)` | `(String)` parses an object text and throws `JSONException` otherwise; `(Map)` requires `String` keys and wraps values with `wrap`. |
| `NULL` | The explicit-null sentinel: `equals(null)` and `equals(NULL)` are true, `toString()` is `"null"`. Compare with `equals` or `isNull`, not `==`. |
| `length()`, `has(name)`, `isNull(name)`, `remove(name)` | `isNull` is true for a missing name too. `remove` returns the old value (boxed before the mapping is unlinked). |
| `keys()`, `keySet()`, `names()`, `toJSONArray(JSONArray names)` | `keys()` and `names()` are in insertion order; `keySet()` is an unordered copy. |
| `get(name)`, `getBoolean`, `getDouble`, `getInt`, `getLong`, `getString`, `getJSONArray`, `getJSONObject` | Android's coercions: `"true"`/`"false"` to boolean, numeric strings to numbers, `getInt` truncates a double, `getString` stringifies anything. A missing name or a failed coercion throws `JSONException`. |
| `opt(name)`, `optBoolean(name[, fallback])`, `optDouble` (default `NaN`), `optInt`, `optLong`, `optString` (default `""`), `optJSONArray`, `optJSONObject` | Same coercions, fallback instead of an exception. |
| `put(name, boolean/double/int/long/Object)`, `putOpt`, `accumulate`, `append` | `put(name, null)` removes; `NULL` stores an explicit null. NaN and infinities throw `JSONException`. `accumulate` builds an array on the second value; `append` requires an array (or nothing) under the name. |
| `toString()`, `toString(int indentSpaces)` | Compact, or one entry per line with `"key": value`. `toString()` returns null past the nesting cap; `toString(int)` throws. |
| `static quote(String)`, `numberToString(Number)`, `wrap(Object)` | `quote` escapes as Android does (`"`, `\`, `/`, control characters). `wrap` turns `null` into `NULL`, a `Collection` or array into a `JSONArray`, a `Map` into a `JSONObject`, anything unknown into its `toString()`. |
| `static debugPoolNodes()` | picodroid-only: live nodes in the pool, for diagnostics. |

## `JSONArray`

| Member | Notes |
|---|---|
| `JSONArray()`, `JSONArray(String json)`, `JSONArray(Collection copyFrom)`, `JSONArray(Object array)` | `(Object)` accepts `Object[]`, `int[]`, `long[]`, `double[]`, `float[]` and `boolean[]` (there is no reflection) and throws `JSONException` for anything else. |
| `length()`, `isNull(index)`, `remove(index)` | `remove` returns the old value or null; later items shift down. |
| `put(boolean/double/int/long/Object)`, `put(int index, …)` | Appending, or setting at an index — an index past the end pads with `NULL`; a negative index throws `JSONException`. |
| `get(index)`, `getBoolean` … `getJSONObject(index)`, `opt(index)`, `optBoolean` … `optJSONObject(index[, fallback])` | As on `JSONObject`; an index out of range throws (`get`) or falls back (`opt`). |
| `join(separator)`, `toJSONObject(JSONArray names)` | `join` encodes each value as JSON, so strings come out quoted. |
| `toString()`, `toString(int)`, `equals`, `hashCode` | Equality is by encoded content, as on Android. |

## `JSONException`

A checked exception, as on Android, with the `(String)`, `(String, Throwable)` and `(Throwable)` constructors. A parse failure's message names the offending position: `Expected ':' after key at character 12`.

## Divergences from Android

- **Strict parser.** RFC 8259 only: no single quotes, comments, unquoted keys or hex literals (Android's `JSONTokener` is lenient). Duplicate keys keep the first position and the last value.
- **Fresh wrappers.** `getJSONObject`/`getJSONArray` return a new wrapper each call; mutations are shared, identity (`==`) is not.
- **`keySet()`** is an unordered copy; use `keys()` or `names()` for insertion order.
- **Doubles** serialize in Rust's shortest form: integral values without a fraction (as Android's `numberToString`), but a very large or very small magnitude prints as `1e21`, not `1.0E21`. `\uXXXX` escapes decode to UTF-8; a lone surrogate becomes U+FFFD.
- **No `JSONTokener` or `JSONStringer`**, and no constructors taking one.
- **Cycles are refused.** Putting a container into its own descendant throws `IllegalArgumentException` (Android would overflow the stack in `toString`).
- **Capacity.** 2048 nodes and 16 KiB of string bytes across all live documents, 32 levels of nesting.
