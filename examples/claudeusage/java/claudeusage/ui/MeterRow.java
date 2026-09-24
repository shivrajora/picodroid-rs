// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.Context;
import picodroid.view.ViewGroup;

/** "Opus [===== ] 61%": a name, a bar and a figure on one line. */
final class MeterRow {
  static final int HEIGHT = 21;

  private final Palette palette;
  private final String dash;
  private final Line name;
  private final Line figure;
  private final BarView bar;

  MeterRow(Context ctx, Palette palette, ViewGroup card, int y) {
    this.palette = palette;
    dash = ctx.getString(R.string.dash);
    name = new Line(Ui.label(ctx, card, "", Ui.CARD_PAD, y, palette.text), "", palette.text);
    bar = new BarView(ctx, palette, card, 84, y + 5, 156, 8);
    figure = new Line(Ui.labelRight(ctx, card, "", 246, y, 46, palette.text), "", palette.text);
  }

  void show(String label, int pct, int color, boolean stale) {
    name.show(label, stale ? palette.muted : palette.text);
    bar.setVisible(true);
    bar.show(pct, color, stale);
    figure.show(pct < 0 ? dash : pct + "%", stale ? palette.muted : palette.text);
  }

  void clear() {
    name.show("", palette.text);
    bar.show(-1, palette.good, false);
    bar.setVisible(false);
    figure.show("", palette.text);
  }
}
