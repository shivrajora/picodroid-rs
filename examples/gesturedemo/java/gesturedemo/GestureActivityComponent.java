// SPDX-License-Identifier: GPL-3.0-only
package gesturedemo;

import picodroid.di.ActivitySingletonComponent;

public class GestureActivityComponent extends ActivitySingletonComponent {
  private final GestureAppComponent appComponent;

  public GestureActivityComponent() {
    super();
    this.appComponent = (GestureAppComponent) app();
  }

  public GestureAppComponent appComponent() {
    return appComponent;
  }
}
