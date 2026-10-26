// SPDX-License-Identifier: GPL-3.0-only
package navdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DetailActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i("NavDemo", "Detail.onCreate");
    // The launching Intent (extras included) is available for our lifetime.
    Log.i("NavDemo", "Detail origin=" + getIntent().getStringExtra("origin"));

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText("Detail Activity");
    title.setTextColor(Color.CYAN);
    root.addView(title);

    Button backBtn = new Button(this);
    backBtn.setText("Back to Home");
    backBtn.setOnClickListener(
        v -> {
          Log.i("NavDemo", "Detail: finish() pressed");
          finish();
        });
    root.addView(backBtn, new LinearLayout.LayoutParams(200, 40));

    setContentView(root);
  }

  @Override
  public void onResume() {
    Log.i("NavDemo", "Detail.onResume");
  }

  @Override
  public void onPause() {
    Log.i("NavDemo", "Detail.onPause");
  }

  @Override
  public void onDestroy() {
    Log.i("NavDemo", "Detail.onDestroy");
  }
}
