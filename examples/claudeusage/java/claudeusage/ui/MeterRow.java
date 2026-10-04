// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.view.View;
import picodroid.widget.ProgressBar;
import picodroid.widget.TextView;

/**
 * "Opus [===== ] 61%": a name, a bar and a figure on one line, over an inflated {@code
 * res/layout/meter_row.xml}.
 */
final class MeterRow {
  private final Palette palette;
  private final String dash;
  private final TextView name;
  private final TextView figure;
  private final ProgressBar bar;

  MeterRow(Context ctx, View row) {
    palette = Palette.of(ctx.getResources());
    dash = ctx.getString(R.string.dash);
    name = row.findViewById(R.id.meter_name);
    bar = row.findViewById(R.id.meter_bar);
    figure = row.findViewById(R.id.meter_figure);
  }

  /**
   * @param pct 0..100, or negative for "unknown" (an empty track)
   * @param color the bar's fill colour
   * @param stale whether the figure is no longer live: the fill fades, the track stays
   */
  void show(String label, int pct, int color, boolean stale) {
    int ink = stale ? palette.muted : palette.text;
    name.setText(label);
    name.setTextColor(ink);
    bar.setVisibility(View.VISIBLE);
    bar.setProgress(pct < 0 ? 0 : pct, true);
    bar.setProgressTintList(ColorStateList.valueOf(color).withAlpha(stale ? Ui.DIM_ALPHA : 0xFF));
    figure.setText(pct < 0 ? dash : pct + "%");
    figure.setTextColor(ink);
  }

  /** An unused row is blank, track included, not an empty gauge. */
  void clear() {
    name.setText("");
    bar.setVisibility(View.INVISIBLE);
    figure.setText("");
  }
}
