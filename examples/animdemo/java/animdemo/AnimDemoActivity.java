// SPDX-License-Identifier: GPL-3.0-only
package animdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.animation.AccelerateDecelerateInterpolator;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class AnimDemoActivity extends Activity {
  private TextView tile;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    getDisplay();

    // FrameLayout root so the animated tile can live at an absolute
    // position. A vertical LinearLayout would re-layout the tile on every
    // pass and clobber x/y set via animate() (translationX/Y would work
    // there).
    FrameLayout root = new FrameLayout(this);

    // Controls column anchored to the top half — title + 3 buttons fit in
    // ~140 px, leaving the lower strip free for the tile to slide across.
    LinearLayout controls = new LinearLayout(this);
    controls.setOrientation(LinearLayout.VERTICAL);
    controls.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText("Animation Demo");
    title.setTextColor(Color.WHITE);
    controls.addView(title);

    // Android-shaped to-only animations: the start value is whatever the
    // view has now, so a toggle reads the getter instead of keeping a flag.
    Button fadeBtn = new Button(this);
    fadeBtn.setText("Fade toggle");
    // Button extends TextView — setTextColor is inherited and reaches the
    // button's child label through the TextView native arm.
    fadeBtn.setTextColor(Color.argb(255, 255, 220, 120));
    fadeBtn.setOnClickListener(
        v -> {
          float to = tile.getAlpha() < 0.5f ? 1.0f : 0.0f;
          Log.i("AnimDemo", "fade -> " + to);
          tile.animate().alpha(to).setDuration(400).start();
        });
    controls.addView(fadeBtn, new LinearLayout.LayoutParams(200, 30));

    Button slideBtn = new Button(this);
    slideBtn.setText("Slide");
    slideBtn.setOnClickListener(
        v -> {
          float to = tile.getLeft() < 90 ? 160f : 20f;
          Log.i("AnimDemo", "slide -> " + to);
          tile.animate().x(to).setDuration(300).start();
        });
    controls.addView(slideBtn, new LinearLayout.LayoutParams(200, 30));

    Button restoreBtn = new Button(this);
    restoreBtn.setText("Restore");
    restoreBtn.setOnClickListener(
        v -> {
          Log.i("AnimDemo", "restore");
          tile.setAlpha(1.0f);
          tile.setLayoutParams(tileParams());
          tile.setRotation(0f);
          tile.setScaleX(1f);
          tile.setScaleY(1f);
        });
    controls.addView(restoreBtn, new LinearLayout.LayoutParams(200, 30));

    root.addView(
        controls,
        new FrameLayout.LayoutParams(
            FrameLayout.LayoutParams.MATCH_PARENT, 150, Gravity.TOP | Gravity.LEFT));

    // The animated tile — sibling of the controls inside the FrameLayout,
    // placed by its margins. Margins + animate().x/y both work because
    // FrameLayout is a plain lv_obj with no flex flow. Kept small: a rotated
    // or scaled view renders through an off-screen layer of its own size.
    tile = new TextView(this);
    tile.setText("hello");
    tile.setBackgroundColor(Color.argb(255, 80, 120, 200));
    tile.setTextColor(Color.WHITE);
    root.addView(tile, tileParams());

    setContentView(root);

    // Startup sequence exercising an interpolator, withEndAction, a delayed
    // start, rotation/scale and the transform getters end-to-end — the two
    // log markers are what the HIL/sim harness asserts on. The second leg is
    // chained *inside* the first end action: end actions are per view, so
    // registering it up front would replace the first before it fired.
    tile.animate()
        .x(100f)
        .setDuration(120)
        .setInterpolator(new AccelerateDecelerateInterpolator())
        .withEndAction(
            () -> {
              Log.i("AnimDemo", "endaction fired");
              tile.animate()
                  .rotation(360f)
                  .scaleX(1.25f)
                  .scaleY(1.25f)
                  .setStartDelay(150)
                  .setDuration(200)
                  .withEndAction(
                      () ->
                          Log.i(
                              "AnimDemo",
                              "spin done rot=" + tile.getRotation() + " scale=" + tile.getScaleX()))
                  .start();
            })
        .start();
  }

  /** The tile's home: 60x30 at (20, 180) in the frame. */
  private static FrameLayout.LayoutParams tileParams() {
    FrameLayout.LayoutParams lp = new FrameLayout.LayoutParams(60, 30, Gravity.TOP | Gravity.LEFT);
    lp.leftMargin = 20;
    lp.topMargin = 180;
    return lp;
  }
}
