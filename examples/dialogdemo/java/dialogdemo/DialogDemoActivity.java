package dialogdemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.AlertDialog;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;
import picodroid.widget.Toast;

public class DialogDemoActivity extends Activity {
  public void onCreate() {
    // Force display init before constructing any widgets — see KeyDemoActivity
    // for the same idiom. Display.getInstance() is what brings up LVGL; without
    // this, widget nativeCreate calls would parent into a null screen.
    getDisplay();
    Log.i("DialogDemo", "Display ready");

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
        () -> {
          Log.i("DialogDemo", "toast button clicked");
          Toast.makeText("Hello from Picodroid!", Toast.LENGTH_SHORT).show();
        });
    root.addView(toastBtn);

    Button dialogBtn = new Button("Show Dialog");
    dialogBtn.setSize(200, 40);
    dialogBtn.setOnClickListener(
        () -> {
          Log.i("DialogDemo", "dialog button clicked");
          new AlertDialog.Builder()
              .setTitle("Delete?")
              .setMessage("Are you sure?")
              .setPositiveButton("OK", () -> Log.i("DialogDemo", "user confirmed"))
              .setNegativeButton("Cancel", () -> Log.i("DialogDemo", "user cancelled"))
              .show();
        });
    root.addView(dialogBtn);

    setContentView(root);
  }
}
