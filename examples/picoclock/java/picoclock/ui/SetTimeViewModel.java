// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.AlarmStore;
import picoclock.Clock;
import picoclock.ClockApp;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.ViewModel;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * What the Set-time screen's fields read, and setting the clock from them.
 *
 * <p>{@link SystemClock#setCurrentTimeMillis} takes UTC, so the fields are a local time plus the
 * zone it is local to: change the zone and the local time moves with it, keeping the same instant.
 */
public final class SetTimeViewModel extends ViewModel {
  private static final String TAG = ClockApp.TAG;

  /**
   * Where the fields start when the clock has never been set. Not "now" — there is no now to read —
   * and not 1970 either, which {@link Clock#isSet} would treat as still unset and which is a long
   * way from anywhere the user wants to be. A round recent date is the shortest walk to a real one.
   */
  private static final int DEFAULT_YEAR = 2026;

  /**
   * The offsets the zone stepper walks, in minutes: whole hours, plus the fractional ones in use.
   */
  private static final int[] OFFSETS = {
    -720, -660, -600, -540, -480, -420, -360, -300, -240, -210, -180, -120, -60, 0, 60, 120, 180,
    210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 540, 570, 600, 630, 660, 720, 780, 840
  };

  /** The fields: a local wall time to the minute and the zone it is in. Immutable. */
  static final class Fields {
    final long localMs;
    final int offsetMinutes;
    private final int[] ymd;

    Fields(long localMs, int offsetMinutes) {
      this.localMs = localMs;
      this.offsetMinutes = offsetMinutes;
      this.ymd = Clock.civilFromDay(Clock.dayOf(localMs));
    }

    int year() {
      return ymd[0];
    }

    /** 1-based, January is 1. */
    int month() {
      return ymd[1];
    }

    int day() {
      return ymd[2];
    }

    int hour() {
      return Clock.hourOf(localMs);
    }

    int minute() {
      return Clock.minuteOf(localMs);
    }
  }

  private final AlarmStore store;
  private final MutableLiveData<Fields> fields = new MutableLiveData<>();

  SetTimeViewModel(AlarmStore store) {
    this.store = store;
    int offset = store.offsetMinutes();
    long utc = System.currentTimeMillis();
    long local;
    if (Clock.isSet(utc)) {
      long now = Clock.toLocal(utc, offset);
      local = now - Clock.floorMod(now, Clock.MS_PER_MINUTE);
    } else {
      local = Clock.dayFromCivil(DEFAULT_YEAR, 1, 1) * Clock.MS_PER_DAY;
    }
    fields.setValue(new Fields(local, offset));
  }

  LiveData<Fields> fields() {
    return fields;
  }

  void stepYear(int direction) {
    Fields f = fields.getValue();
    pickDate(f.year() + direction, f.month(), f.day());
  }

  void stepMonth(int direction) {
    Fields f = fields.getValue();
    int year = f.year();
    int month = f.month() + direction;
    if (month > 12) {
      month = 1;
      year++;
    } else if (month < 1) {
      month = 12;
      year--;
    }
    pickDate(year, month, f.day());
  }

  /** A date, the month 1-based; a day past the month's end lands on its last day. */
  void pickDate(int year, int month, int day) {
    Fields f = fields.getValue();
    int last = Clock.daysInMonth(year, month);
    long epochDay = Clock.dayFromCivil(year, month, day > last ? last : day);
    set(epochDay * Clock.MS_PER_DAY + Clock.msIntoDay(f.localMs), f.offsetMinutes);
  }

  void pickTime(int hour, int minute) {
    Fields f = fields.getValue();
    set(
        Clock.startOfDay(f.localMs) + hour * Clock.MS_PER_HOUR + minute * Clock.MS_PER_MINUTE,
        f.offsetMinutes);
  }

  /**
   * One step along {@link #OFFSETS}, carrying the local time with it so the fields stay the same
   * instant while the zone's label changes.
   */
  void stepOffset(int direction) {
    Fields f = fields.getValue();
    int i = indexOfOffset(f.offsetMinutes) + direction;
    if (i < 0 || i >= OFFSETS.length) {
      return;
    }
    int offset = OFFSETS[i];
    set(f.localMs + (offset - f.offsetMinutes) * Clock.MS_PER_MINUTE, offset);
  }

  /** Sets the platform clock and zone from the fields; false if the platform refused. */
  boolean apply() {
    Fields f = fields.getValue();
    store.setOffsetMinutes(f.offsetMinutes);
    if (!SystemClock.setCurrentTimeMillis(Clock.toUtc(f.localMs, f.offsetMinutes))) {
      Log.w(TAG, "the platform refused the clock set");
      return false;
    }
    // Every armed alarm was scheduled against the old clock; the service hears
    // the clock set as ACTION_TIME_CHANGED and re-plans.
    Log.i(
        TAG,
        "clock set to "
            + Clock.date(f.localMs)
            + " "
            + Clock.hm(f.hour(), f.minute())
            + " "
            + Clock.offset(f.offsetMinutes));
    return true;
  }

  private void set(long localMs, int offsetMinutes) {
    Fields f = fields.getValue();
    if (f.localMs != localMs || f.offsetMinutes != offsetMinutes) {
      fields.setValue(new Fields(localMs, offsetMinutes));
    }
  }

  private static int indexOfOffset(int minutes) {
    for (int i = 0; i < OFFSETS.length; i++) {
      if (OFFSETS[i] == minutes) {
        return i;
      }
    }
    return 0;
  }
}
