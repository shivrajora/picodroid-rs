// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Activity;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.content.SharedPreferences;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;

/**
 * The driver: verifies extras, runs the B (result) and C (service) excursions, then walks the
 * service through unbind / rebind / stop / restart one step per frame — every Context service call
 * is delivered on a later tick, as on Android — and reports.
 */
public class ActA extends Activity {
  private int resumes = 0;
  private int resultCode = -99;
  private int resultReq = -1;
  private int resultR = -1;
  private String resultRs = null;
  private boolean resultRb = false;
  private int resultCount = 0;
  private IBinder binderA = null;
  private int disconnectsA = 0;
  private int pingFromA = -1;
  private int step = 0;

  private final ServiceConnection conn =
      new ServiceConnection() {
        @Override
        public void onServiceConnected(IBinder binder) {
          T.log("A.connected");
          binderA = binder;
          pingFromA = ((Svc.LocalBinder) binder).service.ping();
        }

        @Override
        public void onServiceDisconnected() {
          disconnectsA++;
          T.log("A.disconnected");
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    T.log("A.onCreate");
    Intent in = getIntent();
    T.check("A intent non-null", in != null);
    if (in != null) {
      T.check("int extra", in.getIntExtra("n", 0) == 43);
      T.check("int MIN extra", in.getIntExtra("neg", 0) == Integer.MIN_VALUE);
      T.check("string extra", "hello".equals(in.getStringExtra("s")));
      T.check("empty string extra", "".equals(in.getStringExtra("empty")) && in.hasExtra("empty"));
      T.check(
          "boolean extras", in.getBooleanExtra("flag", false) && !in.getBooleanExtra("f2", true));
      String longv = in.getStringExtra("long");
      T.check("200-char extra", longv != null && longv.length() == 200 && longv.charAt(199) == 'R');
      T.check("odd key extra", "v".equals(in.getStringExtra("k.with-odd_chars/")));
      T.check("missing int default", in.getIntExtra("missing", 7) == 7);
      T.check("missing string null", in.getStringExtra("missing") == null);
      T.check("missing boolean default", in.getBooleanExtra("missing", true));
      T.check("wrong-type int default", in.getIntExtra("s", 5) == 5);
      T.check("wrong-type string null", in.getStringExtra("n") == null);
      Bundle extras = in.getExtras();
      T.check("extras count received", extras != null && extras.size() == 8);
      boolean enumOk = extras != null;
      int ints = 0;
      if (extras != null) {
        for (String k : extras.keySet()) {
          if (k == null || !in.hasExtra(k)) {
            enumOk = false;
          }
          if (extras.get(k) instanceof Integer) {
            ints++;
            if (extras.getInt(k) != in.getIntExtra(k, -1)) {
              enumOk = false;
            }
          }
        }
      }
      T.check("extra enumeration consistent", enumOk && ints == 2);
    }
    T.check("A package name", "qa_life".equals(getPackageName()));
    // Service calls are asynchronous: nothing below has run until the next tick.
    startService(new Intent(Svc.class).putExtra("cmd", 1));
    startService(new Intent(Svc.class).putExtra("cmd", 2));
    bindService(new Intent(Svc.class), conn);
    T.check("startService is asynchronous", Svc.creates == 0 && binderA == null);
    startActivityForResult(new Intent(ActB.class).putExtra("q", 9), 5);
  }

  @Override
  public void onStart() {
    T.log("A.onStart");
  }

  @Override
  public void onRestart() {
    T.log("A.onRestart");
  }

  @Override
  public void onPause() {
    T.log("A.onPause");
  }

  @Override
  public void onStop() {
    T.log("A.onStop");
  }

  @Override
  public void onDestroy() {
    T.log("A.onDestroy");
  }

  @Override
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    resultCount++;
    T.log("A.onActivityResult");
    this.resultReq = requestCode;
    this.resultCode = resultCode;
    if (data != null) {
      resultR = data.getIntExtra("r", -1);
      resultRs = data.getStringExtra("rs");
      resultRb = data.getBooleanExtra("rb", false);
    }
  }

  @Override
  public void onResume() {
    resumes++;
    T.log("A.onResume");
    if (resumes == 2) {
      afterB();
    } else if (resumes == 3) {
      afterC();
    }
  }

  private void afterB() {
    T.check("result delivered once", resultCount == 1);
    T.check("result code/request", resultCode == RESULT_OK && resultReq == 5);
    T.check("result extras", resultR == 7 && "res".equals(resultRs) && resultRb);
    T.check("B.onCreate after A.onPause", T.before("A.onPause", 1, "B.onCreate", 1));
    T.check("result after B.onCreate", T.before("B.onCreate", 1, "A.onActivityResult", 1));
    T.check("result before A.onResume#2", T.before("A.onActivityResult", 1, "A.onResume", 2));
    T.check("B destroyed", T.count("B.onDestroy") == 1);
    T.check("A.onStart#2 before A.onResume#2", T.before("A.onStart", 2, "A.onResume", 2));
    // Android skips onStart/onResume for an Activity that finishes inside onCreate; picodroid runs
    // the whole cycle. Recorded, not asserted.
    Log.i(
        T.TAG,
        "B lifecycle after finish() in onCreate: start="
            + T.count("B.onStart")
            + " resume="
            + T.count("B.onResume")
            + " pause="
            + T.count("B.onPause")
            + " stop="
            + T.count("B.onStop"));
    // The service ops queued in onCreate have run by now.
    T.check("service created once for two starts", Svc.creates == 1 && Svc.starts == 2);
    T.check("start commands carry their intents", Svc.lastCmd == 2);
    T.check("startIds increase", Svc.lastStartId == 2);
    T.check("flags zero", Svc.lastFlags == 0);
    T.check("service package name", "qa_life".equals(Svc.pkg));
    T.check("bind on a started service: one onBind", Svc.binds == 1);
    T.check(
        "A bound: connected with binder", binderA != null && binderA instanceof Svc.LocalBinder);
    T.check("A ping worked", pingFromA == 1);
    T.check("service alive", Svc.live != null && Svc.destroys == 0);
    T.check(
        "Svc.onCreate before onStartCommand#1",
        T.before("Svc.onCreate", 1, "Svc.onStartCommand#1", 1));
    T.check("onStartCommand#2 before onBind", T.before("Svc.onStartCommand#2", 1, "Svc.onBind", 1));
    T.check("onBind before A.connected", T.before("Svc.onBind", 1, "A.connected", 1));
    T.later(150, () -> startActivity(new Intent(ActC.class)));
  }

  private void afterC() {
    T.check("C.onCreate after A.onPause#2", T.before("A.onPause", 2, "C.onCreate", 1));
    T.check("C.onResume before A.onStop#2", T.before("C.onResume", 1, "A.onStop", 2));
    // picodroid's documented finish order: the leaving Activity is fully torn down before the one
    // below restarts (Android resumes the one below first, then stops the leaving one).
    T.check("C.onPause before C.onStop", T.before("C.onPause", 1, "C.onStop", 1));
    T.check("C.onStop before C.onDestroy", T.before("C.onStop", 1, "C.onDestroy", 1));
    T.check("C.onDestroy before A.onRestart#2", T.before("C.onDestroy", 1, "A.onRestart", 2));
    T.check("A.onRestart#2 before A.onStart#3", T.before("A.onRestart", 2, "A.onStart", 3));
    T.check("A.onStart#3 before A.onResume#3", T.before("A.onStart", 3, "A.onResume", 3));
    T.check("C connected, no second onBind", ActC.binderSeen != null && Svc.binds == 1);
    T.check("same binder for both clients", ActC.binderSeen == binderA);
    T.check("C ping continued the count", ActC.pingFromC == 2);
    T.check("C destroyed without unbind: disconnected", ActC.disconnects == 1);
    T.check(
        "service outlives C (A still bound)",
        Svc.live != null && Svc.destroys == 0 && Svc.unbinds == 0);
    int pingAfter = Svc.live == null ? -1 : Svc.live.ping();
    T.check("service usable after C", pingAfter == 3);
    SharedPreferences p = getSharedPreferences("life", Context.MODE_PRIVATE);
    T.check(
        "prefs written by C visible in A",
        p.getInt("from_c", 0) == 1 && "C".equals(p.getString("who", "")));
    p.edit().clear().commit();
    unbindService(conn);
    T.check("unbind is asynchronous", Svc.unbinds == 0);
    T.later(80, () -> serviceStep());
  }

  /** One service transition per frame; each step verifies the previous one landed. */
  private void serviceStep() {
    step++;
    switch (step) {
      case 1:
        T.check(
            "last unbind -> onUnbind, service kept (started)",
            Svc.unbinds == 1 && Svc.destroys == 0);
        Log.i(T.TAG, "explicit unbind called onServiceDisconnected: " + disconnectsA);
        unbindService(conn);
        break;
      case 2:
        T.check("second unbind does not re-unbind", Svc.unbinds == 1);
        bindService(new Intent(Svc.class), conn);
        break;
      case 3:
        T.check(
            "rebind after onUnbind true -> onRebind, no onBind",
            Svc.rebinds == 1 && Svc.binds == 1);
        T.check("reconnected", binderA != null && T.count("A.connected") == 2);
        unbindService(conn);
        break;
      case 4:
        T.check("unbind again", Svc.unbinds == 2);
        stopService(new Intent(Svc.class));
        break;
      case 5:
        T.check("stopService destroys", Svc.destroys == 1 && Svc.live == null);
        startService(new Intent(Svc.class).putExtra("cmd", 9));
        break;
      case 6:
        T.check("service recreated after stop", Svc.creates == 2 && Svc.lastCmd == 9);
        Log.i(T.TAG, "startId after recreate = " + Svc.lastStartId);
        stopService(new Intent(Svc.class));
        break;
      case 7:
        T.check("stopped again", Svc.destroys == 2);
        stopService(new Intent(Svc.class));
        break;
      case 8:
        T.check("stop when not running harmless", Svc.destroys == 2 && Svc.creates == 2);
        T.check("app created once", QaLifeApp.appCreates == 1);
        // More transitions than the per-frame queue holds: the overflow is an
        // IllegalStateException the app sees, never a silently dropped op.
        int accepted = 0;
        int refused = 0;
        for (int i = 0; i < 24; i++) {
          try {
            stopService(new Intent(Svc.class));
            accepted++;
          } catch (IllegalStateException e) {
            refused++;
          }
        }
        Log.i(T.TAG, "pending-op burst: accepted=" + accepted + " refused=" + refused);
        T.check("pending-op overflow throws IllegalStateException", refused > 0 && accepted > 0);
        T.later(80, () -> serviceStep());
        return;
      case 9:
        T.check("service stayed stopped through the burst", Svc.destroys == 2 && Svc.creates == 2);
        T.report();
        finish();
        return;
      default:
        return;
    }
    T.later(80, () -> serviceStep());
  }
}
