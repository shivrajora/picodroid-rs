// SPDX-License-Identifier: GPL-3.0-only
package keyboarddemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.view.inputmethod.EditorInfo;
import picodroid.widget.Button;
import picodroid.widget.EditText;
import picodroid.widget.Keyboard;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class KeyboardDemoActivity extends Activity {
  private TextView echo;

  @Override
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

    // Press-outside-to-dismiss target: tapping anywhere on this strip
    // (with no EditText behind it) dismisses the system keyboard.
    TextView dismissHint = new TextView();
    dismissHint.setText("Tap me to dismiss");
    dismissHint.setTextColor(Color.YELLOW);
    root.addView(dismissHint);

    // EditText #1: default behavior — tapping pops up the system
    // keyboard with a slide-up animation. The OnEditorActionListener
    // logs the entered text when the user taps OK; returning false lets
    // the keyboard's default dismiss run.
    EditText auto = new EditText();
    auto.setHint("Tap to type (auto)");
    auto.setSize(220, 30);
    auto.setOnEditorActionListener(
        (v, actionId, event) -> {
          if (actionId == EditorInfo.IME_ACTION_DONE) {
            String got = v.getText();
            Log.i("KeyboardDemo", "OnEditorAction: actionId=" + actionId + " text=\"" + got + "\"");
            echo.setText("Auto: " + got);
          }
          return false; // let the keyboard hide as usual
        });
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
        clicked -> {
          Log.i("KeyboardDemo", "manual keyboard show");
          Keyboard kb = new Keyboard();
          kb.setSize(240, 140);
          kb.setPosition(0, 100);
          kb.setEditText(manual);
          // Explicit instance fires READY so the app can read the value.
          kb.setOnReadyListener(
              keyboard -> {
                String got = manual.getText();
                Log.i("KeyboardDemo", "manual got: " + got);
                echo.setText("Manual: " + got);
                keyboard.hide();
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
