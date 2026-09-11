// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Intent;

/**
 * Schedules an Activity to start at a time, mirroring {@code android.app.AlarmManager}: obtain it
 * with {@code getSystemService(Context.ALARM_SERVICE)}.
 *
 * <p>What it is for is outliving your own app. The framework keeps the alarm outside every app's
 * heap, so it still fires after the user has gone to the launcher or into another app, and the app
 * that set it is started again to receive it. That is the whole reason to prefer an alarm over a
 * thread that sleeps: a thread dies when the app is torn down.
 *
 * <pre>{@code
 * AlarmManager am = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
 * am.setExact(AlarmManager.RTC_WAKEUP, whenMillis,
 *     PendingIntent.getActivity(this, 0, new Intent(RingActivity.class), 0));
 * }</pre>
 *
 * <p>Alarms live in RAM and are lost at a reset, as the wall clock itself is on a board with no
 * battery-backed clock. An app re-registers what it still wants on its next start, the way an
 * Android app re-registers after {@code BOOT_COMPLETED}.
 *
 * <p>Multi-app boards only.
 */
public final class AlarmManager {
  /** Wall-clock time, waking the device if it sleeps. Android's value. */
  public static final int RTC_WAKEUP = 0;

  /** Wall-clock time. Identical to {@link #RTC_WAKEUP} here: nothing suspends the JVM. */
  public static final int RTC = 1;

  /** Time since boot ({@code SystemClock.elapsedRealtime}), waking the device. Android's value. */
  public static final int ELAPSED_REALTIME_WAKEUP = 2;

  /** Time since boot. Identical to {@link #ELAPSED_REALTIME_WAKEUP} here. */
  public static final int ELAPSED_REALTIME = 3;

  private static final AlarmManager INSTANCE = new AlarmManager();

  private AlarmManager() {}

  public static AlarmManager getInstance() {
    return INSTANCE;
  }

  /**
   * Schedule {@code operation} for {@code triggerAtMillis} on the clock named by {@code type}.
   * Identical to {@link #setExact}: Android may batch a {@code set} to save power, and there is no
   * such trade to make here.
   */
  public void set(int type, long triggerAtMillis, PendingIntent operation) {
    setExact(type, triggerAtMillis, operation);
  }

  /**
   * Schedule {@code operation} for {@code triggerAtMillis}, replacing any alarm already set with an
   * equal operation. An alarm whose time has already passed fires at the next opportunity.
   *
   * @throws IllegalArgumentException if {@code type} is not one of the four constants, or {@code
   *     operation} is null
   * @throws IllegalStateException if the framework has no room for another alarm
   */
  public void setExact(int type, long triggerAtMillis, PendingIntent operation) {
    if (operation == null) {
      throw new IllegalArgumentException("null operation");
    }
    if (type < RTC_WAKEUP || type > ELAPSED_REALTIME) {
      throw new IllegalArgumentException("unknown alarm type " + type);
    }
    int result =
        nativeSet(
            type,
            triggerAtMillis,
            operation.requestCode,
            operation.targetClassName,
            operation.key0,
            operation.value0,
            operation.key1,
            operation.value1);
    if (result == 1) {
      throw new IllegalStateException("Maximum limit of concurrent alarms reached");
    }
    if (result != 0) {
      throw new IllegalArgumentException("the framework refused this operation: name too long");
    }
  }

  /**
   * Remove the alarm set with an equal operation. Cancelling an alarm that is not set, or one
   * already on its way to being delivered, does nothing.
   */
  public void cancel(PendingIntent operation) {
    if (operation != null) {
      nativeCancel(operation.requestCode, operation.targetClassName);
    }
  }

  /**
   * Deliver a fired alarm: the framework calls this on the UI thread, in the app that set it and
   * once it is running again. Rebuilding the Intent here rather than natively keeps one code path
   * for starting an Activity — the same {@code startActivity} every app calls.
   */
  static void fireAlarm(String className, String key0, int value0, String key1, int value1) {
    Intent intent = new Intent().setClassName(null, className);
    if (key0 != null) {
      intent.putExtra(key0, value0);
    }
    if (key1 != null) {
      intent.putExtra(key1, value1);
    }
    INSTANCE.startActivity(intent);
  }

  /** Pushes the Activity, exactly as {@code Activity.startActivity} does. */
  private native void startActivity(Intent intent);

  /** 0 when the alarm is set, 1 when the framework has no room, 2 when it refused a name. */
  private static native int nativeSet(
      int type,
      long triggerAtMillis,
      int requestCode,
      String className,
      String key0,
      int value0,
      String key1,
      int value1);

  /** Whether an alarm was armed under this identity and has now been removed. */
  private static native boolean nativeCancel(int requestCode, String className);
}
