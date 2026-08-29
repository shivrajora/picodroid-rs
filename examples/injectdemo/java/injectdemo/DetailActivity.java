// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import picodroid.util.Log;
import picodroid.widget.TextView;

/**
 * Declares no @Inject members of its own — the generated leaf injector delegates to BaseActivity's.
 */
public class DetailActivity extends BaseActivity {
  @Override
  public void onCreate() {
    Log.i(InjectDemoApp.TAG, "Detail clock#" + clock.id() + " inherited=" + (clock != null));
    getDisplay();
    TextView text = new TextView();
    text.setText("detail clock#" + clock.id());
    setContentView(text);
  }
}
