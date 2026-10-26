// SPDX-License-Identifier: GPL-3.0-only
package tutorial_screens;

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
 * LAUNCHER intent filter, so the launcher starts it. onCreate runs exactly once: while Counter or
 * About sits on top, this Activity is paused but its widget tree is preserved, and it is restored
 * as-is when the top screen finishes — watch the logs show onCreate once but onResume on every
 * return.
 */
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    // The content view fills the window; no size needed.
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.home_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    // Views are locals, as on Android: the content view keeps them, listeners included, for as
    // long as this Activity lives. A view becomes a field only when later code needs it.
    Button counterButton = new Button(this);
    counterButton.setText(getString(R.string.button_counter));
    counterButton.setOnClickListener(v -> startActivity(new Intent(this, CounterActivity.class)));
    root.addView(counterButton, buttonParams());

    Button aboutButton = new Button(this);
    aboutButton.setText(getString(R.string.button_about));
    aboutButton.setOnClickListener(v -> startActivity(new Intent(this, AboutActivity.class)));
    root.addView(aboutButton, buttonParams());

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
