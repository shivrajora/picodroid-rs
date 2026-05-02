// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.ui.home.HomeActivity;

public class EnvApp extends Application {
  public void onCreate() {
    Theme.colorBackground = Color.argb(255, 14, 20, 24);
    Theme.colorSurface = Color.argb(255, 24, 36, 44);
    Theme.colorPrimary = Color.argb(255, 38, 166, 154);
    Theme.colorOnPrimary = Color.WHITE;
    Theme.colorText = Color.argb(255, 240, 240, 240);
    Theme.colorTextSecondary = Color.argb(255, 160, 180, 188);
    Theme.colorOutline = Color.argb(255, 56, 80, 92);

    new EnvAppComponent();

    startActivity(new Intent(HomeActivity.class));
  }
}
