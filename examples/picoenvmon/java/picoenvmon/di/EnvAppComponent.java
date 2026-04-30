package picoenvmon.di;

import picodroid.content.Preferences;
import picodroid.di.ApplicationComponent;
import picodroid.util.Log;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.hardware.RgbLed;
import picoenvmon.util.Formatter;

public class EnvAppComponent extends ApplicationComponent {
  public static final String TAG = "PicoEnvMon";
  public static final String PREFS_NAME = "picoenvmon";

  private final Preferences prefs;
  private final ThresholdConfig thresholds;
  private final Formatter formatter;
  private RgbLed rgbLed;

  public EnvAppComponent() {
    super();
    this.prefs = Preferences.open(PREFS_NAME);
    this.thresholds = new ThresholdConfig();
    this.thresholds.load(prefs);
    this.formatter = new Formatter();
    Log.i(
        TAG,
        "app component up; thresholds tempHi="
            + thresholds.tempHiCentiC
            + " humLo="
            + thresholds.humLoMilliPct
            + " luxLo="
            + thresholds.luxLo);
  }

  public Preferences prefs() {
    return prefs;
  }

  public ThresholdConfig thresholds() {
    return thresholds;
  }

  public Formatter formatter() {
    return formatter;
  }

  /** RGB LED — owned at app scope so the Service and Activity see the same instance. */
  public RgbLed rgbLed() {
    if (rgbLed == null) {
      rgbLed = new RgbLed();
    }
    return rgbLed;
  }
}
