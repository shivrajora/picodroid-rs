// SPDX-License-Identifier: GPL-3.0-only
package bootdemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.TextView;

/** What the user sees when the app is opened from the launcher, or after a ping. */
public class MainActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Intent intent = getIntent();
    boolean pinged = intent != null && intent.getBooleanExtra("pinged", false);
    TextView text = new TextView(this);
    text.setText(pinged ? "Pinged by the boot alarm" : "Boot demo: see the log");
    setContentView(text);
    Log.i(BootDemoApp.TAG, pinged ? "shown after the ping" : "shown");
  }
}
