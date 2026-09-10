// SPDX-License-Identifier: GPL-3.0-only
package kotlin.text;

import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.NoSuchElementException;
import java.util.Set;
import kotlin.Pair;
import kotlin.ranges.IntRange;

/**
 * The non-inline part of {@code kotlin.text}. {@code CharSequence} receivers are read through
 * {@code toString()} (a {@code String} returns itself; a {@code StringBuilder} snapshots) and the
 * work is done on {@code String} builtins. {@code $default} bridges: mask bit i = the i-th
 * parameter after the receiver takes its default.
 *
 * <p>Inline (never here): {@code toInt}/{@code toFloat}/{@code toDouble}/{@code toLong} → {@code
 * parseX}, {@code toBoolean} → {@code Boolean.parseBoolean}, {@code format}, {@code uppercase}/
 * {@code lowercase}, {@code isEmpty}/{@code isNotEmpty}/{@code isNullOrEmpty}, {@code substring
 * (int, int)}, {@code filter}/{@code map}/{@code forEach}/{@code count}/{@code any}/{@code all},
 * {@code trimIndent}/{@code trimMargin} on literals (folded by kotlinc).
 */
public final class StringsKt {
  private StringsKt() {}

  private static IllegalArgumentException negativeCount(int n) {
    return new IllegalArgumentException("Requested character count " + n + " is less than zero.");
  }

  private static boolean regionMatches(String s, int at, String needle, boolean ignoreCase) {
    int n = needle.length();
    if (at < 0 || at + n > s.length()) {
      return false;
    }
    for (int i = 0; i < n; i++) {
      char a = s.charAt(at + i);
      char b = needle.charAt(i);
      if (a != b && (!ignoreCase || Character.toLowerCase(a) != Character.toLowerCase(b))) {
        return false;
      }
    }
    return true;
  }

  private static int indexOf(String s, String needle, int start, boolean ignoreCase) {
    if (start < 0) {
      start = 0;
    }
    if (!ignoreCase) {
      if (start == 0) {
        return s.indexOf(needle);
      }
      if (start > s.length()) {
        return needle.isEmpty() ? s.length() : -1;
      }
      int i = s.substring(start).indexOf(needle);
      return i < 0 ? -1 : i + start;
    }
    for (int i = start; i + needle.length() <= s.length(); i++) {
      if (regionMatches(s, i, needle, true)) {
        return i;
      }
    }
    return -1;
  }

  private static int lastIndexOf(String s, String needle, int start, boolean ignoreCase) {
    int from = s.length() - needle.length();
    if (start < from) {
      from = start;
    }
    if (!ignoreCase && from == s.length() - needle.length()) {
      return s.lastIndexOf(needle);
    }
    for (int i = from; i >= 0; i--) {
      if (regionMatches(s, i, needle, ignoreCase)) {
        return i;
      }
    }
    return -1;
  }

  private static boolean sameChar(char a, char b, boolean ignoreCase) {
    return a == b || (ignoreCase && Character.toLowerCase(a) == Character.toLowerCase(b));
  }

  // ── chars ─────────────────────────────────────────────────────────────────

  public static char first(CharSequence cs) {
    String s = cs.toString();
    if (s.isEmpty()) {
      throw new NoSuchElementException("Char sequence is empty.");
    }
    return s.charAt(0);
  }

  public static char last(CharSequence cs) {
    String s = cs.toString();
    if (s.isEmpty()) {
      throw new NoSuchElementException("Char sequence is empty.");
    }
    return s.charAt(s.length() - 1);
  }

  public static Character firstOrNull(CharSequence cs) {
    String s = cs.toString();
    return s.isEmpty() ? null : Character.valueOf(s.charAt(0));
  }

  public static Character getOrNull(CharSequence cs, int index) {
    String s = cs.toString();
    return index >= 0 && index < s.length() ? Character.valueOf(s.charAt(index)) : null;
  }

  public static int getLastIndex(CharSequence cs) {
    return cs.toString().length() - 1;
  }

  public static IntRange getIndices(CharSequence cs) {
    return new IntRange(0, cs.toString().length() - 1);
  }

  public static List toList(CharSequence cs) {
    String s = cs.toString();
    ArrayList<Object> out = new ArrayList<Object>(s.length());
    for (int i = 0; i < s.length(); i++) {
      out.add(Character.valueOf(s.charAt(i)));
    }
    return out;
  }

  public static Set toSet(CharSequence cs) {
    String s = cs.toString();
    LinkedHashSet<Object> out = new LinkedHashSet<Object>();
    for (int i = 0; i < s.length(); i++) {
      out.add(Character.valueOf(s.charAt(i)));
    }
    return out;
  }

  public static List zip(CharSequence a, CharSequence b) {
    String sa = a.toString();
    String sb = b.toString();
    int n = sa.length() < sb.length() ? sa.length() : sb.length();
    ArrayList<Object> out = new ArrayList<Object>(n);
    for (int i = 0; i < n; i++) {
      out.add(new Pair(Character.valueOf(sa.charAt(i)), Character.valueOf(sb.charAt(i))));
    }
    return out;
  }

  /**
   * {@code String(chars)} / {@code chars.concatToString()}: the JVM has no {@code String(char[])}.
   */
  public static String concatToString(char[] chars) {
    StringBuilder sb = new StringBuilder();
    for (char c : chars) {
      sb.append(c);
    }
    return sb.toString();
  }

  // ── whitespace ────────────────────────────────────────────────────────────

  public static boolean isBlank(CharSequence cs) {
    String s = cs.toString();
    for (int i = 0; i < s.length(); i++) {
      if (!CharsKt.isWhitespace(s.charAt(i))) {
        return false;
      }
    }
    return true;
  }

  public static CharSequence trim(CharSequence cs) {
    String s = cs.toString();
    int start = 0;
    int end = s.length();
    while (start < end && CharsKt.isWhitespace(s.charAt(start))) {
      start++;
    }
    while (end > start && CharsKt.isWhitespace(s.charAt(end - 1))) {
      end--;
    }
    return s.substring(start, end);
  }

  public static CharSequence trimStart(CharSequence cs) {
    String s = cs.toString();
    int start = 0;
    while (start < s.length() && CharsKt.isWhitespace(s.charAt(start))) {
      start++;
    }
    return s.substring(start);
  }

  public static CharSequence trimEnd(CharSequence cs) {
    String s = cs.toString();
    int end = s.length();
    while (end > 0 && CharsKt.isWhitespace(s.charAt(end - 1))) {
      end--;
    }
    return s.substring(0, end);
  }

  // ── comparison ────────────────────────────────────────────────────────────

  public static boolean equals(String a, String b, boolean ignoreCase) {
    if (a == null) {
      return b == null;
    }
    if (b == null) {
      return false;
    }
    return ignoreCase ? a.equalsIgnoreCase(b) : a.equals(b);
  }

  public static int compareTo(String a, String b, boolean ignoreCase) {
    return ignoreCase ? a.toLowerCase().compareTo(b.toLowerCase()) : a.compareTo(b);
  }

  public static boolean startsWith(String s, String prefix, boolean ignoreCase) {
    return ignoreCase ? regionMatches(s, 0, prefix, true) : s.startsWith(prefix);
  }

  public static boolean startsWith$default(
      String s, String prefix, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    return startsWith(s, prefix, ignoreCase);
  }

  public static boolean endsWith(String s, String suffix, boolean ignoreCase) {
    return ignoreCase
        ? regionMatches(s, s.length() - suffix.length(), suffix, true)
        : s.endsWith(suffix);
  }

  public static boolean endsWith$default(
      String s, String suffix, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    return endsWith(s, suffix, ignoreCase);
  }

  public static boolean startsWith$default(
      CharSequence cs, char c, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    String s = cs.toString();
    return !s.isEmpty() && sameChar(s.charAt(0), c, ignoreCase);
  }

  public static boolean endsWith$default(
      CharSequence cs, char c, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    String s = cs.toString();
    return !s.isEmpty() && sameChar(s.charAt(s.length() - 1), c, ignoreCase);
  }

  // ── searching ─────────────────────────────────────────────────────────────

  public static boolean contains(CharSequence cs, CharSequence other, boolean ignoreCase) {
    return indexOf(cs.toString(), other.toString(), 0, ignoreCase) >= 0;
  }

  public static boolean contains$default(
      CharSequence cs, CharSequence other, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    return contains(cs, other, ignoreCase);
  }

  public static boolean contains$default(
      CharSequence cs, char c, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    return indexOf$default(cs, c, 0, ignoreCase, 0, null) >= 0;
  }

  public static int indexOf$default(
      CharSequence cs, String needle, int startIndex, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      startIndex = 0;
    }
    if ((mask & 4) != 0) {
      ignoreCase = false;
    }
    return indexOf(cs.toString(), needle, startIndex, ignoreCase);
  }

  public static int indexOf$default(
      CharSequence cs, char c, int startIndex, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 2) != 0) {
      startIndex = 0;
    }
    if ((mask & 4) != 0) {
      ignoreCase = false;
    }
    String s = cs.toString();
    if (!ignoreCase && startIndex <= 0) {
      return s.indexOf(c);
    }
    for (int i = startIndex < 0 ? 0 : startIndex; i < s.length(); i++) {
      if (sameChar(s.charAt(i), c, ignoreCase)) {
        return i;
      }
    }
    return -1;
  }

  public static int lastIndexOf$default(
      CharSequence cs, String needle, int startIndex, boolean ignoreCase, int mask, Object marker) {
    String s = cs.toString();
    if ((mask & 2) != 0) {
      startIndex = s.length() - 1;
    }
    if ((mask & 4) != 0) {
      ignoreCase = false;
    }
    return lastIndexOf(s, needle, startIndex, ignoreCase);
  }

  // ── building ──────────────────────────────────────────────────────────────

  public static String repeat(CharSequence cs, int n) {
    if (n < 0) {
      throw new IllegalArgumentException("Count 'n' must be non-negative, but was " + n + ".");
    }
    String s = cs.toString();
    if (n == 0 || s.isEmpty()) {
      return "";
    }
    if (n == 1) {
      return s;
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < n; i++) {
      sb.append(s);
    }
    return sb.toString();
  }

  public static CharSequence reversed(CharSequence cs) {
    String s = cs.toString();
    StringBuilder sb = new StringBuilder();
    for (int i = s.length() - 1; i >= 0; i--) {
      sb.append(s.charAt(i));
    }
    return sb.toString();
  }

  public static String padStart(String s, int length, char padChar) {
    if (length < 0) {
      throw new IllegalArgumentException("Desired length " + length + " is less than zero.");
    }
    if (length <= s.length()) {
      return s;
    }
    StringBuilder sb = new StringBuilder();
    for (int i = s.length(); i < length; i++) {
      sb.append(padChar);
    }
    sb.append(s);
    return sb.toString();
  }

  public static String padStart$default(
      String s, int length, char padChar, int mask, Object marker) {
    if ((mask & 2) != 0) {
      padChar = ' ';
    }
    return padStart(s, length, padChar);
  }

  public static String padEnd(String s, int length, char padChar) {
    if (length < 0) {
      throw new IllegalArgumentException("Desired length " + length + " is less than zero.");
    }
    if (length <= s.length()) {
      return s;
    }
    StringBuilder sb = new StringBuilder(s);
    for (int i = s.length(); i < length; i++) {
      sb.append(padChar);
    }
    return sb.toString();
  }

  public static String padEnd$default(String s, int length, char padChar, int mask, Object marker) {
    if ((mask & 2) != 0) {
      padChar = ' ';
    }
    return padEnd(s, length, padChar);
  }

  /**
   * Case-sensitive replace goes to the builtin (an empty {@code oldValue} is left unchanged:
   * documented).
   */
  public static String replace(String s, String oldValue, String newValue, boolean ignoreCase) {
    if (!ignoreCase) {
      return s.replace(oldValue, newValue);
    }
    if (oldValue.isEmpty()) {
      return s;
    }
    StringBuilder sb = new StringBuilder();
    int cur = 0;
    while (true) {
      int i = indexOf(s, oldValue, cur, true);
      if (i < 0) {
        break;
      }
      sb.append(s.substring(cur, i));
      sb.append(newValue);
      cur = i + oldValue.length();
    }
    sb.append(s.substring(cur));
    return sb.toString();
  }

  public static String replace$default(
      String s, String oldValue, String newValue, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 4) != 0) {
      ignoreCase = false;
    }
    return replace(s, oldValue, newValue, ignoreCase);
  }

  public static String replace$default(
      String s, char oldChar, char newChar, boolean ignoreCase, int mask, Object marker) {
    if ((mask & 4) != 0) {
      ignoreCase = false;
    }
    if (!ignoreCase) {
      return s.replace(oldChar, newChar);
    }
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < s.length(); i++) {
      char c = s.charAt(i);
      sb.append(sameChar(c, oldChar, true) ? newChar : c);
    }
    return sb.toString();
  }

  // ── splitting ─────────────────────────────────────────────────────────────

  /**
   * Kotlin semantics: keeps empty parts, delimiters tried in order at each index, {@code limit}
   * parts at most.
   */
  public static List split(CharSequence cs, String[] delimiters, boolean ignoreCase, int limit) {
    if (limit < 0) {
      throw new IllegalArgumentException("Limit must be non-negative, but was " + limit);
    }
    String s = cs.toString();
    ArrayList<Object> out = new ArrayList<Object>();
    int cur = 0;
    int len = s.length();
    while (limit == 0 || out.size() < limit - 1) {
      int at = -1;
      int width = 0;
      for (int i = cur; i <= len && at < 0; i++) {
        for (String d : delimiters) {
          if (!d.isEmpty() && regionMatches(s, i, d, ignoreCase)) {
            at = i;
            width = d.length();
            break;
          }
        }
      }
      if (at < 0) {
        break;
      }
      out.add(s.substring(cur, at));
      cur = at + width;
    }
    out.add(s.substring(cur));
    return out;
  }

  public static List split$default(
      CharSequence cs,
      String[] delimiters,
      boolean ignoreCase,
      int limit,
      int mask,
      Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    if ((mask & 4) != 0) {
      limit = 0;
    }
    return split(cs, delimiters, ignoreCase, limit);
  }

  public static List split$default(
      CharSequence cs, char[] delimiters, boolean ignoreCase, int limit, int mask, Object marker) {
    if ((mask & 2) != 0) {
      ignoreCase = false;
    }
    if ((mask & 4) != 0) {
      limit = 0;
    }
    String[] strings = new String[delimiters.length];
    for (int i = 0; i < delimiters.length; i++) {
      strings[i] = String.valueOf(delimiters[i]);
    }
    return split(cs, strings, ignoreCase, limit);
  }

  // ── substrings ────────────────────────────────────────────────────────────

  public static String substring(String s, IntRange range) {
    return s.substring(range.getFirst(), range.getLast() + 1);
  }

  public static String substringBefore$default(
      String s, String delimiter, String missing, int mask, Object marker) {
    if ((mask & 2) != 0) {
      missing = s;
    }
    int i = s.indexOf(delimiter);
    return i < 0 ? missing : s.substring(0, i);
  }

  public static String substringBefore$default(
      String s, char delimiter, String missing, int mask, Object marker) {
    if ((mask & 2) != 0) {
      missing = s;
    }
    int i = s.indexOf(delimiter);
    return i < 0 ? missing : s.substring(0, i);
  }

  public static String substringAfter(String s, String delimiter, String missing) {
    int i = s.indexOf(delimiter);
    return i < 0 ? missing : s.substring(i + delimiter.length());
  }

  public static String substringAfter$default(
      String s, String delimiter, String missing, int mask, Object marker) {
    if ((mask & 2) != 0) {
      missing = s;
    }
    return substringAfter(s, delimiter, missing);
  }

  public static String substringBeforeLast$default(
      String s, String delimiter, String missing, int mask, Object marker) {
    if ((mask & 2) != 0) {
      missing = s;
    }
    int i = s.lastIndexOf(delimiter);
    return i < 0 ? missing : s.substring(0, i);
  }

  public static String substringAfterLast$default(
      String s, String delimiter, String missing, int mask, Object marker) {
    if ((mask & 2) != 0) {
      missing = s;
    }
    int i = s.lastIndexOf(delimiter);
    return i < 0 ? missing : s.substring(i + delimiter.length());
  }

  public static String removePrefix(String s, CharSequence prefix) {
    String p = prefix.toString();
    return s.startsWith(p) ? s.substring(p.length()) : s;
  }

  public static String removeSuffix(String s, CharSequence suffix) {
    String x = suffix.toString();
    return s.endsWith(x) ? s.substring(0, s.length() - x.length()) : s;
  }

  public static String removeSurrounding(String s, CharSequence prefix, CharSequence suffix) {
    String p = prefix.toString();
    String x = suffix.toString();
    if (s.length() >= p.length() + x.length() && s.startsWith(p) && s.endsWith(x)) {
      return s.substring(p.length(), s.length() - x.length());
    }
    return s;
  }

  public static String take(String s, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    return s.substring(0, n < s.length() ? n : s.length());
  }

  public static String drop(String s, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    return s.substring(n < s.length() ? n : s.length());
  }

  public static String takeLast(String s, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    int len = s.length();
    return s.substring(len - (n < len ? n : len));
  }

  public static String dropLast(String s, int n) {
    if (n < 0) {
      throw negativeCount(n);
    }
    int keep = s.length() - n;
    return take(s, keep < 0 ? 0 : keep);
  }

  // ── parsing (no exceptions: validate, then the builtin parseX) ────────────

  public static Integer toIntOrNull(String s) {
    Long v = toLongOrNull(s);
    if (v == null) {
      return null;
    }
    long l = v.longValue();
    if (l < Integer.MIN_VALUE || l > Integer.MAX_VALUE) {
      return null;
    }
    return Integer.valueOf((int) l);
  }

  public static Long toLongOrNull(String s) {
    int len = s.length();
    if (len == 0) {
      return null;
    }
    int i = 0;
    boolean negative = false;
    char c0 = s.charAt(0);
    if (c0 == '-' || c0 == '+') {
      negative = c0 == '-';
      i = 1;
      if (len == 1) {
        return null;
      }
    }
    long result = 0;
    for (; i < len; i++) {
      int digit = s.charAt(i) - '0';
      if (digit < 0 || digit > 9) {
        return null;
      }
      if (result < (Long.MIN_VALUE + digit) / 10) {
        return null;
      }
      result = result * 10 - digit;
    }
    if (!negative) {
      if (result == Long.MIN_VALUE) {
        return null;
      }
      result = -result;
    }
    return Long.valueOf(result);
  }

  private static boolean isDecimal(String s) {
    int len = s.length();
    int i = 0;
    if (len > 0 && (s.charAt(0) == '-' || s.charAt(0) == '+')) {
      i = 1;
    }
    int digits = 0;
    boolean dot = false;
    boolean exp = false;
    int expDigits = 0;
    for (; i < len; i++) {
      char c = s.charAt(i);
      if (c >= '0' && c <= '9') {
        if (exp) {
          expDigits++;
        } else {
          digits++;
        }
      } else if (c == '.' && !dot && !exp) {
        dot = true;
      } else if ((c == 'e' || c == 'E') && !exp && digits > 0) {
        exp = true;
        if (i + 1 < len && (s.charAt(i + 1) == '-' || s.charAt(i + 1) == '+')) {
          i++;
        }
      } else {
        return false;
      }
    }
    return digits > 0 && (!exp || expDigits > 0);
  }

  public static Float toFloatOrNull(String s) {
    return isDecimal(s) ? Float.valueOf(Float.parseFloat(s)) : null;
  }

  public static Double toDoubleOrNull(String s) {
    return isDecimal(s) ? Double.valueOf(Double.parseDouble(s)) : null;
  }
}
