package navdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DetailActivity extends Activity {
  public void onResume() {
    Log.i("NavDemo", "Detail.onResume");

    // Display has already been brought up by HomeActivity, but this idiom
    // is harmless (idempotent) and self-contained — copy when forking
    // this demo as a starting point for new screens.
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
    // Calling finish() on this Activity pops it off the stack, triggering
    // onPause/onStop/onDestroy here, then onStart/onResume on HomeActivity
    // (which rebuilds its UI in onResume).
    backBtn.setOnClickListener(
        () -> {
          Log.i("NavDemo", "Detail: finish() pressed");
          finish();
        });
    root.addView(backBtn);

    setContentView(root);
  }

  public void onPause() {
    Log.i("NavDemo", "Detail.onPause");
  }

  public void onDestroy() {
    Log.i("NavDemo", "Detail.onDestroy");
  }
}
