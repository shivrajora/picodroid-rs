// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Activity;
import picodroid.content.Intent;

/** Reports a result and finishes from inside onCreate. */
public class ActB extends Activity {
  @Override
  public void onCreate() {
    T.log("B.onCreate");
    Intent in = getIntent();
    T.check("B intent extra", in != null && in.getIntExtra("q", -1) == 9);
    setResult(RESULT_OK, new Intent().putExtra("r", 7).putExtra("rs", "res").putExtra("rb", true));
    finish();
  }

  @Override
  public void onStart() {
    T.log("B.onStart");
  }

  @Override
  public void onResume() {
    T.log("B.onResume");
  }

  @Override
  public void onPause() {
    T.log("B.onPause");
  }

  @Override
  public void onStop() {
    T.log("B.onStop");
  }

  @Override
  public void onDestroy() {
    T.log("B.onDestroy");
  }
}
