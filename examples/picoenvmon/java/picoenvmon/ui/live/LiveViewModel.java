// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.live;

import javax.inject.Inject;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.Transformations;
import picodroid.lifecycle.ViewModel;
import picodroid.lifecycle.ViewModelProvider;
import picoenvmon.data.LatestReadings;
import picoenvmon.data.Readings;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.util.Formatter;

/**
 * What the Live screen shows: the repository's 1 Hz readings turned into tile text and breach flags
 * in the user's units, so the Activity only paints. Made by its {@link Factory}, which the object
 * graph injects into the Activity with the singletons it needs.
 */
public final class LiveViewModel extends ViewModel {

  /** One frame of the five tiles, indexed like {@link LatestReadings#IDX_TEMPERATURE}. */
  public static final class Tiles {
    /** Display text per tile; null while that sensor has not reported. */
    public final String[] values = new String[LatestReadings.COUNT];

    /** Whether the tile's reading is past its alert threshold. */
    public final boolean[] breached = new boolean[LatestReadings.COUNT];
  }

  private final LiveData<Tiles> tiles;
  private final Formatter formatter;
  private final ThresholdConfig thresholds;

  LiveViewModel(LatestReadings readings, Formatter formatter, ThresholdConfig thresholds) {
    this.formatter = formatter;
    this.thresholds = thresholds;
    tiles = Transformations.map(readings.readings(), this::toTiles);
  }

  public LiveData<Tiles> tiles() {
    return tiles;
  }

  private Tiles toTiles(Readings r) {
    Tiles t = new Tiles();
    Formatter f = formatter;
    if (r.isValid(LatestReadings.IDX_TEMPERATURE)) {
      float v = r.get(LatestReadings.IDX_TEMPERATURE);
      t.values[LatestReadings.IDX_TEMPERATURE] = f.formatTemp(v);
      t.breached[LatestReadings.IDX_TEMPERATURE] = thresholds.tempBreached(v);
    }
    if (r.isValid(LatestReadings.IDX_HUMIDITY)) {
      float v = r.get(LatestReadings.IDX_HUMIDITY);
      t.values[LatestReadings.IDX_HUMIDITY] = f.formatHumidity(v);
      t.breached[LatestReadings.IDX_HUMIDITY] = thresholds.humidityBreached(v);
    }
    if (r.isValid(LatestReadings.IDX_PRESSURE)) {
      t.values[LatestReadings.IDX_PRESSURE] = f.formatPressure(r.get(LatestReadings.IDX_PRESSURE));
    }
    if (r.isValid(LatestReadings.IDX_GAS)) {
      t.values[LatestReadings.IDX_GAS] = f.formatGasIaq(r.get(LatestReadings.IDX_GAS));
    }
    if (r.isValid(LatestReadings.IDX_LIGHT)) {
      float v = r.get(LatestReadings.IDX_LIGHT);
      t.values[LatestReadings.IDX_LIGHT] = f.formatLux(v);
      t.breached[LatestReadings.IDX_LIGHT] = thresholds.luxBreached(v);
    }
    return t;
  }

  /** Makes the {@link LiveViewModel} with the app's singletons. */
  public static final class Factory implements ViewModelProvider.Factory {
    private final LatestReadings readings;
    private final Formatter formatter;
    private final ThresholdConfig thresholds;

    @Inject
    public Factory(LatestReadings readings, Formatter formatter, ThresholdConfig thresholds) {
      this.readings = readings;
      this.formatter = formatter;
      this.thresholds = thresholds;
    }

    @Override
    @SuppressWarnings("unchecked")
    public <T extends ViewModel> T create(Class<T> modelClass) {
      if (modelClass != LiveViewModel.class) {
        throw new IllegalArgumentException("not a LiveViewModel: " + modelClass.getName());
      }
      return (T) new LiveViewModel(readings, formatter, thresholds);
    }
  }
}
