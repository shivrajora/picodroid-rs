// SPDX-License-Identifier: GPL-3.0-only
package kotlin.text;

/**
 * The two {@code Char} extensions that do not inline to {@code Character} statics. ASCII only, like
 * the {@code Character} builtins.
 */
public final class CharsKt {
  private CharsKt() {}

  public static boolean isWhitespace(char c) {
    return c == ' ' || (c >= '\t' && c <= '\r') || (c >= 0x1C && c <= 0x1F);
  }

  public static int digitToInt(char c) {
    int digit = c - '0';
    if (digit < 0 || digit > 9) {
      throw new IllegalArgumentException("Char " + c + " is not a decimal digit");
    }
    return digit;
  }
}
