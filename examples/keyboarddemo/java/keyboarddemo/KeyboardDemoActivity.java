// SPDX-License-Identifier: GPL-3.0-only
package keyboarddemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.ViewGroup;
import picodroid.view.inputmethod.EditorInfo;
import picodroid.widget.Button;
import picodroid.widget.EditText;
import picodroid.widget.Keyboard;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class KeyboardDemoActivity extends Activity {
  private TextView echo;
  private LinearLayout root;
  private Keyboard keyboard;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Log.i("KeyboardDemo", "ready");

    // The window is a column: the form takes what the explicit keyboard leaves, so showing the
    // keyboard below it shrinks the form rather than covering it.
    root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);

    LinearLayout form = new LinearLayout(this);
    form.setOrientation(LinearLayout.VERTICAL);
    form.setPadding(8, 8, 8, 8);
    root.addView(form, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

    TextView title = new TextView(this);
    title.setText("Keyboard Demo");
    title.setTextColor(Color.WHITE);
    form.addView(title);

    // Press-outside-to-dismiss target: tapping anywhere on this strip
    // (with no EditText behind it) dismisses the system keyboard.
    TextView dismissHint = new TextView(this);
    dismissHint.setText("Tap me to dismiss");
    dismissHint.setTextColor(Color.YELLOW);
    form.addView(dismissHint);

    // EditText #1: default behavior — tapping pops up the system
    // keyboard with a slide-up animation. The OnEditorActionListener
    // logs the entered text when the user taps OK; returning false lets
    // the keyboard's default dismiss run.
    EditText auto = new EditText(this);
    auto.setHint("Tap to type (auto)");
    auto.setOnEditorActionListener(
        (v, actionId, event) -> {
          if (actionId == EditorInfo.IME_ACTION_DONE) {
            String got = v.getText();
            Log.i("KeyboardDemo", "OnEditorAction: actionId=" + actionId + " text=\"" + got + "\"");
            echo.setText("Auto: " + got);
          }
          return false; // let the keyboard hide as usual
        });
    form.addView(auto, new LinearLayout.LayoutParams(220, 30));

    // TextWatcher: live character count, plus a deterministic setText-fires-
    // watcher check (Android semantics — programmatic changes notify too).
    TextView counter = new TextView(this);
    counter.setText("len=0");
    counter.setTextColor(Color.CYAN);
    form.addView(counter);
    auto.addTextChangedListener(
        s -> {
          counter.setText("len=" + s.length());
          Log.i("KeyboardDemo", "afterTextChanged: \"" + s + "\"");
        });
    auto.setText("seed");

    // EditText #2: opted out of the system keyboard. The "Type" button
    // below adds an explicit Keyboard under the form, binds it, and shows it.
    EditText manual = new EditText(this);
    manual.setHint("Manual mode");
    manual.setShowKeyboardOnTouch(false);
    form.addView(manual, new LinearLayout.LayoutParams(220, 30));

    Button typeBtn = new Button(this);
    typeBtn.setText("Type into manual");
    typeBtn.setOnClickListener(
        clicked -> {
          Log.i("KeyboardDemo", "manual keyboard show");
          if (keyboard == null) {
            keyboard = new Keyboard(this);
            keyboard.setEditText(manual);
            // Explicit instance fires READY so the app can read the value.
            keyboard.setOnReadyListener(
                kb -> {
                  String got = manual.getText();
                  Log.i("KeyboardDemo", "manual got: " + got);
                  echo.setText("Manual: " + got);
                  kb.hide();
                });
            root.addView(
                keyboard, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 140));
          }
          keyboard.show();
        });
    form.addView(typeBtn, new LinearLayout.LayoutParams(220, 28));

    echo = new TextView(this);
    echo.setText("(echo)");
    echo.setTextColor(Color.CYAN);
    form.addView(echo);

    setContentView(root);
  }
}
