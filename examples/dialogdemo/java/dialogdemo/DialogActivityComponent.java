// SPDX-License-Identifier: GPL-3.0-only
package dialogdemo;

import picodroid.di.ActivitySingletonComponent;

public class DialogActivityComponent extends ActivitySingletonComponent {
  private final DialogAppComponent appComponent;
  private int dialogShowCount = 0;

  public DialogActivityComponent() {
    super();
    this.appComponent = (DialogAppComponent) app();
  }

  public DialogAppComponent appComponent() {
    return appComponent;
  }

  public int incShowCount() {
    return ++dialogShowCount;
  }
}
