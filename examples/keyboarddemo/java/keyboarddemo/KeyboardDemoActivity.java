package keyboarddemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.EditText;
import picodroid.widget.Keyboard;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class KeyboardDemoActivity extends Activity {
  private TextView echo;

  public void onCreate() {
    getDisplay();
    Log.i("KeyboardDemo", "ready");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 100);
    root.setPadding(8, 8, 8, 8);

    TextView title = new TextView();
    title.setText("Keyboard Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    // EditText #1: default behavior — tapping pops up the system keyboard
    // automatically. Dismissed by BACK or the keyboard's OK key.
    EditText auto = new EditText();
    auto.setHint("Tap to type (auto)");
    auto.setSize(220, 30);
    root.addView(auto);

    // EditText #2: opted out of the system keyboard. The "Type" button
    // below constructs an explicit Keyboard, binds it, and shows it.
    EditText manual = new EditText();
    manual.setHint("Manual mode");
    manual.setSize(220, 30);
    manual.setShowKeyboardOnTouch(false);
    root.addView(manual);

    Button typeBtn = new Button("Type into manual");
    typeBtn.setSize(220, 28);
    typeBtn.setOnClickListener(
        () -> {
          Log.i("KeyboardDemo", "manual keyboard show");
          Keyboard kb = new Keyboard();
          kb.setSize(240, 140);
          kb.setPosition(0, 100);
          kb.setEditText(manual);
          // Explicit instance fires READY so the app can read the value.
          kb.setOnReadyListener(
              () -> {
                String got = manual.getText();
                Log.i("KeyboardDemo", "manual got: " + got);
                echo.setText("Manual: " + got);
                kb.hide();
              });
          kb.show();
        });
    root.addView(typeBtn);

    echo = new TextView();
    echo.setText("(echo)");
    echo.setTextColor(Color.CYAN);
    root.addView(echo);

    setContentView(root);
  }
}
