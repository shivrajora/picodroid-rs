// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.app.Activity;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.widget.TextView;

/** The boot Activity: covered by Fill for the whole test, it must survive it, then reports. */
public class ActHome extends Activity {
  static int instances;
  static boolean destroyed;
  private int resumes;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    instances++;
    TextView label = new TextView(this);
    label.setText("qa_trim home");
    setContentView(label);
  }

  @Override
  public void onResume() {
    super.onResume();
    resumes++;
    if (resumes == 1) {
      startActivity(new Intent(ActFill.class));
      return;
    }
    T.check("home instance survived the round", instances == 1 && !destroyed);
    T.report();
    finish();
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
    destroyed = true;
  }

  @Override
  public void onTrimMemory(int level) {
    T.log("home:" + level);
  }

  @Override
  public void onLowMemory() {
    T.log("home:low");
  }
}
