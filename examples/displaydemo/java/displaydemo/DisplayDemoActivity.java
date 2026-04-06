package displaydemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class DisplayDemoActivity extends Activity {
  public void onCreate() {
    getDisplay().calibrate();
    Log.i("DisplayDemo", "Display ready");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);

    TextView title = new TextView();
    title.setText("Picodroid UI Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button btn = new Button("Tap Me!");
    btn.setSize(200, 50);
    btn.setOnClickListener(new TapHandler(title));
    root.addView(btn);

    setContentView(root);
  }
}
