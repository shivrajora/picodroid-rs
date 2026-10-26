// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.history;

import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.ViewModel;
import picoenvmon.service.SensorLoggerService;

/**
 * What the History screen shows: one snapshot of the logging service's temperature ring, taken when
 * the screen binds the service. The Activity renders it; the info dialog reads a sample back from
 * the same snapshot, so the row and the dialog always agree.
 */
public final class HistoryViewModel extends ViewModel {

  /** The ring, oldest sample first. */
  public static final class History {
    public final float[] samples = new float[SensorLoggerService.RING_CAPACITY];

    /** Epoch seconds per sample; 0 = taken before the wall clock was set. */
    public final int[] timestamps = new int[SensorLoggerService.RING_CAPACITY];

    public int count;
  }

  private final MutableLiveData<History> history = new MutableLiveData<>();

  public HistoryViewModel() {}

  /** Null until {@link #load} has run. */
  public LiveData<History> history() {
    return history;
  }

  /** Take a fresh snapshot of {@code service}'s temperature ring and publish it. */
  void load(SensorLoggerService service) {
    History h = new History();
    h.count = service.snapshot(SensorLoggerService.IDX_TEMPERATURE, h.samples, h.timestamps);
    history.setValue(h);
  }
}
