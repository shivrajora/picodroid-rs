// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.view.View;
import picodroid.widget.LinearLayout;

/**
 * A screen's rows, built one per UI-thread Runnable. A row costs the device about 20 ms (three
 * LVGL objects and a dozen natives), so a screen of ten rows built inside {@code onCreate} would
 * hold the UI tick for 200 ms. Built this way the header is on screen at once and every row lands
 * between two ticks, each under the 50 ms the slow-handler watchdog allows. {@link #stop} ends the
 * chain when the screen goes away or is rebuilt.
 */
final class Column {
  /** Row {@code i} of the screen, {@code null} once there are no more. */
  interface Rows {
    View row(int i);
  }

  final LinearLayout layout;
  private final Activity activity;
  private int next;
  private boolean stopped;

  /** The header, in place and on screen; the rows follow through {@link #fill}. */
  Column(Activity a, String title, View.OnClickListener onTitle) {
    activity = a;
    layout = Screens.column(a);
    layout.addView(Screens.header(a, title, onTitle));
    a.setContentView(Screens.scrollable(a, layout, 1));
  }

  /**
   * On the next tick run {@code prepare} (the queries the rows need; may be null), then add {@code
   * rows}'s rows one per tick, then size the column to them and run {@code done} (may be null).
   */
  void fill(Runnable prepare, Rows rows, Runnable done) {
    next = 0;
    Executors.mainExecutor()
        .execute(
            () -> {
              if (stopped) {
                return;
              }
              if (prepare != null) {
                prepare.run();
              }
              Executors.mainExecutor().execute(() -> addNext(rows, done));
            });
  }

  private void addNext(Rows rows, Runnable done) {
    if (stopped) {
      return;
    }
    View row = rows.row(next);
    if (row != null) {
      layout.addView(row);
      next++;
      Executors.mainExecutor().execute(() -> addNext(rows, done));
      return;
    }
    Screens.size(activity, layout, 1 + next);
    if (done != null) {
      done.run();
    }
  }

  void stop() {
    stopped = true;
  }
}
