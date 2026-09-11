// SPDX-License-Identifier: GPL-3.0-only
package picoclock;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.content.SharedPreferences;

/**
 * The alarms and the clock's own settings, backed by {@link SharedPreferences} so they survive a
 * power cut — which matters more here than on a phone, since a board that reboots at 3 a.m. with
 * its alarms forgotten is a board that does not wake anyone.
 *
 * <p>A fixed array of {@link #MAX_ALARMS}: the store is read once at startup and written through on
 * every edit, so a screen never waits on flash and the service never re-reads.
 */
@Singleton
public final class AlarmStore {
  /** The alarms a user can hold at once. Eight fills the list screen without scrolling. */
  public static final int MAX_ALARMS = 8;

  /** Minutes a snooze adds, matching the Android clock's default. */
  public static final int SNOOZE_MINUTES = 9;

  private static final String KEY_OFFSET = "tz.offset";
  private static final String KEY_USED = "used";

  private final SharedPreferences prefs;
  private final Alarm[] alarms = new Alarm[MAX_ALARMS];

  /** Which slots hold a user-created alarm, as a bit per id. */
  private int used;

  private int offsetMinutes;

  @Inject
  public AlarmStore(SharedPreferences prefs) {
    this.prefs = prefs;
    used = prefs.getInt(KEY_USED, 0);
    offsetMinutes = prefs.getInt(KEY_OFFSET, 0);
    for (int i = 0; i < MAX_ALARMS; i++) {
      Alarm a = new Alarm(i);
      a.hour = prefs.getInt(key(i, "h"), a.hour);
      a.minute = prefs.getInt(key(i, "m"), a.minute);
      a.enabled = prefs.getBoolean(key(i, "on"), false);
      a.daysMask = prefs.getInt(key(i, "d"), Alarm.ONCE);
      a.label = prefs.getString(key(i, "l"), "");
      alarms[i] = a;
    }
  }

  // ── Alarms ─────────────────────────────────────────────────────────────────

  /** The alarm in slot {@code id}, whether or not the slot is in use. */
  public Alarm get(int id) {
    return alarms[id];
  }

  /** Whether slot {@code id} holds an alarm the user created. */
  public boolean exists(int id) {
    return (used & (1 << id)) != 0;
  }

  /** How many alarms the user has. */
  public int count() {
    int n = 0;
    for (int i = 0; i < MAX_ALARMS; i++) {
      if (exists(i)) {
        n++;
      }
    }
    return n;
  }

  /**
   * The user's alarms, newly packed into an array of exactly {@link #count} entries. The service
   * and the clock screen scan this; every other caller works through {@link #get}.
   */
  public Alarm[] live() {
    Alarm[] out = new Alarm[count()];
    int n = 0;
    for (int i = 0; i < MAX_ALARMS; i++) {
      if (exists(i)) {
        out[n++] = alarms[i];
      }
    }
    return out;
  }

  /** The lowest free slot, or -1 when all {@link #MAX_ALARMS} are taken. */
  public int firstFree() {
    for (int i = 0; i < MAX_ALARMS; i++) {
      if (!exists(i)) {
        return i;
      }
    }
    return -1;
  }

  /**
   * Claim {@code id} and write its alarm through. Used for both a new alarm and an edit of an
   * existing one, so the edit screen has a single save path.
   */
  public void save(int id) {
    Alarm a = alarms[id];
    used |= 1 << id;
    prefs
        .edit()
        .putInt(KEY_USED, used)
        .putInt(key(id, "h"), a.hour)
        .putInt(key(id, "m"), a.minute)
        .putBoolean(key(id, "on"), a.enabled)
        .putInt(key(id, "d"), a.daysMask)
        .putString(key(id, "l"), a.label)
        .apply();
  }

  /** Free {@code id}, and reset the slot so the next new alarm starts from the defaults. */
  public void delete(int id) {
    used &= ~(1 << id);
    alarms[id] = new Alarm(id);
    prefs
        .edit()
        .putInt(KEY_USED, used)
        .remove(key(id, "h"))
        .remove(key(id, "m"))
        .remove(key(id, "on"))
        .remove(key(id, "d"))
        .remove(key(id, "l"))
        .apply();
  }

  /** Arm or disarm {@code id} without touching its time — what the list screen's switch does. */
  public void setEnabled(int id, boolean enabled) {
    alarms[id].enabled = enabled;
    save(id);
  }

  // ── Settings ───────────────────────────────────────────────────────────────

  /** Minutes east of UTC that every screen displays in. */
  public int offsetMinutes() {
    return offsetMinutes;
  }

  public void setOffsetMinutes(int minutes) {
    offsetMinutes = minutes;
    prefs.edit().putInt(KEY_OFFSET, minutes).apply();
  }

  private static String key(int id, String field) {
    return "a" + id + "." + field;
  }
}
