// SPDX-License-Identifier: GPL-3.0-only
package qa_bcast;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.BroadcastReceiver;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.IntentFilter;
import picodroid.os.Bundle;
import picodroid.util.Log;

/**
 * The driver. Synchronous checks in onCreate (filters, the registry's rules), then a chain of
 * main-thread turns: each {@code sendBroadcast} is followed by a posted step, which runs after the
 * broadcast's own turn, so the chain needs no sleeps. The ActB excursion (a leaked receiver) runs
 * in the middle; ActA continues from its second onResume.
 */
public class ActA extends Activity {
  private static final String ACTION_A = "qa_bcast.A";
  private static final String ACTION_B = "qa_bcast.B";
  private static final String ACTION_ORDER = "qa_bcast.ORDER";
  private static final String ACTION_THROW = "qa_bcast.THROW";

  private int resumes = 0;
  private int aCount = 0;
  private String aLastAction = null;
  private int aLastExtra = -1;
  private boolean aContextIsThis = false;
  private boolean aOnMainThread = false;
  private int bCount = 0;
  private int orderFirst = 0;
  private int orderSecond = 0;
  private int afterThrow = 0;
  private int orderCalls = 0;

  private final BroadcastReceiver receiverA =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          aCount++;
          aLastAction = intent.getAction();
          aLastExtra = intent.getIntExtra("n", -1);
          aContextIsThis = context == ActA.this;
          aOnMainThread = Thread.currentThread() == getMainThread();
          T.log("A.onReceive");
        }
      };

  private final BroadcastReceiver receiverB =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          bCount++;
          T.log("B.onReceive");
        }
      };

  // Order and re-entrancy: `first` unregisters `second` from inside onReceive, so `second`,
  // registered after it, must not hear the same broadcast.
  private final BroadcastReceiver first =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          orderCalls++;
          orderFirst = orderCalls;
          unregisterReceiver(second);
        }
      };

  private final BroadcastReceiver second =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          orderCalls++;
          orderSecond = orderCalls;
        }
      };

  private final BroadcastReceiver thrower =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          throw new IllegalStateException("qa: a receiver that throws");
        }
      };

  private final BroadcastReceiver survivor =
      new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
          afterThrow++;
        }
      };

  private Thread mainThread;

  Thread getMainThread() {
    return mainThread;
  }

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    mainThread = Thread.currentThread();
    T.log("A.onCreate");
    checkIntentAndFilter();
    checkRegistryRules();
  }

  @Override
  public void onResume() {
    super.onResume();
    resumes++;
    T.log("A.onResume" + resumes);
    if (resumes == 1) {
      Executors.mainExecutor().execute(this::stepSendToRegistered);
    } else if (resumes == 2) {
      Executors.mainExecutor().execute(this::stepAfterLeak);
    }
  }

  private void checkIntentAndFilter() {
    Intent explicit = new Intent(ActB.class);
    T.check("explicit intent has no action", explicit.getAction() == null);
    Intent act = new Intent(ACTION_A);
    T.check("Intent(String) action", ACTION_A.equals(act.getAction()));
    T.check("Intent(String) has no class", act.getTargetClassName() == null);
    T.check("setAction chains", ACTION_B.equals(act.setAction(ACTION_B).getAction()));
    T.check(
        "ACTION_TIME_TICK value",
        "android.intent.action.TIME_TICK".equals(Intent.ACTION_TIME_TICK));
    T.check(
        "ACTION_TIME_CHANGED value",
        "android.intent.action.TIME_SET".equals(Intent.ACTION_TIME_CHANGED));

    IntentFilter f = new IntentFilter(ACTION_A);
    T.check("filter count 1", f.countActions() == 1);
    f.addAction(ACTION_B);
    f.addAction(ACTION_B);
    T.check("duplicate addAction ignored", f.countActions() == 2);
    T.check("getAction order", ACTION_A.equals(f.getAction(0)) && ACTION_B.equals(f.getAction(1)));
    T.check("hasAction", f.hasAction(ACTION_B) && !f.hasAction(ACTION_ORDER));
    T.check("matchAction null is false", !f.matchAction(null));
    IntentFilter copy = new IntentFilter(f);
    T.check("copy constructor", copy.countActions() == 2 && copy.hasAction(ACTION_A));
    f.addAction("x.3");
    f.addAction("x.4");
    boolean capped = false;
    try {
      f.addAction("x.5");
    } catch (IllegalStateException e) {
      capped = true;
    }
    T.check("filter caps at MAX_ACTIONS", capped && f.countActions() == IntentFilter.MAX_ACTIONS);
    boolean npe = false;
    try {
      new IntentFilter().addAction(null);
    } catch (NullPointerException e) {
      npe = true;
    }
    T.check("addAction(null) throws", npe);
  }

  private void checkRegistryRules() {
    boolean nullFilter = false;
    try {
      registerReceiver(receiverA, null);
    } catch (IllegalArgumentException e) {
      nullFilter = true;
    }
    T.check("register with null filter throws", nullFilter);
    T.check(
        "register null receiver returns null",
        registerReceiver(null, new IntentFilter(ACTION_A)) == null);

    boolean notRegistered = false;
    try {
      unregisterReceiver(receiverA);
    } catch (IllegalArgumentException e) {
      notRegistered = true;
    }
    T.check("unregister unknown throws", notRegistered);

    T.check(
        "register returns null (no sticky)",
        registerReceiver(receiverA, new IntentFilter(ACTION_A)) == null);
    boolean twice = false;
    try {
      registerReceiver(receiverA, new IntentFilter(ACTION_B));
    } catch (IllegalArgumentException e) {
      twice = true;
    }
    T.check("register twice throws", twice);

    // The cap: receiverA holds one slot; seven more fill the registry, the ninth is refused.
    BroadcastReceiver[] fillers = new BroadcastReceiver[BroadcastReceiver.MAX_RECEIVERS];
    int registered = 0;
    boolean full = false;
    for (int i = 0; i < fillers.length; i++) {
      fillers[i] =
          new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {}
          };
      try {
        registerReceiver(
            fillers[i], new IntentFilter("qa_bcast.FILL"), Context.RECEIVER_NOT_EXPORTED);
        registered++;
      } catch (IllegalStateException e) {
        full = true;
      }
    }
    T.check(
        "registry caps at MAX_RECEIVERS",
        full && registered == BroadcastReceiver.MAX_RECEIVERS - 1);
    for (int i = 0; i < registered; i++) {
      unregisterReceiver(fillers[i]);
    }
    boolean refusedPackage = false;
    try {
      sendBroadcast(new Intent(ACTION_A).setPackage("launcher"));
    } catch (IllegalArgumentException e) {
      refusedPackage = true;
    }
    T.check("sendBroadcast to a package is refused", refusedPackage);
  }

  private void stepSendToRegistered() {
    registerReceiver(receiverB, new IntentFilter(ACTION_B));
    sendBroadcast(new Intent(ACTION_A).putExtra("n", 7));
    T.check("sendBroadcast is asynchronous", aCount == 0);
    Executors.mainExecutor().execute(this::stepCheckRegistered);
  }

  private void stepCheckRegistered() {
    T.check("A received once", aCount == 1);
    T.check("A got the action", ACTION_A.equals(aLastAction));
    T.check("A got the extra", aLastExtra == 7);
    T.check("A's context is the registering Activity", aContextIsThis);
    T.check("onReceive on the main thread", aOnMainThread);
    T.check("B not matched by A's action", bCount == 0);
    // An Intent with neither action nor class reaches nobody and throws nothing.
    sendBroadcast(new Intent());
    // Order and the forgotten receiver.
    registerReceiver(first, new IntentFilter(ACTION_ORDER));
    registerReceiver(second, new IntentFilter(ACTION_ORDER));
    sendBroadcast(new Intent(ACTION_ORDER));
    Executors.mainExecutor().execute(this::stepCheckOrder);
  }

  private void stepCheckOrder() {
    T.check("first heard it first", orderFirst == 1);
    T.check("a receiver unregistered mid-round is skipped", orderSecond == 0 && orderCalls == 1);
    T.check("A untouched by other actions", aCount == 1);
    unregisterReceiver(first);
    // A receiver that throws does not starve the next one.
    registerReceiver(thrower, new IntentFilter(ACTION_THROW));
    registerReceiver(survivor, new IntentFilter(ACTION_THROW));
    sendBroadcast(new Intent(ACTION_THROW));
    Executors.mainExecutor().execute(this::stepCheckThrow);
  }

  private void stepCheckThrow() {
    T.check("receiver after a throwing one still runs", afterThrow == 1);
    unregisterReceiver(thrower);
    unregisterReceiver(survivor);
    // Explicit component: a fresh instance each time, the sender's Context.
    sendBroadcast(new Intent(this, ExplicitReceiver.class).putExtra("v", 42));
    Executors.mainExecutor().execute(this::stepCheckExplicit);
  }

  private void stepCheckExplicit() {
    T.check("explicit receiver instantiated and heard", ExplicitReceiver.received == 1);
    T.check("explicit receiver got the extra", ExplicitReceiver.lastValue == 42);
    T.check("explicit receiver got the sender's context", ExplicitReceiver.contextWasActivity);
    T.check("registered receivers not reached by an explicit send", aCount == 1 && bCount == 0);
    // A receiver unregistered then re-registered hears again.
    unregisterReceiver(receiverA);
    sendBroadcast(new Intent(ACTION_A));
    Executors.mainExecutor().execute(this::stepCheckUnregistered);
  }

  private void stepCheckUnregistered() {
    T.check("nothing after unregister", aCount == 1);
    registerReceiver(receiverA, new IntentFilter(ACTION_A));
    // The leak excursion: ActB registers and finishes without unregistering.
    startActivity(new Intent(ActB.class));
  }

  private void stepAfterLeak() {
    T.check("B destroyed", T.count("B.onDestroy") == 1);
    // The reaped receiver is no longer registered, so registering it again is not "already
    // registered"; a send meanwhile reaches nobody in ActB.
    boolean reRegistered = false;
    try {
      registerReceiver(ActB.leaked, new IntentFilter(ActB.ACTION));
      reRegistered = true;
    } catch (IllegalArgumentException e) {
      Log.i(T.TAG, "leaked receiver was still registered: " + e);
    }
    T.check("leaked receiver was reaped at destroy", reRegistered);
    unregisterReceiver(ActB.leaked);
    sendBroadcast(new Intent(ActB.ACTION));
    Executors.mainExecutor().execute(this::stepFinish);
  }

  private void stepFinish() {
    T.check(
        "dead Activity heard nothing",
        ActB.receivedAfterDestroy == 0 && T.count("Leaked.onReceive") == 0);
    unregisterReceiver(receiverA);
    unregisterReceiver(receiverB);
    T.report();
    finish();
  }
}
