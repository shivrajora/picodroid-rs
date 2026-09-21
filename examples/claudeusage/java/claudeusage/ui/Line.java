// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import picodroid.widget.TextView;

/**
 * A TextView that remembers what it shows. The screens redraw once a second; re-setting an
 * unchanged label would re-lay-out and re-rasterise it every time.
 */
final class Line {
  private final TextView view;
  private String text;
  private int color;

  Line(TextView view, String text, int color) {
    this.view = view;
    this.text = text;
    this.color = color;
  }

  void show(String newText, int newColor) {
    if (!newText.equals(text)) {
      view.setText(newText);
      text = newText;
    }
    if (newColor != color) {
      view.setTextColor(newColor);
      color = newColor;
    }
  }
}
