// SPDX-License-Identifier: GPL-3.0-only
package dialogdemo;

import picodroid.app.Activity;
import picodroid.app.AlertDialog;
import picodroid.graphics.Color;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picodroid.widget.Toast;

public class DialogDemoActivity extends Activity {
  @Override
  public void onCreate() {
    DialogActivityComponent c = new DialogActivityComponent();

    // Force display init before constructing any widgets — see KeyDemoActivity
    // for the same idiom. Display.getInstance() is what brings up LVGL; without
    // this, widget nativeCreate calls would parent into a null screen.
    getDisplay();
    c.appComponent().info("Display ready");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Toast & Dialog Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button toastBtn = new Button("Show Toast");
    toastBtn.setSize(200, 40);
    toastBtn.setOnClickListener(
        v -> {
          c.appComponent().info("toast button clicked");
          Toast.makeText(this, "Hello from Picodroid!", Toast.LENGTH_SHORT).show();
        });
    root.addView(toastBtn);

    Button dialogBtn = new Button("Show Dialog");
    dialogBtn.setSize(200, 40);
    dialogBtn.setOnClickListener(
        v -> {
          c.appComponent().info("dialog clicked #" + c.incShowCount());
          new AlertDialog.Builder()
              .setTitle("Delete?")
              .setMessage("Are you sure?")
              .setPositiveButton("OK", (dialog, which) -> c.appComponent().info("user confirmed"))
              .setNegativeButton(
                  "Cancel", (dialog, which) -> c.appComponent().info("user cancelled"))
              .setNeutralButton("Later", (dialog, which) -> c.appComponent().info("user deferred"))
              .show();
        });
    root.addView(dialogBtn);

    Button listBtn = new Button("Pick One");
    listBtn.setSize(200, 40);
    listBtn.setOnClickListener(
        v -> {
          AlertDialog d =
              new AlertDialog.Builder()
                  .setTitle("Pick a color")
                  .setItems(
                      new String[] {"Red", "Green", "Blue"},
                      (dialog, which) -> c.appComponent().info("picked item " + which))
                  .show();
          // Headless verification: synthetically tap "Green" (index 1).
          d.performItemClick(1);
        });
    root.addView(listBtn);

    // Drive the list dialog once at startup so the HIL/sim run exercises the
    // setItems path without needing pixel-accurate touch on the list row.
    AlertDialog startup =
        new AlertDialog.Builder()
            .setTitle("Pick a color")
            .setItems(
                new String[] {"Red", "Green", "Blue"},
                (dialog, which) -> c.appComponent().info("picked item " + which))
            .show();
    startup.performItemClick(2); // selects "Blue" → "picked item 2"

    setContentView(root);
  }
}
