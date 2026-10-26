// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.Alarm;
import picoclock.AlarmStore;
import picoclock.ClockApp;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.ViewModel;
import picodroid.util.Log;

/** The alarm list: the store's alarms as they change, and the list's one edit, the switch. */
public final class AlarmListViewModel extends ViewModel {
  private static final String TAG = ClockApp.TAG;

  private final AlarmStore store;

  AlarmListViewModel(AlarmStore store) {
    this.store = store;
  }

  /** Every alarm, republished after any screen changes one. */
  LiveData<Alarm[]> alarms() {
    return store.alarms();
  }

  /** Arm or disarm {@code alarm} without opening it. */
  void setEnabled(Alarm alarm, boolean enabled) {
    store.setEnabled(alarm.id, enabled);
    Log.i(TAG, "alarm " + alarm.id + " " + alarm.time() + (enabled ? " on" : " off"));
  }

  /** The slot a new alarm goes in, or -1 when all {@link AlarmStore#MAX_ALARMS} are taken. */
  int firstFree() {
    return store.firstFree();
  }
}
