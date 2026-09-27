// SPDX-License-Identifier: GPL-3.0-only
package settings;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.net.wifi.WifiManager;
import picodroid.os.Bundle;
import picodroid.text.InputType;
import picodroid.util.Log;
import picodroid.view.inputmethod.EditorInfo;
import picodroid.widget.Button;
import picodroid.widget.EditText;
import picodroid.widget.LinearLayout;

/**
 * The password for one network (its SSID in the Intent's {@code ssid} extra): a masked {@link
 * EditText} and a Connect button. On a touch panel a tap on the field opens the system keyboard;
 * on a keypad board NEXT reaches the field, ENTER opens the keyboard, PREV/NEXT walk its keys,
 * ENTER types one, ESC closes it, and the keyboard's OK key connects as the button does. The join
 * itself runs on the link task; this screen finishes at once and the Wi-Fi screen shows the
 * outcome.
 *
 * <p>Row 0 is the header, the field sits in row 1 and the button in row 2.
 */
public class WifiPasswordActivity extends Activity {
  private static final String TAG = SettingsActivity.TAG;

  private String ssid;
  private EditText field;
  private Button connect;
  private boolean done;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    ssid = getIntent().getStringExtra("ssid");
    if (ssid == null) {
      ssid = "";
    }
    int width = getDisplay().getWidth();

    LinearLayout root = Screens.column(this);
    root.addView(Screens.header(this, "< " + ssid, v -> finish()));

    // The field's row is not focusable itself: the field is, and takes
    // the keypad's select (ENTER opens the keyboard).
    LinearLayout fieldRow = new LinearLayout();
    fieldRow.setOrientation(LinearLayout.HORIZONTAL);
    fieldRow.setSize(width, Screens.ROW_HEIGHT);
    fieldRow.setPadding(8, 5, 8, 5);
    fieldRow.setSpacing(0);
    field = new EditText();
    field.setHint("Password");
    field.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
    field.setSize(width - 16, Screens.ROW_HEIGHT - 10);
    field.setOnEditorActionListener(
        (v, actionId, event) -> {
          if (actionId == EditorInfo.IME_ACTION_DONE) {
            submit();
          }
          return false;
        });
    fieldRow.addView(field);
    root.addView(fieldRow);

    LinearLayout buttonRow = new LinearLayout();
    buttonRow.setOrientation(LinearLayout.HORIZONTAL);
    buttonRow.setSize(width, Screens.ROW_HEIGHT);
    buttonRow.setPadding(8, 4, 8, 4);
    buttonRow.setSpacing(0);
    connect = new Button("Connect");
    connect.setSize(width - 16, Screens.ROW_HEIGHT - 8);
    connect.setOnClickListener(v -> submit());
    buttonRow.addView(connect);
    root.addView(buttonRow);

    setContentView(Screens.scrollable(this, root, 3));
    field.requestFocus();
    Log.i(TAG, "wifi password " + ssid);
  }

  /** Save and join, once; the outcome shows on the Wi-Fi screen. */
  private void submit() {
    if (done) {
      return;
    }
    String password = field.getText();
    if (password == null || password.length() == 0) {
      Log.i(TAG, "wifi password empty");
      return;
    }
    done = true;
    WifiActivity.connect((WifiManager) getSystemService(Context.WIFI_SERVICE), ssid, password);
    finish();
  }
}
