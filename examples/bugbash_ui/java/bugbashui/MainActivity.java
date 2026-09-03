// SPDX-License-Identifier: GPL-3.0-only
package bugbashui;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

public class MainActivity extends Activity {
  private static final String TAG = "BugBashUi";
  private static final int BINDS = 17; // one past the framework's connection table

  static int passed = 0;
  static int failed = 0;
  static int resumeCount = 0;
  static int launchClicks = 0;

  private Button launchBtn;
  private final ServiceConnection[] conns = new ServiceConnection[BINDS];

  static void check(String name, boolean condition) {
    if (condition) {
      Log.i(TAG, "PASS: " + name);
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "Main.onCreate");
    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(240, 240);
    TextView title = new TextView();
    title.setText("bug bash");
    root.addView(title);
    launchBtn = new Button("launch");
    check("TextView.getText round-trip", "bug bash".equals(title.getText().toString()));
    check("Button.getText round-trip", "launch".equals(launchBtn.getText().toString()));
    launchBtn.setSize(200, 40);
    launchBtn.setOnClickListener(
        v -> {
          launchClicks++;
          Log.i(TAG, "launch click #" + launchClicks);
          startActivity(new Intent(DetailActivity.class));
        });
    root.addView(launchBtn);
    setContentView(root);
  }

  @Override
  public void onResume() {
    resumeCount++;
    Log.i(TAG, "Main.onResume #" + resumeCount);
    if (resumeCount == 1) {
      // Phase 1 — two clicks land in one dispatch tick. Android delivers
      // both to the departing Activity (a double tap double-launches unless
      // the app guards it), so two Details are pushed; each calls finish()
      // twice (F1) and must pop only itself — Main resumes exactly once.
      launchBtn.performClick();
      launchBtn.performClick();
    } else if (resumeCount == 2) {
      check("double click launched twice (Android semantics)", DetailActivity.creates == 2);
      check("F1 double finish popped one Activity each", launchClicks == 2);
      // Phase 2 — F10: bind one connection past the table, then unbind every
      // one of them; the Service must reach onDestroy. Service ops are
      // deferred through the pending-op queue and drained once per main-loop
      // callback, so issue one op per posted Runnable rather than 34 from a
      // single callback (which would overflow the queue).
      bindStep(0);
    }
  }

  private void bindStep(int i) {
    if (i < BINDS) {
      conns[i] =
          new ServiceConnection() {
            @Override
            public void onServiceConnected(IBinder binder) {}

            @Override
            public void onServiceDisconnected() {}
          };
      bindService(new Intent(ProbeService.class), conns[i]);
      final int next = i + 1;
      Executors.mainExecutor().execute(() -> bindStep(next));
    } else {
      unbindStep(0);
    }
  }

  private void unbindStep(int i) {
    if (i < BINDS) {
      unbindService(conns[i]);
      final int next = i + 1;
      Executors.mainExecutor().execute(() -> unbindStep(next));
    } else {
      Executors.mainExecutor().execute(this::finishUp);
    }
  }

  private void finishUp() {
    check("F10 service destroyed after unbinding every recorded bind", ProbeService.destroyed);
    Log.i(TAG, "passed=" + passed + " failed=" + failed);
    if (failed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " ===");
    }
    finish();
  }
}
