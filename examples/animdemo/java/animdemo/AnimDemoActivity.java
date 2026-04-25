package animdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class AnimDemoActivity extends Activity {
  private TextView tile;
  private boolean tileMoved = false;
  private boolean tileFaded = false;

  public void onCreate() {
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Animation Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    // The animated tile. Position is absolute (set via animate().x/y), so
    // we don't add it to the LinearLayout — flex flow would override our
    // x/y on every layout pass.
    tile = new TextView();
    tile.setText("hello");
    tile.setSize(60, 30);
    tile.setBackgroundColor(Color.argb(255, 80, 120, 200));
    tile.setTextColor(Color.WHITE);
    tile.setPosition(20, 120);
    root.addView(tile);

    Button fadeBtn = new Button("Fade toggle");
    fadeBtn.setSize(200, 36);
    fadeBtn.setOnClickListener(
        () -> {
          float from = tileFaded ? 0.0f : 1.0f;
          float to = tileFaded ? 1.0f : 0.0f;
          tileFaded = !tileFaded;
          Log.i("AnimDemo", "fade " + from + " -> " + to);
          tile.animate().alpha(from, to).setDuration(400).start();
        });
    root.addView(fadeBtn);

    Button slideBtn = new Button("Slide");
    slideBtn.setSize(200, 36);
    slideBtn.setOnClickListener(
        () -> {
          int from = tileMoved ? 160 : 20;
          int to = tileMoved ? 20 : 160;
          tileMoved = !tileMoved;
          Log.i("AnimDemo", "slide " + from + " -> " + to);
          tile.animate().x(from, to).setDuration(300).start();
        });
    root.addView(slideBtn);

    Button restoreBtn = new Button("Restore");
    restoreBtn.setSize(200, 36);
    restoreBtn.setOnClickListener(
        () -> {
          Log.i("AnimDemo", "restore");
          tile.setAlpha(1.0f);
          tile.setPosition(20, 120);
          tileMoved = false;
          tileFaded = false;
        });
    root.addView(restoreBtn);

    setContentView(root);
  }
}
