// SPDX-License-Identifier: GPL-3.0-only
package navdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DetailActivity extends Activity {
  @Override
  public void onCreate() {
    Log.i("NavDemo", "Detail.onCreate");

    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Detail Activity");
    title.setTextColor(Color.CYAN);
    root.addView(title);

    Button backBtn = new Button("Back to Home");
    backBtn.setSize(200, 40);
    backBtn.setOnClickListener(
        v -> {
          Log.i("NavDemo", "Detail: finish() pressed");
          finish();
        });
    root.addView(backBtn);

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
