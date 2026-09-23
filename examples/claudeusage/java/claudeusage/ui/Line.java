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

  /** An inflated label: whatever it shows now (the widget default is "Text"), it starts blank. */
  Line(TextView view, int color) {
    this(view, "", color);
    view.setText("");
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
