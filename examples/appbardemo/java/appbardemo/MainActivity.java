// SPDX-License-Identifier: GPL-3.0-only
package appbardemo;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.Menu;
import picodroid.view.MenuItem;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picodroid.widget.Toolbar;

/**
 * The app bar, as an Android app writes it: a {@link Toolbar} at the top of the layout made the
 * Activity's action bar with {@code setActionBar}. Its title is the manifest's {@code
 * android:label} ("Inbox"), and the options menu sits behind its overflow button (in place of the
 * floating control a touch board draws for an Activity without a bar). "Compose" opens {@link
 * DraftActivity}, whose bar has the up button. Every step is logged for the sim row.
 */
public class MainActivity extends Activity {
  static final String TAG = "AppBarDemo";
  private static final int ID_REFRESH = 1;
  private static final int ID_SORT = 2;

  private TextView status;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = AppBar.screen(this, false);

    status = new TextView(this);
    status.setText("Nothing new");
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    lp.setMargins(12, 12, 12, 0);
    root.addView(status, lp);

    Button compose = new Button(this);
    compose.setText("Compose");
    compose.setOnClickListener(v -> startActivity(new Intent(this, DraftActivity.class)));
    LinearLayout.LayoutParams blp = new LinearLayout.LayoutParams(160, 48);
    blp.setMargins(12, 12, 12, 0);
    root.addView(compose, blp);
    Log.i(TAG, "inbox ready, title " + getTitle());
  }

  @Override
  public boolean onCreateOptionsMenu(Menu menu) {
    menu.add(Menu.NONE, ID_REFRESH, Menu.NONE, "Refresh");
    menu.add(Menu.NONE, ID_SORT, Menu.NONE, "Sort by date");
    return true;
  }

  @Override
  public boolean onOptionsItemSelected(MenuItem item) {
    status.setText(item.getTitle() + " picked");
    Log.i(TAG, "selected " + item.getTitle());
    return true;
  }
}
