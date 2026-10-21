// SPDX-License-Identifier: GPL-3.0-only
package qa_trim;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Grows a hidden cache of labels a few at a time until the framework's pool poll says the LVGL pool
 * is short (an eighth free), then checks the delivery and drops the cache.
 */
public class ActFill extends Activity {
  /** Labels added per step: about 1–2 KB of pool, well inside the headroom at the threshold. */
  private static final int STEP = 8;

  /** Past this the pool would be at risk of running out, which hangs the UI task: give up. */
  private static final int MAX_VIEWS = 2400;

  private static final int FILL = 0;
  private static final int VERIFY = 1;
  private static final int LEAF = 2;
  private static final int DONE = 3;

  static int instances;

  private LinearLayout cache;
  private ScheduledFuture<?> task;
  private int phase = FILL;
  private int views;
  private int viewsAtTrim = -1;
  private int restarts;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    instances++;
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    TextView label = new TextView(this);
    label.setText("qa_trim: filling the pool");
    root.addView(label);
    cache = new LinearLayout(this);
    cache.setOrientation(LinearLayout.VERTICAL);
    cache.setVisibility(View.GONE);
    root.addView(cache);
    setContentView(root);
  }

  @Override
  public void onResume() {
    super.onResume();
    if (phase == FILL && task == null) {
      task =
          Executors.mainScheduledExecutor()
              .scheduleAtFixedRate(this::step, 150, 150, TimeUnit.MILLISECONDS);
      return;
    }
    if (phase == LEAF) {
      phase = DONE;
      T.check("fill instance survived the leaf launch", instances == 1 && restarts == 1);
      T.check(
          "home instance survived the leaf launch", ActHome.instances == 1 && !ActHome.destroyed);
      T.check(
          "no second trim after the cache went",
          T.snapshot().indexOf("fill:") == T.snapshot().lastIndexOf("fill:"));
      finish();
    }
  }

  @Override
  public void onRestart() {
    super.onRestart();
    restarts++;
  }

  @Override
  public void onDestroy() {
    super.onDestroy();
    if (task != null) {
      task.cancel(false);
      task = null;
    }
  }

  private void step() {
    if (phase != FILL) {
      return;
    }
    if (viewsAtTrim >= 0) {
      phase = VERIFY;
      verify();
      return;
    }
    for (int i = 0; i < STEP; i++) {
      TextView t = new TextView(this);
      t.setText("cache " + views);
      cache.addView(t);
      views++;
    }
    if (views >= MAX_VIEWS) {
      T.check("pool trim arrived before " + MAX_VIEWS + " views", false);
      phase = DONE;
      finish();
    }
  }

  private void verify() {
    String trace = T.snapshot();
    T.check(
        "delivery order app, extra, home, fill, svc",
        "app:15 extra:15 home:15 fill:15 svc:15 ".equals(trace));
    T.check("no onLowMemory", !trace.contains("low"));
    T.check("home not reclaimed before the trim", ActHome.instances == 1 && !ActHome.destroyed);
    T.check("the cache had grown first", viewsAtTrim >= STEP * 4);
    phase = LEAF;
    startActivity(new Intent(ActLeaf.class));
  }

  @Override
  public void onTrimMemory(int level) {
    T.log("fill:" + level);
    if (viewsAtTrim < 0) {
      viewsAtTrim = views;
      cache.removeAllViews();
      views = 0;
    }
  }

  @Override
  public void onLowMemory() {
    T.log("fill:low");
  }
}
