// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Activity;
import picodroid.content.ComponentName;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.widget.TextView;

/**
 * Binds the service, writes a preference, then finishes without unbinding. Leaves a task behind
 * that touches its TextView after the finish: the widget went with the screen, and the touch must
 * be an IllegalStateException the task can catch, not a null widget handed to the renderer.
 */
public class ActC extends Activity {
  static IBinder binderSeen = null;
  static int disconnects = 0;
  static int pingFromC = -1;
  static int staleTouches = 0;

  private final ServiceConnection conn =
      new ServiceConnection() {
        @Override
        public void onServiceConnected(ComponentName name, IBinder binder) {
          T.log("C.connected");
          binderSeen = binder;
          Svc s = ((Svc.LocalBinder) binder).service;
          pingFromC = s.ping();
        }

        @Override
        public void onServiceDisconnected(ComponentName name) {
          disconnects++;
          T.log("C.disconnected");
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    T.log("C.onCreate");
    Intent in = getIntent();
    T.check(
        "C intent present and empty", in != null && in.getExtras() == null && !in.hasExtra("n"));
    T.check("C getStringExtra missing null", in == null || in.getStringExtra("nothing") == null);
    T.check("C package name", "qa_life".equals(getPackageName()));
    bindService(new Intent(Svc.class), conn, BIND_AUTO_CREATE);
    getSharedPreferences("life", Context.MODE_PRIVATE)
        .edit()
        .putInt("from_c", 1)
        .putString("who", "C")
        .commit();
    TextView label = new TextView(this);
    label.setText("C");
    setContentView(label);
    T.later(200, () -> finish());
    // Fires after the finish, with A back on screen and C's widgets gone.
    T.later(700, () -> touchStaleView(label));
  }

  private static void touchStaleView(TextView label) {
    try {
      label.setText("late");
      T.check("a finished Activity's view refuses a late setText", false);
    } catch (IllegalStateException e) {
      staleTouches++;
      T.check("a finished Activity's view refuses a late setText", true);
    }
  }

  @Override
  public void onStart() {
    T.log("C.onStart");
  }

  @Override
  public void onResume() {
    T.log("C.onResume");
  }

  @Override
  public void onPause() {
    T.log("C.onPause");
  }

  @Override
  public void onStop() {
    T.log("C.onStop");
  }

  @Override
  public void onDestroy() {
    T.log("C.onDestroy");
  }
}
