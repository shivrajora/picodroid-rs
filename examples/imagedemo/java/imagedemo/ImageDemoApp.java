// SPDX-License-Identifier: GPL-3.0-only
package imagedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ImageDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(ImageDemoActivity.class));
  }
}
