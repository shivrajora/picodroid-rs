package displaydemo;

import picodroid.graphics.Color;
import picodroid.graphics.Display;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.ProgressBar;
import picodroid.widget.Switch;
import picodroid.widget.TextView;

public class DisplayDemo {
  public static void main() {
    Display display = Display.getInstance();
    display.calibrate();
    Log.i("DisplayDemo", "Display ready");

    // Build the UI tree
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);

    TextView title = new TextView();
    title.setText("Picodroid UI Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button btn = new Button("Tap Me!");
    btn.setSize(200, 50);
    root.addView(btn);

    ProgressBar bar = new ProgressBar();
    bar.setSize(200, 20);
    root.addView(bar);

    Switch sw = new Switch();
    root.addView(sw);

    TextView status = new TextView();
    status.setText("Ready");
    root.addView(status);

    display.setContentView(root);

    // Main loop
    int taps = 0;
    int progress = 0;
    int frames = 0;

    while (true) {
      display.update();

      if (btn.wasClicked()) {
        taps = taps + 1;
        status.setText("Taps: " + taps);
      }

      progress = (progress + 1) % 101;
      bar.setProgress(progress);

      if (sw.isChecked()) {
        title.setTextColor(Color.GREEN);
      } else {
        title.setTextColor(Color.WHITE);
      }

      SystemClock.sleep(16);
      frames = frames + 1;
    }
  }
}
