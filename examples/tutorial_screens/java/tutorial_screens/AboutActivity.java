// SPDX-License-Identifier: GPL-3.0-only
package tutorial_screens;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Static screen with an explicit Back button. Calling finish() is exactly what the BACK key does by
 * default (Activity.onBackPressed → finish), so both routes pop this screen back to Home.
 */
public class AboutActivity extends Activity {
  private static final String TAG = "AboutActivity";

  // Field-held so the GC roots the listener-bearing button through this Activity.
  private Button backButton;

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("About");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView();
    body.setText("Back-stack tutorial app.\nEach screen is an Activity.");
    body.setTextColor(Color.WHITE);
    root.addView(body);

    backButton = new Button("Back");
    backButton.setSize(200, 40);
    backButton.setOnClickListener(v -> finish());
    root.addView(backButton);

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
}
