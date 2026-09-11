// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;

/**
 * A description of an Activity to start later, held by {@link AlarmManager} on behalf of this app.
 * Mirrors {@code android.app.PendingIntent}, narrowed to what an alarm needs: {@link #getActivity}
 * is the only factory, there being no broadcasts or started Services to point one at.
 *
 * <pre>{@code
 * PendingIntent pi = PendingIntent.getActivity(this, 0,
 *     new Intent(RingActivity.class).putExtra("alarm", id),
 *     PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
 * alarmManager.setExact(AlarmManager.RTC_WAKEUP, when, pi);
 * }</pre>
 *
 * <p>The framework keeps the target class, the request code and the extras as plain bytes, outside
 * any app's heap, which is what lets an alarm outlive the app that set it. That store is small and
 * untyped, hence the limits below: at most {@link #MAX_EXTRAS} extras, each an {@code int} under a
 * key of at most {@link #MAX_KEY_LENGTH} characters. Everything else an Intent can carry is
 * rejected here, where the mistake is, rather than silently dropped at the far end.
 *
 * <p>Multi-app boards only.
 */
public final class PendingIntent {
  /** Android's flag value; accepted and ignored, every alarm here being consumed by its fire. */
  public static final int FLAG_ONE_SHOT = 1 << 30;

  /** Android's flag value: return {@code null} rather than creating anything. */
  public static final int FLAG_NO_CREATE = 1 << 29;

  /** Android's flag value; accepted and ignored: a set always replaces by identity. */
  public static final int FLAG_CANCEL_CURRENT = 1 << 28;

  /** Android's flag value, and what this implementation always does: a set replaces by identity. */
  public static final int FLAG_UPDATE_CURRENT = 1 << 27;

  /** Android's flag value; accepted and ignored: the extras are copied at creation either way. */
  public static final int FLAG_IMMUTABLE = 1 << 26;

  /** Android's flag value; accepted and ignored. */
  public static final int FLAG_MUTABLE = 1 << 25;

  /** How many extras the framework's alarm store keeps per entry. */
  public static final int MAX_EXTRAS = 2;

  /** The longest extra key the framework's alarm store keeps. */
  public static final int MAX_KEY_LENGTH = 15;

  final int requestCode;
  final String targetClassName;
  final String key0;
  final int value0;
  final String key1;
  final int value1;

  private PendingIntent(
      int requestCode, String targetClassName, String key0, int value0, String key1, int value1) {
    this.requestCode = requestCode;
    this.targetClassName = targetClassName;
    this.key0 = key0;
    this.value0 = value0;
    this.key1 = key1;
    this.value1 = value1;
  }

  /**
   * An operation that starts {@code intent}'s Activity in this app. {@code requestCode} tells two
   * operations on the same Activity apart: a set replaces the alarm with the same request code and
   * target, and {@link #cancel} removes it.
   *
   * <p>{@code context} is accepted for source compatibility and otherwise unused. Of the flags,
   * {@link #FLAG_NO_CREATE} returns {@code null}; the rest are accepted and ignored, since a set
   * always replaces by identity and every alarm is consumed when it fires.
   *
   * @throws IllegalArgumentException if the Intent names no Activity class, targets another
   *     package, carries more than {@link #MAX_EXTRAS} extras, carries an extra that is not an
   *     {@code int}, or carries a key longer than {@link #MAX_KEY_LENGTH}.
   */
  public static PendingIntent getActivity(
      Context context, int requestCode, Intent intent, int flags) {
    if (intent == null || intent.getTargetClassName() == null) {
      throw new IllegalArgumentException("PendingIntent needs an Intent naming an Activity class");
    }
    if (intent.getPackage() != null) {
      throw new IllegalArgumentException("a PendingIntent cannot target another package");
    }
    int n = intent.extraCount();
    if (n > MAX_EXTRAS) {
      throw new IllegalArgumentException("at most " + MAX_EXTRAS + " extras, not " + n);
    }
    for (int i = 0; i < n; i++) {
      if (!intent.isIntExtra(i)) {
        throw new IllegalArgumentException("extra '" + intent.extraKey(i) + "' is not an int");
      }
      if (intent.extraKey(i).length() > MAX_KEY_LENGTH) {
        throw new IllegalArgumentException(
            "extra key longer than " + MAX_KEY_LENGTH + ": '" + intent.extraKey(i) + "'");
      }
    }
    if ((flags & FLAG_NO_CREATE) != 0) {
      return null;
    }
    return new PendingIntent(
        requestCode,
        intent.getTargetClassName(),
        n > 0 ? intent.extraKey(0) : null,
        n > 0 ? intent.extraInt(0) : 0,
        n > 1 ? intent.extraKey(1) : null,
        n > 1 ? intent.extraInt(1) : 0);
  }

  /** Removes the alarm set with this operation, if one is still armed. */
  public void cancel() {
    AlarmManager.getInstance().cancel(this);
  }

  /**
   * Two operations are equal when they would replace one another: same request code, same target
   * Activity. The extras are not part of the identity, exactly as an Android PendingIntent ignores
   * its Intent's extras when matching.
   */
  @Override
  public boolean equals(Object other) {
    if (this == other) {
      return true;
    }
    if (!(other instanceof PendingIntent)) {
      return false;
    }
    PendingIntent that = (PendingIntent) other;
    return requestCode == that.requestCode && targetClassName.equals(that.targetClassName);
  }

  @Override
  public int hashCode() {
    return requestCode * 31 + targetClassName.hashCode();
  }
}
