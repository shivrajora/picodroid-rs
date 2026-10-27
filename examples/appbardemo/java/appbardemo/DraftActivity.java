// SPDX-License-Identifier: GPL-3.0-only
package appbardemo;

import picodroid.activity.OnBackPressedCallback;
import picodroid.app.Activity;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;

/**
 * A screen with unsaved work: its bar has the up button (which finishes it, {@code onNavigateUp}),
 * and an {@link OnBackPressedCallback} keeps the first BACK from throwing the edit away. The
 * callback is enabled only while there is an edit, as Android's guide has it: "Edit" turns it on,
 * the first BACK after an edit only warns and turns it off again, so the next BACK leaves.
 */
public class DraftActivity extends Activity {
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
    Log.i(MainActivity.TAG, "draft ready, title " + getTitle());
  }
}
