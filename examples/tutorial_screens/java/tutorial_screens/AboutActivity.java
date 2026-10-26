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
 * Static screen with an explicit Back button. Calling finish() is exactly what the BACK key does by
 * default (Activity.onBackPressed → finish), so both routes pop this screen back to Home.
 */
public class AboutActivity extends Activity {
  private static final String TAG = "AboutActivity";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "onCreate");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText(getString(R.string.about_title));
    title.setTextColor(Color.WHITE);
    root.addView(title);

    TextView body = new TextView(this);
    body.setText(getString(R.string.about_body));
    body.setTextColor(Color.WHITE);
    root.addView(body);

    // A local, as on Android: the content view keeps the button and its listener alive.
    Button backButton = new Button(this);
    backButton.setText(getString(R.string.button_back));
    backButton.setOnClickListener(v -> finish());
    root.addView(backButton, HomeActivity.buttonParams());

    setContentView(root);
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
