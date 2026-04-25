package navdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class HomeActivity extends Activity {
  /**
   * v1 caveat: Activity content views are NOT preserved across pause. When DetailActivity is pushed
   * on top, this Activity's view tree is freed by the new top's setContentView. So we build the UI
   * in onResume (called once on first launch and again whenever this Activity returns to the
   * foreground), not in onCreate.
   */
  public void onResume() {
    Log.i("NavDemo", "Home.onResume");
    rebuildUi();
  }

  public void onPause() {
    Log.i("NavDemo", "Home.onPause");
  }

  private void rebuildUi() {
    // Force Display init before constructing widgets (idempotent on resubsequent
    // resumes). Same idiom used by KeyDemoActivity / DialogDemoActivity.
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
        () -> {
          Log.i("NavDemo", "Home: launching Detail");
          startActivity(new DetailActivity());
        });
    root.addView(openBtn);

    setContentView(root);
  }
}
