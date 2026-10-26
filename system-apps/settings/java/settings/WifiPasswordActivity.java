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
 * EditText} and a Connect button. On a touch panel a tap on the field opens the system keyboard; on
 * a keypad board NEXT reaches the field, ENTER opens the keyboard, PREV/NEXT walk its keys, ENTER
 * types one, ESC closes it, and the keyboard's OK key connects as the button does. The join itself
 * runs on the link task; this screen finishes at once and the Wi-Fi screen shows the outcome.
 *
 * <p>Row 0 is the header, the field sits in row 1 and the button in row 2.
 */
public class WifiPasswordActivity extends Activity {
  private static final int MATCH_PARENT = LinearLayout.LayoutParams.MATCH_PARENT;

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
    LinearLayout root = Screens.column(this);
    Screens.header(this, root, "< " + ssid, v -> finish());

    // The field's row is not focusable itself: the field is, and takes
    // the keypad's select (ENTER opens the keyboard).
    LinearLayout fieldRow = new LinearLayout(this);
    fieldRow.setOrientation(LinearLayout.HORIZONTAL);
    fieldRow.setPadding(8, 5, 8, 5);
    field = new EditText(this);
    field.setHint("Password");
    field.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD);
    field.setOnEditorActionListener(
        (v, actionId, event) -> {
          if (actionId == EditorInfo.IME_ACTION_DONE) {
            submit();
          }
          return false;
        });
    // Each row is a row high; the field and the button fill it inside its padding.
    fieldRow.addView(field, new LinearLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT));
    root.addView(fieldRow, new LinearLayout.LayoutParams(MATCH_PARENT, Screens.ROW_HEIGHT));

    LinearLayout buttonRow = new LinearLayout(this);
    buttonRow.setOrientation(LinearLayout.HORIZONTAL);
    buttonRow.setPadding(8, 4, 8, 4);
    connect = new Button(this);
    connect.setText("Connect");
    connect.setOnClickListener(v -> submit());
    buttonRow.addView(connect, new LinearLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT));
    root.addView(buttonRow, new LinearLayout.LayoutParams(MATCH_PARENT, Screens.ROW_HEIGHT));

    setContentView(Screens.scrollable(this, root));
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
