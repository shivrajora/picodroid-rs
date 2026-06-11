// SPDX-License-Identifier: GPL-3.0-only
package tutorial_screens;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Root hub of the back stack. onCreate runs exactly once: while Counter or About sits on top, this
 * Activity is paused but its widget tree is preserved, and it is restored as-is when the top screen
 * finishes — watch the logs show onCreate once but onResume on every return.
 */
public class HomeActivity extends Activity {
  private static final String TAG = "HomeActivity";

  // Views with listeners are held as fields so the GC always sees them rooted through this
  // Activity, not only through the native listener registry — best practice for callback views.
  private Button counterButton;
  private Button aboutButton;

  @Override
  public void onCreate() {
    Log.i(TAG, "onCreate");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Tutorial: Screens");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    counterButton = new Button("Counter");
    counterButton.setSize(200, 40);
    counterButton.setOnClickListener(v -> startActivity(new Intent(CounterActivity.class)));
    root.addView(counterButton);

    aboutButton = new Button("About");
    aboutButton.setSize(200, 40);
    aboutButton.setOnClickListener(v -> startActivity(new Intent(AboutActivity.class)));
    root.addView(aboutButton);

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
