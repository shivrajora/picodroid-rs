// SPDX-License-Identifier: GPL-3.0-only
package qa_bcast;

import picodroid.app.Activity;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.IntentFilter;
import picodroid.os.Bundle;

/**
 * Registers a receiver and finishes without unregistering it: the framework must reap it at destroy
 * with Android's "has leaked IntentReceiver" warning, so that ActA can register the same instance
 * again and a later send does not reach the dead Activity.
 */
public class ActB extends Activity {
  static final String ACTION = "qa_bcast.LEAKED";
  static int receivedAfterDestroy = 0;

  static final BroadcastReceiver leaked =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          if (context instanceof ActB) {
            receivedAfterDestroy++;
          }
          T.log("Leaked.onReceive");
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    T.log("B.onCreate");
    registerReceiver(leaked, new IntentFilter(ACTION));
    finish();
  }

  @Override
  public void onDestroy() {
    T.log("B.onDestroy");
    super.onDestroy();
  }
}
