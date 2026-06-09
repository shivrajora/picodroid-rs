// SPDX-License-Identifier: GPL-3.0-only
package animdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class AnimDemoActivity extends Activity {
  private TextView tile;
  private boolean tileMoved = false;
  private boolean tileFaded = false;

  @Override
  public void onCreate() {
    getDisplay();

    // FrameLayout root so the animated tile can live at an absolute
    // position. A vertical LinearLayout would re-layout the tile on every
    // pass and clobber x/y set via animate() — children of a flex
    // container don't honor setPosition.
    FrameLayout root = new FrameLayout();
    root.setSize(240, 240);

    // Controls column anchored to the top half — title + 3 buttons fit in
    // ~140 px, leaving the lower strip free for the tile to slide across.
    LinearLayout controls = new LinearLayout();
    controls.setOrientation(LinearLayout.VERTICAL);
    controls.setSize(240, 150);
    controls.setPosition(0, 0);
    controls.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Animation Demo");
    title.setTextColor(Color.WHITE);
    controls.addView(title);

    Button fadeBtn = new Button("Fade toggle");
    fadeBtn.setSize(200, 30);
    fadeBtn.setOnClickListener(
        v -> {
          float from = tileFaded ? 0.0f : 1.0f;
          float to = tileFaded ? 1.0f : 0.0f;
          tileFaded = !tileFaded;
          Log.i("AnimDemo", "fade " + from + " -> " + to);
          tile.animate().alpha(from, to).setDuration(400).start();
        });
    controls.addView(fadeBtn);

    Button slideBtn = new Button("Slide");
    slideBtn.setSize(200, 30);
    slideBtn.setOnClickListener(
        v -> {
          int from = tileMoved ? 160 : 20;
          int to = tileMoved ? 20 : 160;
          tileMoved = !tileMoved;
          Log.i("AnimDemo", "slide " + from + " -> " + to);
          tile.animate().x(from, to).setDuration(300).start();
        });
    controls.addView(slideBtn);

    Button restoreBtn = new Button("Restore");
    restoreBtn.setSize(200, 30);
    restoreBtn.setOnClickListener(
        v -> {
          Log.i("AnimDemo", "restore");
          tile.setAlpha(1.0f);
          tile.setPosition(20, 180);
          tileMoved = false;
          tileFaded = false;
        });
    controls.addView(restoreBtn);

    root.addView(controls);

    // The animated tile — sibling of the controls inside the FrameLayout,
    // positioned absolutely. setPosition + animate().x/y both work because
    // FrameLayout is a plain lv_obj with no flex flow.
    tile = new TextView();
    tile.setText("hello");
    tile.setSize(60, 30);
    tile.setBackgroundColor(Color.argb(255, 80, 120, 200));
    tile.setTextColor(Color.WHITE);
    tile.setPosition(20, 180);
    root.addView(tile);

    setContentView(root);
  }
}
