// SPDX-License-Identifier: GPL-3.0-only
package themedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.Theme;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class ThemeDemoActivity extends Activity {
  public void onCreate() {
    getDisplay();
    Log.i("ThemeDemo", "applying theme");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);
    root.setBackgroundColor(Theme.colorBackground);

    // Header with a vertical gradient + rounded bottom corners.
    TextView header = new TextView();
    header.setSize(220, 50);
    header.setText("  Theme Demo");
    header.setTextColor(Theme.colorOnPrimary);
    header.setBackground(
        new GradientDrawable()
            .setGradient(
                Theme.colorPrimary,
                darken(Theme.colorPrimary),
                GradientDrawable.Orientation.TOP_BOTTOM)
            .setCornerRadius(8));
    root.addView(header);

    // A surface "card" with stroke + rounded corners.
    TextView card = new TextView();
    card.setSize(220, 60);
    card.setText("  Card surface");
    card.setTextColor(Theme.colorText);
    card.setBackground(
        new GradientDrawable()
            .setColor(Theme.colorSurface)
            .setCornerRadius(10)
            .setStroke(1, Theme.colorOutline));
    root.addView(card);

    // A pill-shaped accent button.
    Button pill = new Button("Pill button");
    pill.setSize(220, 38);
    pill.setBackground(
        new GradientDrawable()
            .setColor(Theme.colorPrimary)
            // Pill = corner radius >= half the height. Setting it past
            // that doesn't visually overshoot — LVGL clamps internally.
            .setCornerRadius(20));
    pill.setOnClickListener(v -> Log.i("ThemeDemo", "pill clicked"));
    root.addView(pill);

    // An outlined ghost button — surface fill, primary stroke.
    Button ghost = new Button("Ghost button");
    ghost.setSize(220, 38);
    ghost.setBackground(
        new GradientDrawable()
            .setColor(Theme.colorBackground)
            .setCornerRadius(8)
            .setStroke(2, Theme.colorPrimary));
    ghost.setOnClickListener(v -> Log.i("ThemeDemo", "ghost clicked"));
    root.addView(ghost);

    setContentView(root);
  }

  /** Quick-and-dirty 70%-brightness shade for the gradient end stop. */
  private static int darken(int argb) {
    int a = (argb >> 24) & 0xFF;
    int r = ((argb >> 16) & 0xFF) * 70 / 100;
    int g = ((argb >> 8) & 0xFF) * 70 / 100;
    int b = (argb & 0xFF) * 70 / 100;
    return Color.argb(a, r, g, b);
  }
}
