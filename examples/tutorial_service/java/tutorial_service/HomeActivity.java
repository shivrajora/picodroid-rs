// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Root hub of the back stack. The {@link UptimeLogService} is already running (started by {@link
 * TutorialServiceApp}); this screen just offers a way into the viewer. onCreate runs once and the
 * widget tree is preserved while the viewer sits on top, so watch the logs show onCreate once but
 * onResume on every return.
 */
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  // Views with listeners are held as fields so the GC always sees them rooted through this
  // Activity, not only through the native listener registry — best practice for callback views.
  private Button viewLogButton;

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Tutorial: Service");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView();
    body.setText("A background service is\nlogging uptime samples.");
    body.setTextColor(Color.WHITE);
    root.addView(body);

    viewLogButton = new Button("View Log");
    viewLogButton.setSize(200, 40);
    viewLogButton.setOnClickListener(v -> startActivity(new Intent(LogViewerActivity.class)));
    root.addView(viewLogButton);

    setContentView(root);
  }

  @Override
  public void onResume() {
    Log.i(TAG, "onResume");
  }

  @Override
  public void onPause() {
    Log.i(TAG, "onPause");
  }

  @Override
  public void onDestroy() {
    Log.i(TAG, "onDestroy");
  }

  // This is the root Activity: the default onBackPressed would finish() it, popping the last stack
  // entry and exiting the whole app. Swallow BACK instead (deliberately no super call).
  @Override
  public void onBackPressed() {
    Log.i(TAG, "onBackPressed (ignored at root)");
  }
}
