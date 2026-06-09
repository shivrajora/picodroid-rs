// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.ui.home;

import picodroid.content.Intent;
import picodroid.graphics.Theme;
import picodroid.util.Log;
import picodroid.widget.ArrayAdapter;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.TextView;
import picoenvmon.di.EnvAppComponent;
import picoenvmon.ui.common.NavActivity;
import picoenvmon.ui.history.HistoryActivity;
import picoenvmon.ui.live.LiveActivity;
import picoenvmon.ui.settings.SettingsActivity;

/**
 * Root hub: a selectable menu of destinations under the standardized 4-button navigation model. A/B
 * move the highlight, X opens the highlighted screen; Y is intentionally disabled here so the root
 * hub can't be backed out of (which would exit the app). The live 5-tile sensor dashboard lives in
 * {@link LiveActivity}; History and Settings are siblings. Adding a screen later is one more {@code
 * LABELS}/{@code DESTINATIONS} entry plus the new Activity.
 */
public class HomeActivity extends NavActivity {

  private static final String[] LABELS = {"Live", "History", "Settings"};
  private static final Class<?>[] DESTINATIONS = {
    LiveActivity.class, HistoryActivity.class, SettingsActivity.class
  };

  // Held as a field so the GC roots the menu ListView via this Activity, in addition to the native
  // item-click listener map — defense-in-depth against the unfielded-callback-view sweep.
  private ListView menu;

  @Override
  public void onCreate() {
    Log.i(EnvAppComponent.TAG, "Home.onCreate");
    getDisplay();

    LinearLayout root = makeScreenRoot();

    TextView title = new TextView();
    title.setText("PicoEnvMon");
    title.setTextColor(Theme.colorPrimary);
    root.addView(title);

    menu = new ListView();
    menu.setSize(224, 188);
    menu.setAdapter(new ArrayAdapter<String>(LABELS));
    // Android-faithful 4-arg item-click: A/B move the row highlight, X (ENTER) activates the
    // focused
    // row -> open its destination.
    menu.setOnItemClickListener(
        (parent, view, position, id) -> startActivity(new Intent(DESTINATIONS[position])));
    root.addView(menu);

    installHintBar(root, "A:Up  B:Down  X:Open");

    setContentView(root);
  }

  // Root hub: Back has nowhere to return to, so swallow it instead of finishing (which would exit
  // the app). Deliberately does not call super.onBackPressed().
  @Override
  public void onBackPressed() {
    // no-op
  }
}
