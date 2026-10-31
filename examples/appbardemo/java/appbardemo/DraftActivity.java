// SPDX-License-Identifier: GPL-3.0-only
package appbardemo;

import picodroid.activity.OnBackPressedCallback;
import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.Menu;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.Toolbar;

/**
 * A screen with unsaved work: its bar has the up button (which finishes it, {@code onNavigateUp}),
 * and an {@link OnBackPressedCallback} keeps the first BACK from throwing the edit away. The
 * callback is enabled only while there is an edit, as Android's guide has it: "Edit" turns it on,
 * the first BACK after an edit only warns and turns it off again, so the next BACK leaves.
 *
 * <p>Under it, a second {@link Toolbar} that is not the action bar: a format bar with a menu of its
 * own ({@code getMenu}, picks to {@code setOnMenuItemClickListener}) and a close icon from {@code
 * res/drawable} as its navigation button.
 */
public class DraftActivity extends Activity {
  private static final int ID_BOLD = 1;
  private static final int ID_ITALIC = 2;

  private final OnBackPressedCallback keepEdit =
      new OnBackPressedCallback(false) {
        @Override
        public void handleOnBackPressed() {
          Log.i(MainActivity.TAG, "draft: unsaved edit kept, BACK again to discard");
          setEnabled(false);
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = AppBar.screen(this, true);
    getOnBackPressedDispatcher().addCallback(this, keepEdit);

    Button edit = new Button(this);
    edit.setText("Edit");
    edit.setOnClickListener(
        v -> {
          keepEdit.setEnabled(true);
          Log.i(MainActivity.TAG, "draft: edited");
        });
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(160, 48);
    lp.setMargins(12, 12, 12, 0);
    root.addView(edit, lp);

    Toolbar format = new Toolbar(this);
    format.setBackgroundColor(0xFF303F9F);
    format.setTitleTextColor(0xFFFFFFFF);
    format.setTitle("Format");
    format.setNavigationIcon(R.drawable.close);
    format.setNavigationOnClickListener(
        v -> {
          format.setVisibility(View.GONE);
          Log.i(MainActivity.TAG, "draft: format bar closed");
        });
    format.getMenu().add(Menu.NONE, ID_BOLD, Menu.NONE, "Bold");
    format.getMenu().add(Menu.NONE, ID_ITALIC, Menu.NONE, "Italic");
    format.setOnMenuItemClickListener(
        item -> {
          Log.i(MainActivity.TAG, "draft: format " + item.getTitle());
          return true;
        });
    LinearLayout.LayoutParams flp =
        new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, Toolbar.DEFAULT_HEIGHT);
    flp.topMargin = 12;
    root.addView(format, flp);
    Log.i(MainActivity.TAG, "draft ready, title " + getTitle());
  }
}
