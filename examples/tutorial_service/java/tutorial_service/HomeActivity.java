// SPDX-License-Identifier: GPL-3.0-only
package tutorial_service;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Root hub of the back stack, and the app's entry: PicodroidManifest.xml gives it the MAIN /
 * LAUNCHER intent filter, so the launcher starts it.
 *
 * <p>It <em>starts</em> {@link UptimeLogService} before anything else. A started Service runs until
 * it is stopped, independent of any one screen, so it keeps sampling on its background Thread while
 * the user navigates and {@link LogViewerActivity} always finds an accumulated ring buffer to read.
 * onCreate runs once and the widget tree is preserved while the viewer sits on top, so watch the
 * logs show onCreate once but onResume on every return.
 */
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");
    Log.i(TAG, "starting UptimeLogService");
    startService(new Intent(this, UptimeLogService.class));

    // The content view fills the window; no size needed.
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.home_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView(this);
    body.setText(getString(R.string.home_body));
    body.setTextColor(Color.WHITE);
    root.addView(body);

    // A local, as on Android: the content view keeps the button and its listener alive.
    Button viewLogButton = new Button(this);
    viewLogButton.setText(getString(R.string.button_view_log));
    viewLogButton.setOnClickListener(v -> startActivity(new Intent(this, LogViewerActivity.class)));
    root.addView(viewLogButton, buttonParams());

    setContentView(root);
  }

  /** A full-width button, a little below the view above it. */
  static LinearLayout.LayoutParams buttonParams() {
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    lp.topMargin = 8;
    return lp;
  }

  @Override
  public void onResume() {
    super.onResume();
    Log.i(TAG, "onResume");
  }

  @Override
  public void onPause() {
    super.onPause();
    Log.i(TAG, "onPause");
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
    Log.i(TAG, "onDestroy");
  }

  // This is the root Activity, so no onBackPressed override: BACK runs the inherited finish(),
  // popping the last stack entry, which ends the app and returns to the launcher, as on Android.
}
