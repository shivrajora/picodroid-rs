// SPDX-License-Identifier: GPL-3.0-only
package resdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ResDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(ResDemoActivity.class));
  }
}
