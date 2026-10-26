// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.ViewModel;
import picodroid.util.Log;

/**
 * One alarm being edited: the draft and what it was on entry.
 *
 * <p>Edits land on the {@link Alarm} in the store immediately but are only written through on
 * {@link #save}, so {@link #cancel} has something to restore: the fields are snapshotted by {@link
 * #open} and put back on the way out. The store holds one object per slot for the life of the
 * process, which is what makes that snapshot three ints and a flag rather than a copy.
 */
public final class AlarmEditViewModel extends ViewModel {
  private static final String TAG = ClockApp.TAG;

  private final AlarmStore store;

  /** The repeat days of the draft, as {@link Alarm#daysMask}; the day toggles paint from it. */
  private final MutableLiveData<Integer> days = new MutableLiveData<>();

  private Alarm alarm;
  private boolean existing;
  private int wasHour;
  private int wasMinute;
  private int wasDays;
  private boolean wasEnabled;

  AlarmEditViewModel(AlarmStore store) {
    this.store = store;
  }

  /**
   * Starts editing slot {@code id} and returns its alarm; a second call for the same slot (the
   * screen built again) keeps the draft.
   */
  Alarm open(int id) {
    if (alarm == null || alarm.id != id) {
      alarm = store.get(id);
      existing = store.exists(id);
      wasHour = alarm.hour;
      wasMinute = alarm.minute;
      wasDays = alarm.daysMask;
      wasEnabled = alarm.enabled;
      days.setValue(alarm.daysMask);
    }
    return alarm;
  }

  /**
   * Whether the slot held an alarm before this edit: an edit offers Delete, a new alarm does not.
   */
  boolean existing() {
    return existing;
  }

  LiveData<Integer> days() {
    return days;
  }

  void setTime(int hour, int minute) {
    alarm.hour = hour;
    alarm.minute = minute;
  }

  /** Flips {@code day}, a bit position of {@link Alarm#daysMask}. */
  void toggleDay(int day) {
    alarm.daysMask ^= 1 << day;
    days.setValue(alarm.daysMask);
  }

  void save() {
    // Saving an alarm arms it: an alarm set and left off is almost always a
    // switch the user forgot, not one they meant.
    alarm.enabled = true;
    store.save(alarm.id);
    Log.i(TAG, "saved alarm " + alarm.id + " " + alarm.time() + " " + alarm.repeatText());
  }

  void delete() {
    int id = alarm.id;
    store.delete(id);
    Log.i(TAG, "deleted alarm " + id);
  }

  /** Puts the alarm back as it was on entry. */
  void cancel() {
    alarm.hour = wasHour;
    alarm.minute = wasMinute;
    alarm.daysMask = wasDays;
    alarm.enabled = wasEnabled;
  }
}
