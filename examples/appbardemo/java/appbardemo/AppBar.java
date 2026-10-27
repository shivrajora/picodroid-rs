// SPDX-License-Identifier: GPL-3.0-only
package appbardemo;

import picodroid.app.Activity;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.Toolbar;

/** The screens' one shape: an app bar over a column, set as the content view. */
final class AppBar {
  private static final int BAR_COLOR = 0xFF3F51B5;

  private AppBar() {}

  /** A column under a {@link Toolbar} that is {@code a}'s action bar; {@code up}: the up button. */
  static LinearLayout screen(Activity a, boolean up) {
    LinearLayout root = new LinearLayout(a);
    root.setOrientation(LinearLayout.VERTICAL);
    Toolbar bar = new Toolbar(a);
    bar.setBackgroundColor(BAR_COLOR);
    bar.setTitleTextColor(0xFFFFFFFF);
    root.addView(
        bar,
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Toolbar.DEFAULT_HEIGHT));
    a.setActionBar(bar);
    a.getActionBar().setDisplayHomeAsUpEnabled(up);
    a.setContentView(root);
    return root;
  }
}
