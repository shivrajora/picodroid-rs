// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.os.Bundle;

/**
 * A description of something to do later, held by {@link AlarmManager} on behalf of this app: start
 * an Activity ({@link #getActivity}) or deliver a broadcast ({@link #getBroadcast}). Mirrors {@code
 * android.app.PendingIntent}; there is no {@code getService}, a Service being started from a
 * receiver as easily.
 *
 * <pre>{@code
 * PendingIntent ring = PendingIntent.getActivity(this, 0,
 *     new Intent(RingActivity.class).putExtra("alarm", id),
 *     PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
 * alarmManager.setExact(AlarmManager.RTC_WAKEUP, when, ring);
 *
 * PendingIntent poll = PendingIntent.getBroadcast(this, 1,
 *     new Intent(this, PollReceiver.class), PendingIntent.FLAG_IMMUTABLE);
 * alarmManager.setExact(AlarmManager.ELAPSED_REALTIME_WAKEUP, later, poll);
 * }</pre>
 *
 * <p>The framework keeps the target, the request code and the extras as plain bytes, outside any
 * app's heap, which is what lets an alarm outlive the app that set it. That store is small and
 * untyped, hence the limits below: at most {@link #MAX_EXTRAS} extras, each an {@code int} under a
 * key of at most {@link #MAX_KEY_LENGTH} characters, and a target name of at most {@link
 * #MAX_TARGET_LENGTH} characters. Everything else an Intent can carry is rejected here, where the
 * mistake is, rather than silently dropped at the far end.
 *
 * <p>A broadcast operation's Intent names either a receiver class ({@code new Intent(this,
 * MyReceiver.class)}, delivered to a new instance of it, as Android delivers to a manifest
 * receiver) or an action ({@code new Intent("com.example.POLL")}, delivered to the receivers
 * registered for it when it fires). When the alarm fires while the app is not running, the app is
 * started first — its {@code Application.onCreate} runs — and then the broadcast is delivered, so a
 * receiver registered there hears an action.
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

  /** The longest target — a class name or a broadcast action — the alarm store keeps. */
  public static final int MAX_TARGET_LENGTH = 48;

  /** What fires: an Activity started ({@link #getActivity}). */
  static final int KIND_ACTIVITY = 0;

  /** What fires: a receiver class instantiated and heard ({@link #getBroadcast} with a class). */
  static final int KIND_RECEIVER = 1;

  /** What fires: the registered receivers for an action ({@link #getBroadcast} with an action). */
  static final int KIND_ACTION = 2;

  final int kind;
  final int requestCode;

  /** The Activity or receiver class in JVM form, or the action. */
  final String target;

  final String key0;
  final int value0;
  final String key1;
  final int value1;

  private PendingIntent(
      int kind, int requestCode, String target, String key0, int value0, String key1, int value1) {
    this.kind = kind;
    this.requestCode = requestCode;
    this.target = target;
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
   *     {@code int}, or carries a key longer than {@link #MAX_KEY_LENGTH}
   */
  public static PendingIntent getActivity(
      Context context, int requestCode, Intent intent, int flags) {
    if (intent == null || intent.getTargetClassName() == null) {
      throw new IllegalArgumentException("PendingIntent needs an Intent naming an Activity class");
    }
    return create(KIND_ACTIVITY, requestCode, intent.getTargetClassName(), intent, flags);
  }

  /**
   * An operation that delivers {@code intent} as a broadcast in this app: to a new instance of the
   * receiver class it names, or to the receivers registered for its action. Mirrors {@code
   * android.app.PendingIntent#getBroadcast}; the identity rules are {@link #getActivity}'s.
   *
   * @throws IllegalArgumentException if the Intent names neither a class nor an action, names a
   *     target longer than {@link #MAX_TARGET_LENGTH}, targets another package, or carries extras
   *     the store cannot keep (see {@link #getActivity})
   */
  public static PendingIntent getBroadcast(
      Context context, int requestCode, Intent intent, int flags) {
    if (intent == null) {
      throw new IllegalArgumentException("null Intent");
    }
    String target = intent.getTargetClassName();
    int kind = KIND_RECEIVER;
    if (target == null) {
      target = intent.getAction();
      kind = KIND_ACTION;
    }
    if (target == null) {
      throw new IllegalArgumentException(
          "PendingIntent.getBroadcast needs an Intent naming a receiver class or an action");
    }
    return create(kind, requestCode, target, intent, flags);
  }

  private static PendingIntent create(
      int kind, int requestCode, String target, Intent intent, int flags) {
    if (intent.getPackage() != null) {
      throw new IllegalArgumentException("a PendingIntent cannot target another package");
    }
    if (target.length() > MAX_TARGET_LENGTH) {
      throw new IllegalArgumentException(
          "target longer than " + MAX_TARGET_LENGTH + ": '" + target + "'");
    }
    Bundle extras = intent.getExtras();
    int n = extras == null ? 0 : extras.size();
    if (n > MAX_EXTRAS) {
      throw new IllegalArgumentException("at most " + MAX_EXTRAS + " extras, not " + n);
    }
    String[] keys = new String[MAX_EXTRAS];
    int[] vals = new int[MAX_EXTRAS];
    if (extras != null) {
      int i = 0;
      for (String key : extras.keySet()) {
        if (!(extras.get(key) instanceof Integer)) {
          throw new IllegalArgumentException("extra '" + key + "' is not an int");
        }
        if (key.length() > MAX_KEY_LENGTH) {
          throw new IllegalArgumentException(
              "extra key longer than " + MAX_KEY_LENGTH + ": '" + key + "'");
        }
        keys[i] = key;
        vals[i] = extras.getInt(key);
        i++;
      }
    }
    if ((flags & FLAG_NO_CREATE) != 0) {
      return null;
    }
    return new PendingIntent(kind, requestCode, target, keys[0], vals[0], keys[1], vals[1]);
  }

  /** Removes the alarm set with this operation, if one is still armed. */
  public void cancel() {
    AlarmManager.getInstance().cancel(this);
  }

  /**
   * Two operations are equal when they would replace one another: same kind, same request code,
   * same target. The extras are not part of the identity, exactly as an Android PendingIntent
   * ignores its Intent's extras when matching.
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
    return kind == that.kind && requestCode == that.requestCode && target.equals(that.target);
  }

  @Override
  public int hashCode() {
    return (requestCode * 31 + target.hashCode()) * 3 + kind;
  }
}
