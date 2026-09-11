// SPDX-License-Identifier: GPL-3.0-only
package calculator;

/**
 * The arithmetic behind {@link CalculatorActivity}: a four-function, immediate-execution
 * calculator, the model behind the basic layout of Android's own Calculator. A key press is one
 * call; the Activity then reads {@link #display} and {@link #expression} and repaints.
 *
 * <p>No views and no framework calls live here, so the whole behaviour of the app is one class that
 * can be reasoned about — and, on a host JVM, tested — on its own.
 *
 * <p>Immediate execution means each operator key finishes the pending one: {@code 2 + 3 x 4 =} is
 * {@code (2 + 3) x 4}, not {@code 2 + (3 x 4)}. That is what a pocket calculator does, and what the
 * four-function layout leads a finger to expect. A repeated {@code =} repeats the last operator and
 * its right operand, so {@code 2 + 3 = = } shows 8.
 */
final class CalculatorEngine {

  /** The value of {@link #pending} and {@link #repeatOp} when no operator is waiting. */
  private static final char NO_OP = 0;

  /** {@code Double.MAX_VALUE}, spelled out: the SDK serves Double's methods, not its fields. */
  private static final double MAX_FINITE = 1.7976931348623157e308;

  /**
   * Digits a typed entry may hold. Twelve fits the display comfortably and stays well inside the 15
   * digits a {@code double} carries exactly.
   */
  private static final int MAX_DIGITS = 12;

  /**
   * What the user is typing, as typed — kept as text so "0." and "-0" survive until the next key.
   */
  private String entry = "0";

  /** True while {@link #entry} is the displayed value; false while {@link #shown} is. */
  private boolean typing;

  /** The displayed value when the user is not typing: a result, or a cleared zero. */
  private double shown;

  /** Left operand of {@link #pending}. */
  private double accumulator;

  /** The operator waiting for a right operand, or {@link #NO_OP}. */
  private char pending = NO_OP;

  /** What a repeated {@code =} re-applies, and to what. */
  private char repeatOp = NO_OP;

  private double repeatOperand;

  /** The secondary line: the computation that led to the displayed value. */
  private String expression = "";

  /** Set by a division by zero or an overflow; every key but clear is ignored until cleared. */
  private boolean error;

  // ── Keys ──────────────────────────────────────────────────────────────────

  /** Append a digit to the entry, starting a new one if the display holds a result. */
  void digit(int d) {
    if (error) {
      return;
    }
    if (!typing) {
      startEntry();
    }
    if (digitCount() >= MAX_DIGITS) {
      return;
    }
    char c = (char) ('0' + d);
    if (entry.equals("0")) {
      entry = String.valueOf(c);
    } else if (entry.equals("-0")) {
      entry = "-" + c;
    } else {
      entry = entry + c;
    }
  }

  /** Begin the fractional part. A second decimal point is ignored, as on Android. */
  void dot() {
    if (error) {
      return;
    }
    if (!typing) {
      startEntry();
    }
    if (entry.indexOf('.') < 0 && digitCount() < MAX_DIGITS) {
      entry = entry + ".";
    }
  }

  /**
   * Apply a pending operator, if the user typed a right operand for it, and make {@code op} the
   * pending one. Pressing two operators in a row just changes which one is pending.
   */
  void operator(char op) {
    if (error) {
      return;
    }
    if (pending != NO_OP && typing) {
      double r = apply(accumulator, value(), pending);
      if (error) {
        return;
      }
      shown = r;
    } else {
      shown = value();
    }
    accumulator = shown;
    typing = false;
    repeatOp = NO_OP;
    pending = op;
    expression = format(accumulator) + " " + op;
  }

  /** Finish the pending operator, or repeat the last one. */
  void equals() {
    if (error) {
      return;
    }
    double lhs;
    double rhs;
    char op;
    if (pending != NO_OP) {
      lhs = accumulator;
      rhs = value();
      op = pending;
    } else if (repeatOp != NO_OP) {
      lhs = value();
      rhs = repeatOperand;
      op = repeatOp;
    } else {
      shown = value();
      typing = false;
      expression = format(shown) + " =";
      return;
    }
    double r = apply(lhs, rhs, op);
    pending = NO_OP;
    typing = false;
    expression = format(lhs) + " " + op + " " + format(rhs) + " =";
    if (error) {
      return;
    }
    repeatOp = op;
    repeatOperand = rhs;
    shown = r;
    accumulator = r;
  }

  /** Flip the sign of whatever is on the display. */
  void negate() {
    if (error) {
      return;
    }
    if (typing) {
      entry = entry.startsWith("-") ? entry.substring(1) : "-" + entry;
    } else {
      shown = -shown;
    }
  }

  /** Read the display as a percentage: 50 becomes 0.5. */
  void percent() {
    if (error) {
      return;
    }
    shown = value() / 100;
    typing = false;
  }

  /**
   * The clear key. Pressing it once drops the current entry; pressing it again, with nothing left
   * to drop, clears the pending operator too. {@link #clearLabel} says which of the two the next
   * press will do, the way Android's C / AC key does.
   */
  void clear() {
    if (!error && (typing || shown != 0)) {
      entry = "0";
      typing = false;
      shown = 0;
      return;
    }
    entry = "0";
    typing = false;
    shown = 0;
    accumulator = 0;
    pending = NO_OP;
    repeatOp = NO_OP;
    repeatOperand = 0;
    expression = "";
    error = false;
  }

  // ── Display ───────────────────────────────────────────────────────────────

  /** The primary line: the entry being typed, the current result, or {@code Error}. */
  String display() {
    if (error) {
      return "Error";
    }
    return typing ? entry : format(shown);
  }

  /** The secondary line: the computation behind the display, empty when there is none. */
  String expression() {
    return error ? "" : expression;
  }

  /** {@code C} while there is something to clear, {@code AC} when the next press clears it all. */
  String clearLabel() {
    boolean fresh = !error && !typing && shown == 0 && pending == NO_OP && repeatOp == NO_OP;
    return fresh ? "AC" : "C";
  }

  // ── Internals ─────────────────────────────────────────────────────────────

  /** Start typing a fresh entry; the first digit replaces the zero. */
  private void startEntry() {
    if (pending == NO_OP) {
      // Nothing is waiting on this number, so it opens a new calculation.
      expression = "";
    }
    entry = "0";
    typing = true;
  }

  private double value() {
    return typing ? Double.parseDouble(entry) : shown;
  }

  private int digitCount() {
    int n = 0;
    for (int i = 0; i < entry.length(); i++) {
      char c = entry.charAt(i);
      if (c >= '0' && c <= '9') {
        n++;
      }
    }
    return n;
  }

  /** Apply one operator, raising {@link #error} on a result no display can show. */
  private double apply(double a, double b, char op) {
    double r;
    switch (op) {
      case '+':
        r = a + b;
        break;
      case '-':
        r = a - b;
        break;
      case 'x':
        r = a * b;
        break;
      case '/':
        r = a / b;
        break;
      default:
        r = b;
        break;
    }
    // The SDK serves no Double.isNaN / isInfinite, and needs neither: a division
    // by zero or an overflow leaves NaN or an infinity, and both fail this test
    // — NaN because every comparison against it is false, an infinity because it
    // is larger than any finite double.
    if (!(Math.abs(r) <= MAX_FINITE)) {
      error = true;
      return 0;
    }
    return r;
  }

  /**
   * A double as a calculator shows it: whole values without a decimal point, everything else to ten
   * significant digits with the trailing zeros cut, and the very large and very small in exponent
   * form. Ten digits is what keeps {@code 0.1 + 0.2} reading as {@code 0.3} rather than as the
   * binary value's full {@code 0.30000000000000004}.
   */
  static String format(double v) {
    if (v == 0) {
      // Also folds -0.0, which no calculator displays as "-0".
      return "0";
    }
    long whole = (long) v;
    if (v == whole && Math.abs(v) < 1e15) {
      return String.valueOf(whole);
    }
    String s = String.format("%.10g", v);
    int e = s.indexOf('e');
    String mantissa = e < 0 ? s : s.substring(0, e);
    String exponent = e < 0 ? "" : s.substring(e);
    if (mantissa.indexOf('.') >= 0) {
      int end = mantissa.length();
      while (end > 0 && mantissa.charAt(end - 1) == '0') {
        end--;
      }
      if (end > 0 && mantissa.charAt(end - 1) == '.') {
        end--;
      }
      mantissa = mantissa.substring(0, end);
    }
    return mantissa + exponent;
  }
}
