// SPDX-License-Identifier: GPL-3.0-only
package androidport;

import android.app.Activity;
import android.graphics.Color;
import android.util.Log;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

/**
 * Built entirely from android.* imports. Exercises the full alias path: plain class references
 * (Activity, TextView, ...), a nested interface (View.OnClickListener), and an invokedynamic SAM
 * lambda whose method descriptor names android.view.View — all rewritten to picodroid/* by
 * class-shrink. {@code performClick()} fires the listener synthetically so the token prints without
 * a tap.
 */
public class AndroidPortActivity extends Activity {
  @Override
  public void onCreate() {
    Log.i("AndroidPort", "onCreate via android.* imports");
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Ported from android.*");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button btn = new Button("Tap");
    btn.setSize(100, 30);
    btn.setOnClickListener(v -> Log.i("AndroidPort", "android.view.View.OnClickListener fired"));
    root.addView(btn);
    btn.performClick();

    setContentView(root);
    Log.i("AndroidPort", "AndroidPort ready");
  }
}
