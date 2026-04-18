package keydemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class KeyDemoActivity extends Activity implements OnKeyListener {
  private static final String TAG = "KeyDemo";
  private TextView status;

  public void onCreate() {
    // Force display init before constructing any widgets.
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Hardware Key Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    status = new TextView();
    status.setText("Press any button");
    status.setTextColor(Color.CYAN);
    root.addView(status);

    // A focusable widget is required so LVGL's default keypad group has a
    // target — without one, lv_group_get_focused() returns null and no key
    // events reach Java.
    Button focus = new Button("Focus me");
    focus.setSize(200, 50);
    focus.setOnKeyListener(this);
    root.addView(focus);

    setContentView(root);
  }

  public boolean onKey(View v, KeyEvent event) {
    String action = event.getAction() == KeyEvent.ACTION_DOWN ? "DOWN" : "UP";
    status.setText(action + " keyCode=" + event.getKeyCode());
    Log.i(TAG, action + " keyCode=" + event.getKeyCode());
    return true;
  }
}
