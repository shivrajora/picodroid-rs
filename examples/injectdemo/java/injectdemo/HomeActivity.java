// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.TextView;

public class HomeActivity extends BaseActivity {
  @Inject Greeter greeter;

  private boolean pushed;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(
        InjectDemoApp.TAG,
        "Home clock#"
            + clock.id()
            + " same="
            + (clock == greeter.clock())
            + " fresh="
            + (greeter != InjectDemoApp.appGreeter));
    getDisplay();
    TextView text = new TextView();
    text.setText(greeter.greet("home"));
    setContentView(text);
    if (!pushed) {
      pushed = true;
      startActivity(new Intent(DetailActivity.class));
    }
  }
}
