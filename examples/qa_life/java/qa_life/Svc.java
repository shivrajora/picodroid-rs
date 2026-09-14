// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Service;
import picodroid.content.Intent;
import picodroid.os.IBinder;

public class Svc extends Service {
  static int creates = 0;
  static int destroys = 0;
  static int binds = 0;
  static int unbinds = 0;
  static int rebinds = 0;
  static int starts = 0;
  static int lastStartId = -1;
  static int lastFlags = -1;
  static int lastCmd = -1;
  static Svc live = null;
  static String pkg = null;

  private final LocalBinder binder = new LocalBinder();
  private int pings = 0;

  public static class LocalBinder implements IBinder {
    public Svc service;
  }

  int ping() {
    return ++pings;
  }

  @Override
  public void onCreate() {
    creates++;
    live = this;
    binder.service = this;
    pkg = getPackageName();
    T.log("Svc.onCreate");
  }

  @Override
  public int onStartCommand(Intent intent, int flags, int startId) {
    starts++;
    lastStartId = startId;
    lastFlags = flags;
    lastCmd = intent == null ? -1 : intent.getIntExtra("cmd", -2);
    T.log("Svc.onStartCommand#" + startId);
    return START_STICKY;
  }

  @Override
  public IBinder onBind(Intent intent) {
    binds++;
    T.log("Svc.onBind");
    return binder;
  }

  @Override
  public boolean onUnbind(Intent intent) {
    unbinds++;
    T.log("Svc.onUnbind");
    return true;
  }

  @Override
  public void onRebind(Intent intent) {
    rebinds++;
    T.log("Svc.onRebind");
  }

  @Override
  public void onDestroy() {
    destroys++;
    live = null;
    T.log("Svc.onDestroy");
  }
}
