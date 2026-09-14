// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.os.IBinder;

/** Binds the service, writes a preference, then finishes without unbinding. */
public class ActC extends Activity {
  static IBinder binderSeen = null;
  static int disconnects = 0;
  static int pingFromC = -1;

  private final ServiceConnection conn =
      new ServiceConnection() {
        @Override
        public void onServiceConnected(IBinder binder) {
          T.log("C.connected");
          binderSeen = binder;
          Svc s = ((Svc.LocalBinder) binder).service;
          pingFromC = s.ping();
        }

        @Override
        public void onServiceDisconnected() {
          disconnects++;
          T.log("C.disconnected");
        }
      };

  @Override
  public void onCreate() {
    T.log("C.onCreate");
    Intent in = getIntent();
    T.check("C intent present and empty", in != null && in.extraCount() == 0 && !in.hasExtra("n"));
    T.check("C getStringExtra missing null", in == null || in.getStringExtra("nothing") == null);
    T.check("C package name", "qa_life".equals(getPackageName()));
    bindService(new Intent(Svc.class), conn);
    getSharedPreferences("life", Context.MODE_PRIVATE)
        .edit()
        .putInt("from_c", 1)
        .putString("who", "C")
        .commit();
    T.later(200, () -> finish());
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
