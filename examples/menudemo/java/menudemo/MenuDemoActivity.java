// SPDX-License-Identifier: GPL-3.0-only
package menudemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.Menu;
import picodroid.view.MenuItem;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * The options menu as the write-once action surface (app portability, K9): three actions declared
 * once in {@link #onCreateOptionsMenu}, reached on a four-key board by holding SELECT, on a touch
 * board through the menu control the framework draws bottom-right, and anywhere by a MENU key.
 * Every step is logged so a sim row can drive it from the control channel.
 */
public class MenuDemoActivity extends Activity {
  private static final String TAG = "MenuDemo";
  private static final int ID_REFRESH = 1;
  private static final int ID_UNITS = 2;
  private static final int ID_ABOUT = 3;

  private TextView status;
  private int shown;
  private int closed;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.MATCH_PARENT);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Options Menu Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    status = new TextView();
    status.setText("Hold SELECT, press MENU or tap the menu control");
    status.setTextColor(Color.CYAN);
    // The width of the column, so the hint wraps on a 240-wide panel rather than running off it.
    status.setSize(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
    root.addView(status);

    // A focusable Button with no long press of its own: holding SELECT on it opens the menu.
    Button focus = new Button("Focus me");
    focus.setSize(200, 50);
    focus.setOnClickListener(v -> show("clicked"));
    root.addView(focus);

    setContentView(root);
    focus.requestFocus();
    Log.i(TAG, "ready");
  }

  @Override
  public boolean onCreateOptionsMenu(Menu menu) {
    menu.add(Menu.NONE, ID_REFRESH, Menu.NONE, "Refresh");
    menu.add(Menu.NONE, ID_UNITS, Menu.NONE, "Toggle units");
    MenuItem about = menu.add(Menu.NONE, ID_ABOUT, Menu.NONE, "About");
    // An item may take its pick itself, before onOptionsItemSelected.
    about.setOnMenuItemClickListener(
        item -> {
          show("about via listener");
          return true;
        });
    Log.i(TAG, "menu created items=" + menu.size());
    return true;
  }

  @Override
  public boolean onPrepareOptionsMenu(Menu menu) {
    shown++;
    Log.i(TAG, "menu shown #" + shown);
    return true;
  }

  @Override
  public boolean onOptionsItemSelected(MenuItem item) {
    show("selected " + item.getTitle() + " id=" + item.getItemId());
    return true;
  }

  @Override
  public void onOptionsMenuClosed(Menu menu) {
    closed++;
    Log.i(TAG, "menu closed #" + closed);
  }

  private void show(String line) {
    status.setText(line);
    Log.i(TAG, line);
  }
}
