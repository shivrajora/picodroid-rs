// SPDX-License-Identifier: GPL-3.0-only
package snackbardemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.Snackbar;
import picodroid.widget.TextView;

public class SnackbarDemoActivity extends Activity {
  private static final String TAG = "SnackbarDemo";

  private int undoCount = 0;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i(TAG, "Display ready");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText("Snackbar Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button plain = new Button(this);
    plain.setText("Plain (auto-dismiss)");
    plain.setOnClickListener(
        v -> {
          Log.i(TAG, "plain snackbar");
          Snackbar.make(root, "Saved", Snackbar.LENGTH_SHORT).show();
        });
    root.addView(plain, new LinearLayout.LayoutParams(220, 36));

    Button withAction = new Button(this);
    withAction.setText("With UNDO");
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
    root.addView(withAction, new LinearLayout.LayoutParams(220, 36));

    Button indef = new Button(this);
    indef.setText("Indefinite");
    indef.setOnClickListener(
        v -> {
          Log.i(TAG, "indefinite snackbar");
          Snackbar.make(root, "Tap RETRY to dismiss", Snackbar.LENGTH_INDEFINITE)
              .setAction("RETRY", view -> Log.i(TAG, "user retried"))
              .show();
        });
    root.addView(indef, new LinearLayout.LayoutParams(220, 36));

    setContentView(root);
  }
}
