// SPDX-License-Identifier: GPL-3.0-only
package bootdemo;

import picodroid.app.Application;
import picodroid.content.Intent;
import picodroid.util.Log;

/**
 * A manifest receiver for {@code BOOT_COMPLETED} (docs/designs/broadcasts-2026-10.md stage 6), the
 * Android idiom for "re-arm after a reboot". At boot the framework runs this app headless — {@code
 * onCreate} here, then {@link BootReceiver} — before the boot app shows; the receiver arms a
 * broadcast alarm for {@link PingReceiver} a few seconds out, and when that fires the framework
 * starts the app again, this time with its Activity, to deliver it. The sim lane ({@code sim-run.sh
 * --app boot}) and a board with a launcher both show the whole circle in the log: {@code
 * [broadcast] manifest 0x20 -> bootdemo}, {@code BootDemo: boot completed}, the launcher's ready
 * line, {@code [alarm] wake bootdemo}, {@code BootDemo: ping}.
 */
public class BootDemoApp extends Application {
  static final String TAG = "BootDemo";

  @Override
  public void onCreate() {
    Log.i(TAG, "application up");
    // In the headless boot run this push is dropped (the framework says so in the log); when
    // the app is opened, or started for its alarm, it is the screen.
    startActivity(new Intent(MainActivity.class));
  }
}
