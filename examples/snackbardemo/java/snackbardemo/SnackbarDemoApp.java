// SPDX-License-Identifier: GPL-3.0-only
package snackbardemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SnackbarDemoApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(SnackbarDemoActivity.class));
  }
}
