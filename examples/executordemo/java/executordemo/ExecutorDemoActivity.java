// SPDX-License-Identifier: GPL-3.0-only
package executordemo;

import picodroid.app.Activity;
import picodroid.concurrent.Executor;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.Thread;
import picodroid.concurrent.TimeUnit;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Exercises the unified main-thread FIFO, the background thread pool and the scheduled executor.
 *
 * <p>The Runnables posted in {@code onCreate} sit in the Rust-side main queue until the framework
 * event loop begins draining it, then fire in strict FIFO order between the LVGL ticks. Tokens
 * {@code MAIN-1..3} and {@code BG-1} must all appear in stdout under both shrink modes.
 *
 * <p>The scheduled part: a one-shot {@code SCHED-1} after 100 ms, a fixed-rate tick every 50 ms
 * that cancels itself on its third run ({@code RATE-3}), a one-shot cancelled before it is due
 * (never logs), and {@code SCHED-DONE} once the rate task has stopped and the executor reports
 * terminated. Each token is checked from the main thread, where the tasks run.
 */
public class ExecutorDemoActivity extends Activity {
  private final ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();
  private ScheduledFuture<?> rate;
  private int rateRuns;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
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

    scheduler.schedule(() -> Log.i("EXEC", "SCHED-1"), 100, TimeUnit.MILLISECONDS);
    ScheduledFuture<?> never =
        scheduler.schedule(() -> Log.i("EXEC", "SCHED-CANCELLED-RAN"), 150, TimeUnit.MILLISECONDS);
    boolean cancelled = never.cancel(false);
    rate = scheduler.scheduleAtFixedRate(this::onRate, 50, 50, TimeUnit.MILLISECONDS);
    scheduler.schedule(() -> finishScheduled(cancelled), 400, TimeUnit.MILLISECONDS);

    Log.i("EXEC", "SETUP_DONE");
  }

  private void onRate() {
    rateRuns++;
    if (rateRuns == 3) {
      Log.i("EXEC", "RATE-3 main=" + Thread.currentThread().getName());
      rate.cancel(false);
    }
  }

  private void finishScheduled(boolean cancelled) {
    scheduler.shutdown();
    // This runs inside the executor's last task, which counts as live until it returns (as on
    // the JDK), so the terminated check waits one main-queue turn.
    Executors.mainExecutor()
        .execute(
            () ->
                Log.i(
                    "EXEC",
                    "SCHED-DONE runs="
                        + rateRuns
                        + " cancelled="
                        + cancelled
                        + " rateCancelled="
                        + rate.isCancelled()
                        + " terminated="
                        + scheduler.isTerminated()));
  }
}
