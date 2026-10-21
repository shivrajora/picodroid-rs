// SPDX-License-Identifier: GPL-3.0-only
package qa_list;

import picodroid.app.Application;
import picodroid.content.Intent;

public class QaListApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(QaListActivity.class));
  }
}
