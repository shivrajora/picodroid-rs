// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon;

import javax.inject.Inject;
import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.graphics.Theme;
import picoenvmon.data.ThresholdConfig;
import picoenvmon.net.NetworkManager;
import picoenvmon.ui.home.HomeActivity;

/**
 * App entry point. The object graph is wired by {@code @Inject}/{@code @Singleton}
 * (docs/designs/inject-annotations-2026-08.md): the framework injects this Application's fields
 * before {@code onCreate}, and every Activity and Service below gets the same treatment, so the
 * singletons ({@link NetworkManager}, ThresholdConfig, Formatter, LatestReadings, RgbLed) are
 * shared without a hand-written component. What needs a Context gets this one, the application
 * context, as an argument.
 */
public class EnvApp extends Application {
  public static final String TAG = "PicoEnvMon";
  public static final String PREFS_NAME = "picoenvmon";

  /** Owns the network callback, dashboard server, clock watch and weather refresh. */
  @Inject NetworkManager networkManager;

  @Inject ThresholdConfig thresholds;

  @Override
  public void onCreate() {
    Theme.colorBackground = getColor(R.color.background);
    Theme.colorSurface = getColor(R.color.surface);
    Theme.colorPrimary = getColor(R.color.primary);
    Theme.colorOnPrimary = getColor(R.color.on_primary);
    Theme.colorText = getColor(R.color.text);
    Theme.colorTextSecondary = getColor(R.color.text_secondary);
    Theme.colorOutline = getColor(R.color.outline);

    // Before any screen or the logging service reads them.
    thresholds.load(getSharedPreferences(PREFS_NAME, MODE_PRIVATE));

    // No-op on boards without a network link (FEATURE_WIFI / FEATURE_ETHERNET probe inside).
    networkManager.start(this);

    startActivity(new Intent(this, HomeActivity.class));
  }
}
