// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;

public class DisplayDemoApp extends Application {
  public void onCreate() {
    // Custom dark palette — Activities reference Theme.color* directly so
    // tweaking these here changes the whole app at once. Demonstrates the
    // Theme.color* static-mutation pattern alongside the widget sampler.
    Theme.colorBackground = Color.argb(255, 14, 18, 28);
    Theme.colorSurface = Color.argb(255, 30, 38, 56);
    Theme.colorPrimary = Color.argb(255, 90, 170, 240);
    Theme.colorOnPrimary = Color.argb(255, 10, 16, 28);
    Theme.colorText = Color.argb(255, 230, 235, 245);
    Theme.colorTextSecondary = Color.argb(255, 150, 160, 180);
    Theme.colorOutline = Color.argb(255, 80, 96, 130);

    startActivity(new Intent(DisplayDemoActivity.class));
  }
}
