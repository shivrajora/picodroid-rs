// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.Context;
import picodroid.view.ViewGroup;

/** "Opus [===== ] 61%": a name, a bar and a figure on one line. */
final class MeterRow {
  static final int HEIGHT = 21;

  private final Palette p;
  private final String dash;
  private final Line name;
  private final Line figure;
  private final BarView bar;

  MeterRow(Context ctx, Palette p, ViewGroup card, int y) {
    this.p = p;
    dash = ctx.getString(R.string.dash);
    name = new Line(Ui.label(ctx, card, "", Ui.CARD_PAD, y, p.text), "", p.text);
    bar = new BarView(ctx, p, card, 84, y + 5, 156, 8, false);
    figure = new Line(Ui.labelRight(ctx, card, "", 246, y, 46, p.text), "", p.text);
  }

  void show(String label, int pct, int color, boolean stale) {
    name.show(label, stale ? p.muted : p.text);
    bar.setVisible(true);
    bar.show(pct, color, -1, stale);
    figure.show(pct < 0 ? dash : pct + "%", stale ? p.muted : p.text);
  }

  void clear() {
    name.show("", p.text);
    bar.show(-1, p.good, -1, false);
    bar.setVisible(false);
    figure.show("", p.text);
  }
}
