// SPDX-License-Identifier: GPL-3.0-only
package bootdemo;

import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.util.Log;

/** The alarm's target: reached by an explicit Intent, as a manifest receiver is on Android. */
public class PingReceiver extends BroadcastReceiver {
  public PingReceiver() {}

  @Override
  public void onReceive(Context context, Intent intent) {
    Log.i(BootDemoApp.TAG, "ping n=" + intent.getIntExtra("n", -1));
    context.startActivity(new Intent(context, MainActivity.class).putExtra("pinged", true));
  }
}
