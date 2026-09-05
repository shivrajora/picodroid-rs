// SPDX-License-Identifier: GPL-3.0-only
package picodroid.json;

/**
 * Thrown to indicate a problem with the JSON API, mirroring {@code org.json.JSONException}: a
 * document that does not parse, a lookup of a missing name or index, a value that cannot be coerced
 * to the requested type, or a document too deeply nested to serialize. Checked, as on Android.
 */
public class JSONException extends Exception {
  public JSONException(String s) {
    super(s);
  }

  public JSONException(String message, Throwable cause) {
    super(message, cause);
  }

  public JSONException(Throwable cause) {
    super(cause);
  }
}
