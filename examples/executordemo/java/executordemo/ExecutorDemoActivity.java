// SPDX-License-Identifier: GPL-3.0-only
package executordemo;

import picodroid.app.Activity;
import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Exercises the unified main-thread FIFO and the background thread pool.
 *
 * <p>The Runnables posted in {@code onCreate} sit in the Rust-side main queue until the framework
 * event loop begins draining it, then fire in strict FIFO order between the LVGL ticks. Tokens
 * {@code MAIN-1..3} and {@code BG-1} must all appear in stdout under both shrink modes.
 */
public class ExecutorDemoActivity extends Activity {
  @Override
  public void onCreate() {
    getDisplay();

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);

    TextView tv = new TextView();
    tv.setText("ExecutorDemo");
    root.addView(tv);

    setContentView(root);

    Executor main = Executors.mainExecutor();
    Executor bg = Executors.backgroundExecutor();

    main.execute(() -> Log.i("EXEC", "MAIN-1"));
    main.execute(() -> Log.i("EXEC", "MAIN-2"));
    bg.execute(() -> Log.i("EXEC", "BG-1"));
    main.execute(() -> Log.i("EXEC", "MAIN-3"));

    Log.i("EXEC", "SETUP_DONE");
  }
}
