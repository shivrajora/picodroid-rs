package picoenvmon.di;

import picodroid.di.ActivitySingletonComponent;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.hardware.RgbLed;
import picoenvmon.util.Formatter;

public class EnvActivityComponent extends ActivitySingletonComponent {
  private final EnvAppComponent appComponent;

  public EnvActivityComponent() {
    super();
    this.appComponent = (EnvAppComponent) app();
  }

  public EnvAppComponent appComponent() {
    return appComponent;
  }

  public ThresholdConfig thresholds() {
    return appComponent.thresholds();
  }

  public Formatter formatter() {
    return appComponent.formatter();
  }

  public RgbLed rgbLed() {
    return appComponent.rgbLed();
  }
}
