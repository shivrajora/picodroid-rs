// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;

/**
 * Starts itself until the Activity stack is full, then unwinds. The one on top when a push is
 * refused checks that it was left resumed: a refused push used to run the covered Activity's
 * onPause and never its onResume, so the screen the user saw was a paused Activity.
 *
 * <p>One watcher thread for the whole chain, not a timer per D: up to eight timers alive at once
 * ran an rp2040's heap out of room for task stacks, the top D's spawn threw, and nothing ever
 * unwound.
 */
public class ActD extends Activity {
  static int created = 0;
  static int resumes = 0;
  static int pauses = 0;
  static boolean checked = false;
  static ActD top;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    created++;
    top = this;
    T.log("D.onCreate");
    if (created > 40) {
      Log.i(T.TAG, "FAIL: stack never filled");
      finish();
      return;
    }
    // Refused or not, the launch is asynchronous; a frame later the next D exists or this one
    // is the top for good.
    startActivity(new Intent(ActD.class));
    if (created == 1) {
      watchUntilSettled();
    }
  }

  /** Waits until no new D has appeared for 150 ms, then hands the top one to settled(). */
  private static void watchUntilSettled() {
    new Thread(
            () -> {
              int seen;
              do {
                seen = created;
                SystemClock.sleep(150);
              } while (created != seen);
              Executors.mainExecutor().execute(() -> top.settled());
            },
            "qa-life-d-watch")
        .start();
  }

  private void settled() {
    if (checked) {
      return;
    }
    checked = true;
    Log.i(T.TAG, "stack filled at " + created + " D activities");
    T.check("refused push leaves the top resumed", resumes - pauses == 1);
    T.check("each covered D paused once, the top never", pauses == created - 1);
    finish();
  }

  @Override
  public void onResume() {
    resumes++;
    T.log("D.onResume");
    if (checked) {
      finish(); // unwinding: the D above is gone, this one goes too
    }
  }

  @Override
  public void onPause() {
    pauses++;
    T.log("D.onPause");
  }
}
