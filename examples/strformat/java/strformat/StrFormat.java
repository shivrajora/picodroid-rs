// SPDX-License-Identifier: GPL-3.0-only
package strformat;

import java.util.IllegalFormatException;
import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Exhaustively exercises String.format across all supported conversions, flags, widths, precisions,
 * and autoboxing paths. One Log line per case so regressions bisect cleanly against the checked-in
 * expected output.
 */
public class StrFormat extends Application {
  private static final String TAG = "StrFormat";

  public void onCreate() {
    run();
  }

  public static void run() {
    // ── Conversions ────────────────────────────────────────────────────────
    Log.i(TAG, String.format("s=[%s]", "pico"));
    Log.i(TAG, String.format("d=[%d]", 42));
    Log.i(TAG, String.format("x=[%x]", 255));
    Log.i(TAG, String.format("X=[%X]", 255));
    Log.i(TAG, String.format("o=[%o]", 8));
    Log.i(TAG, String.format("c=[%c]", 'A'));
    Log.i(TAG, String.format("b=[%b]", true));
    Log.i(TAG, String.format("f=[%f]", 3.14));
    Log.i(TAG, String.format("e=[%e]", 12345.678));
    Log.i(TAG, String.format("g=[%g]", 0.0001234));
    Log.i(TAG, String.format("pct=[%%]"));
    Log.i(TAG, String.format("nl=[%n]"));

    // ── Flags: - (left), 0 (zero pad), + (sign), ' ' (space), ',' (group), # (alt) ─
    Log.i(TAG, String.format("[%-10s]", "hi"));
    Log.i(TAG, String.format("[%05d]", 42));
    Log.i(TAG, String.format("[%+d]", 42));
    Log.i(TAG, String.format("[% d]", 42));
    Log.i(TAG, String.format("[%,d]", 1234567));
    Log.i(TAG, String.format("[%#x]", 255));
    Log.i(TAG, String.format("[%#o]", 8));

    // ── Width + precision ─────────────────────────────────────────────────
    Log.i(TAG, String.format("[%.3s]", "abcdef"));
    Log.i(TAG, String.format("[%10.4f]", 3.14159));
    Log.i(TAG, String.format("[%08.2f]", -1.5));

    // ── Autoboxing paths ──────────────────────────────────────────────────
    Log.i(TAG, String.format("int=%d", Integer.valueOf(7)));
    Log.i(TAG, String.format("long=%d", 9876543210L));
    Log.i(TAG, String.format("double=%.2f", Double.valueOf(2.5)));
    Log.i(TAG, String.format("bool=%b", Boolean.valueOf(true)));
    Log.i(TAG, String.format("null=%s", (Object) null));

    // ── Mixed single format string ────────────────────────────────────────
    Log.i(TAG, String.format("name=%s count=%d hex=%#06x done=%b", "pico", 42, 0xab, true));

    // ── Error case: too few args throws IllegalFormatException ────────────
    try {
      String.format("%d %d", 1);
      Log.i(TAG, "err=NOT_THROWN");
    } catch (IllegalFormatException e) {
      Log.i(TAG, "err=caught");
    } catch (RuntimeException e) {
      // Fallback catch in case IllegalFormatException isn't separately visible.
      Log.i(TAG, "err=caught_rt");
    }
  }
}
