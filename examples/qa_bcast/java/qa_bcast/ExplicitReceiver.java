// SPDX-License-Identifier: GPL-3.0-only
package qa_bcast;

import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;

/** A receiver reached by class, as a manifest receiver is on Android: a new instance per send. */
public class ExplicitReceiver extends BroadcastReceiver {
  static int received = 0;
  static int lastValue = -1;
  static boolean contextWasActivity = false;

  public ExplicitReceiver() {}

  @Override
  public void onReceive(Context context, Intent intent) {
    received++;
    lastValue = intent.getIntExtra("v", -1);
    contextWasActivity = context instanceof ActA;
    T.log("Explicit.onReceive");
  }
}
