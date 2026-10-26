// SPDX-License-Identifier: GPL-3.0-only
package tutorial_screens;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * State lives in the Activity object: {@code count} is a plain instance field and the label showing
 * it is a plain View field. BACK uses the inherited onBackPressed → finish(), which pops this
 * Activity off the stack and destroys it — so a later visit starts from a fresh instance at 0.
 */
public class CounterActivity extends Activity {
  private static final String TAG = "CounterActivity";

  private int count = 0;

  // countLabel is a field because the click handler updates it; the button is a local — the
  // tree keeps it and its listener alive, as on Android.
  private TextView countLabel;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.counter_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    countLabel = new TextView(this);
    countLabel.setText(getString(R.string.count, count));
    countLabel.setTextColor(Color.CYAN);
    root.addView(countLabel);

    Button incrementButton = new Button(this);
    incrementButton.setText(getString(R.string.button_increment));
    incrementButton.setOnClickListener(
        v -> {
          count++;
          Log.i(TAG, "count=" + count);
          countLabel.setText(getString(R.string.count, count));
        });
    root.addView(incrementButton, HomeActivity.buttonParams());

    setContentView(root);
    // No Back button here: the BACK key's default onBackPressed() calls finish() for us.
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
}
