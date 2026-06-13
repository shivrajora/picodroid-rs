// SPDX-License-Identifier: GPL-3.0-only
package navdemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class HomeActivity extends Activity {
  private int buildCount = 0;
  private boolean probeLaunched = false;

  @Override
  public void onCreate() {
    Log.i("NavDemo", "Home.onCreate");
    // Launched by NavDemoApp via an explicit Intent, so getIntent() is non-null
    // (it is null only for a manifest `activity=` boot with no app-side launch).
    Log.i("NavDemo", "Home intent null=" + (getIntent() == null));
    buildUi();
    // One-shot for-result round-trip: launch a probe that immediately reports
    // RESULT_OK + finishes; the result lands in onActivityResult below.
    if (!probeLaunched) {
      probeLaunched = true;
      startActivityForResult(new Intent(ResultProbeActivity.class), 7);
    }
  }

  @Override
  public void onRestart() {
    Log.i("NavDemo", "Home.onRestart");
  }

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    // Delivered AFTER restore but BEFORE onRestart (Android order — the
    // onRestart log lands after this one).
    int answer = data == null ? -1 : data.getIntExtra("answer", -1);
    Log.i(
        "NavDemo",
        "Home.onActivityResult req=" + requestCode + " code=" + resultCode + " answer=" + answer);
  }

  @Override
  public void onResume() {
    Log.i("NavDemo", "Home.onResume");
  }

  @Override
  public void onPause() {
    Log.i("NavDemo", "Home.onPause");
  }

  private void buildUi() {
    buildCount++;
    Log.i("NavDemo", "Home.buildUi count=" + buildCount);

    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Home Activity");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button openBtn = new Button("Open Detail");
    openBtn.setSize(200, 40);
    openBtn.setOnClickListener(
        v -> {
          Log.i("NavDemo", "Home: launching Detail");
          startActivity(new Intent(DetailActivity.class).putExtra("origin", "home"));
        });
    root.addView(openBtn);

    setContentView(root);
  }
}
