// SPDX-License-Identifier: GPL-3.0-only
package snackbardemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.Snackbar;
import picodroid.widget.TextView;

public class SnackbarDemoActivity extends Activity {
  private static final String TAG = "SnackbarDemo";

  private int undoCount = 0;

  @Override
  public void onCreate() {
    getDisplay();
    Log.i(TAG, "Display ready");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Snackbar Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button plain = new Button("Plain (auto-dismiss)");
    plain.setSize(220, 36);
    plain.setOnClickListener(
        v -> {
          Log.i(TAG, "plain snackbar");
          Snackbar.make(root, "Saved", Snackbar.LENGTH_SHORT).show();
        });
    root.addView(plain);

    Button withAction = new Button("With UNDO");
    withAction.setSize(220, 36);
    withAction.setOnClickListener(
        v -> {
          Log.i(TAG, "snackbar with action");
          Snackbar.make(root, "Item deleted", Snackbar.LENGTH_LONG)
              .setAction(
                  "UNDO",
                  view -> {
                    undoCount++;
                    Log.i(TAG, "user undid (count=" + undoCount + ")");
                  })
              .show();
        });
    root.addView(withAction);

    Button indef = new Button("Indefinite");
    indef.setSize(220, 36);
    indef.setOnClickListener(
        v -> {
          Log.i(TAG, "indefinite snackbar");
          Snackbar.make(root, "Tap RETRY to dismiss", Snackbar.LENGTH_INDEFINITE)
              .setAction("RETRY", view -> Log.i(TAG, "user retried"))
              .show();
        });
    root.addView(indef);

    setContentView(root);
  }
}
