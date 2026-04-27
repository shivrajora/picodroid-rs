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

  public void onCreate() {
    Log.i("NavDemo", "Home.onCreate");
    buildUi();
  }

  public void onResume() {
    Log.i("NavDemo", "Home.onResume");
  }

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
        () -> {
          Log.i("NavDemo", "Home: launching Detail");
          startActivity(new Intent(DetailActivity.class));
        });
    root.addView(openBtn);

    setContentView(root);
  }
}
