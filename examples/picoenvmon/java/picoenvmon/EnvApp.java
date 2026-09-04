// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon;

import javax.inject.Inject;
import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picoenvmon.net.NetworkManager;
import picoenvmon.ui.home.HomeActivity;

/**
 * App entry point. The object graph is wired by {@code @Inject}/{@code @Singleton}
 * (docs/designs/inject-annotations-2026-08.md): the framework injects this Application's fields
 * before {@code onCreate}, and every Activity and Service below gets the same treatment, so the
 * singletons ({@link NetworkManager}, ThresholdConfig, Formatter, LatestReadings, RgbLed) are
 * shared without a hand-written component; SDK types come from {@link EnvModule}.
 */
public class EnvApp extends Application {
  public static final String TAG = "PicoEnvMon";
  public static final String PREFS_NAME = "picoenvmon";

  /** Owns the WiFi join, dashboard server, NTP sync and weather refresh. */
  @Inject NetworkManager networkManager;

  @Override
  public void onCreate() {
    Theme.colorBackground = Color.argb(255, 14, 20, 24);
    Theme.colorSurface = Color.argb(255, 24, 36, 44);
    Theme.colorPrimary = Color.argb(255, 38, 166, 154);
    Theme.colorOnPrimary = Color.WHITE;
    Theme.colorText = Color.argb(255, 240, 240, 240);
    Theme.colorTextSecondary = Color.argb(255, 160, 180, 188);
    Theme.colorOutline = Color.argb(255, 56, 80, 92);

    // No-op on boards without a network link (FEATURE_WIFI / FEATURE_ETHERNET probe inside).
    networkManager.start();

    startActivity(new Intent(HomeActivity.class));
  }
}
