// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

import java.util.TimeZone;
import picodroid.util.Log;

/**
 * Base class for code that receives broadcast Intents, mirroring {@code
 * android.content.BroadcastReceiver}. Register an instance with {@link
 * Context#registerReceiver(BroadcastReceiver, IntentFilter)} and it hears every broadcast whose
 * action its filter names, on the main thread, between frames, like every other framework callback;
 * unregister it with {@link Context#unregisterReceiver}.
 *
 * <pre>{@code
 * private final BroadcastReceiver tick = new BroadcastReceiver() {
 *   @Override public void onReceive(Context context, Intent intent) {
 *     if (Intent.ACTION_TIME_TICK.equals(intent.getAction())) { showTime(); }
 *   }
 * };
 *
 * @Override protected void onResume() {
 *   super.onResume();
 *   IntentFilter filter = new IntentFilter(Intent.ACTION_TIME_TICK);
 *   filter.addAction(Intent.ACTION_TIME_CHANGED);
 *   filter.addAction(Intent.ACTION_TIMEZONE_CHANGED);
 *   registerReceiver(tick, filter);
 * }
 *
 * @Override protected void onPause() {
 *   unregisterReceiver(tick);
 *   super.onPause();
 * }
 * }</pre>
 *
 * <p>What is broadcast here: the system actions the framework raises ({@link
 * Intent#ACTION_TIME_TICK} at every minute, {@link Intent#ACTION_TIME_CHANGED}, {@link
 * Intent#ACTION_TIMEZONE_CHANGED}, {@link Intent#ACTION_SCREEN_ON} / {@link
 * Intent#ACTION_SCREEN_OFF}, the Wi-Fi and connectivity actions on a network board), an app's own
 * {@link Context#sendBroadcast}, and an alarm set with {@code PendingIntent.getBroadcast}. A
 * broadcast never leaves the app: there is one app running, so {@code sendBroadcast} reaches only
 * this app's receivers, and {@code RECEIVER_EXPORTED} changes nothing.
 *
 * <p>Not here: ordered and sticky broadcasts ({@code sendOrderedBroadcast}, {@code abortBroadcast},
 * {@code setResult*}, {@code getResultCode}, {@code isInitialStickyBroadcast}), {@code goAsync} (do
 * blocking work on a {@link picodroid.concurrent.Thread}), {@code peekService}, and
 * manifest-declared receivers other than {@code BOOT_COMPLETED}.
 *
 * <p>Limits: at most {@link #MAX_RECEIVERS} receivers registered at once per app, and a filter of
 * at most {@link IntentFilter#MAX_ACTIONS} actions. Registering the same receiver twice throws, as
 * on Android. An Activity that is destroyed with a receiver still registered has it unregistered
 * for it, with the warning Android logs ("has leaked IntentReceiver").
 */
public abstract class BroadcastReceiver {
  private static final String TAG = "BroadcastReceiver";

  /** The most receivers an app may hold registered at once. */
  public static final int MAX_RECEIVERS = 8;

  // The system actions as bits: what the framework hands `fireSystem`, and what
  // `nativeSetInterest` tells it to watch for. The order is the delivery order within one
  // frame — a clock set before the tick that follows it. Mirrored in
  // crates/picodroid-core/src/broadcasts.rs.
  static final int BIT_TIME_CHANGED = 1;
  static final int BIT_TIMEZONE_CHANGED = 1 << 1;
  static final int BIT_TIME_TICK = 1 << 2;
  static final int BIT_SCREEN_OFF = 1 << 3;
  static final int BIT_SCREEN_ON = 1 << 4;
  static final int BIT_BOOT_COMPLETED = 1 << 5;
  static final int BIT_SUPPLICANT_STATE = 1 << 6;
  static final int BIT_NETWORK_STATE = 1 << 7;
  static final int BIT_SCAN_RESULTS = 1 << 8;
  static final int BIT_CONNECTIVITY = 1 << 9;

  /** Android's text, so a log reader recognises it. */
  private static final String LEAK =
      " has leaked IntentReceiver that was originally registered here. Are you missing a call to"
          + " unregisterReceiver()?";

  // The app's registry. Allocated on the first registration: an app that never registers a
  // receiver pays nothing. Guarded by `BroadcastReceiver.class`; the heap reset at an app switch
  // clears all of it, so nothing here outlives the app.
  private static BroadcastReceiver[] sReceivers;
  private static IntentFilter[] sFilters;
  private static Context[] sOwners;
  private static int sCount;

  /** The system bits some registered filter names; what the framework was last told. */
  private static int sInterest;

  public BroadcastReceiver() {}

  /**
   * A broadcast this receiver's filter matched. {@code context} is the Context the receiver was
   * registered through (the Activity, Service or Application), and {@code intent} carries the
   * action and its extras. Runs on the main thread: return quickly.
   */
  public abstract void onReceive(Context context, Intent intent);

  // ── The registry (Context's registerReceiver / unregisterReceiver / sendBroadcast) ─────────

  /** {@link Context#registerReceiver}: null until there are sticky broadcasts, which is never. */
  static Intent register(Context owner, BroadcastReceiver receiver, IntentFilter filter) {
    if (filter == null) {
      throw new IllegalArgumentException("null IntentFilter");
    }
    if (receiver == null) {
      // Android's "query the sticky value" form: nothing is sticky here.
      return null;
    }
    int interest;
    synchronized (BroadcastReceiver.class) {
      if (indexOf(receiver) >= 0) {
        throw new IllegalArgumentException("Receiver is already registered");
      }
      if (sReceivers == null) {
        sReceivers = new BroadcastReceiver[MAX_RECEIVERS];
        sFilters = new IntentFilter[MAX_RECEIVERS];
        sOwners = new Context[MAX_RECEIVERS];
      }
      if (sCount == MAX_RECEIVERS) {
        throw new IllegalStateException(
            "too many BroadcastReceivers registered (" + MAX_RECEIVERS + ")");
      }
      sReceivers[sCount] = receiver;
      sFilters[sCount] = filter;
      sOwners[sCount] = owner;
      sCount++;
      Context.sReceiversRegistered = true;
      interest = interestLocked();
    }
    publishInterest(interest);
    return null;
  }

  /** {@link Context#unregisterReceiver}. */
  static void unregister(BroadcastReceiver receiver) {
    if (receiver == null) {
      throw new IllegalArgumentException("null BroadcastReceiver");
    }
    int interest;
    synchronized (BroadcastReceiver.class) {
      int at = indexOf(receiver);
      if (at < 0) {
        throw new IllegalArgumentException("Receiver not registered: " + receiver);
      }
      removeLocked(at);
      interest = interestLocked();
    }
    publishInterest(interest);
  }

  /**
   * The framework's half of an Activity's destroy: every receiver it registered and did not
   * unregister is dropped, with Android's warning. {@code Activity.performDestroy} calls it after
   * {@code onDestroy} returned, where an app's own unregister belongs.
   */
  static void unregisterOwned(Context owner) {
    int interest;
    synchronized (BroadcastReceiver.class) {
      if (sCount == 0) {
        return;
      }
      int i = 0;
      while (i < sCount) {
        if (sOwners[i] == owner) {
          Log.w(TAG, owner.getClass().getName() + LEAK + " (" + sReceivers[i] + ")");
          removeLocked(i);
        } else {
          i++;
        }
      }
      interest = interestLocked();
    }
    publishInterest(interest);
  }

  /** {@link Context#sendBroadcast}'s posted half: main thread. */
  static void send(Context context, Intent intent) {
    if (intent.getTargetClassName() != null) {
      deliverExplicit(context, intent);
    } else {
      dispatch(intent);
    }
  }

  /**
   * An Intent with a component: Android starts the named manifest receiver. Here the class is
   * instantiated through its public no-argument constructor and heard once.
   */
  static void deliverExplicit(Context context, Intent intent) {
    String name = intent.getTargetClassName();
    Object receiver;
    try {
      receiver = Class.forName(name).newInstance();
    } catch (ClassNotFoundException | InstantiationException | IllegalAccessException e) {
      Log.e(TAG, "cannot instantiate receiver " + name + ": " + e);
      return;
    }
    if (!(receiver instanceof BroadcastReceiver)) {
      Log.e(TAG, name + " is not a BroadcastReceiver");
      return;
    }
    deliver((BroadcastReceiver) receiver, context, intent);
  }

  /**
   * Deliver {@code intent} to every registered receiver whose filter names its action. A receiver
   * unregistered by an earlier one in the same round is skipped, as Android's dispatcher skips a
   * forgotten receiver; one that throws is logged and the round goes on.
   */
  static void dispatch(Intent intent) {
    String action = intent.getAction();
    BroadcastReceiver[] receivers;
    IntentFilter[] filters;
    Context[] owners;
    int n;
    synchronized (BroadcastReceiver.class) {
      n = sCount;
      if (n == 0) {
        return;
      }
      receivers = new BroadcastReceiver[n];
      filters = new IntentFilter[n];
      owners = new Context[n];
      System.arraycopy(sReceivers, 0, receivers, 0, n);
      System.arraycopy(sFilters, 0, filters, 0, n);
      System.arraycopy(sOwners, 0, owners, 0, n);
    }
    for (int i = 0; i < n; i++) {
      if (!filters[i].matchAction(action)) {
        continue;
      }
      synchronized (BroadcastReceiver.class) {
        if (indexOf(receivers[i]) < 0) {
          continue;
        }
      }
      deliver(receivers[i], owners[i], intent);
    }
  }

  private static void deliver(BroadcastReceiver receiver, Context context, Intent intent) {
    try {
      receiver.onReceive(context, intent);
    } catch (RuntimeException e) {
      // Android would crash the app. picodroid says what was thrown and carries on, as it does
      // for a posted Runnable.
      Log.e(TAG, "onReceive threw: " + e);
    }
  }

  // ── The system actions (the framework's event loop) ────────────────────────────────────────

  /**
   * The framework calls this on the main thread with the system actions that happened since the
   * last frame, as bits; several in one frame arrive as one call, in bit order. Nothing is called
   * unless some registered filter names one of them ({@link #nativeSetInterest}).
   */
  static void fireSystem(int mask) {
    for (int bit = 1; bit != 0 && bit <= mask; bit <<= 1) {
      if ((mask & bit) == 0) {
        continue;
      }
      String action = actionOf(bit);
      if (action == null) {
        continue;
      }
      Intent intent = new Intent(action);
      if (bit == BIT_TIMEZONE_CHANGED) {
        intent.putExtra(Intent.EXTRA_TIMEZONE, TimeZone.getDefault().getID());
      }
      dispatch(intent);
    }
  }

  /** The system action a bit stands for; null for a bit no action has. */
  static String actionOf(int bit) {
    switch (bit) {
      case BIT_TIME_CHANGED:
        return Intent.ACTION_TIME_CHANGED;
      case BIT_TIMEZONE_CHANGED:
        return Intent.ACTION_TIMEZONE_CHANGED;
      case BIT_TIME_TICK:
        return Intent.ACTION_TIME_TICK;
      case BIT_SCREEN_OFF:
        return Intent.ACTION_SCREEN_OFF;
      case BIT_SCREEN_ON:
        return Intent.ACTION_SCREEN_ON;
      case BIT_BOOT_COMPLETED:
        return Intent.ACTION_BOOT_COMPLETED;
      default:
        return null;
    }
  }

  /** The bit a system action is, or 0 for an app's own action. */
  static int bitOf(String action) {
    if (Intent.ACTION_TIME_TICK.equals(action)) {
      return BIT_TIME_TICK;
    }
    if (Intent.ACTION_TIME_CHANGED.equals(action)) {
      return BIT_TIME_CHANGED;
    }
    if (Intent.ACTION_TIMEZONE_CHANGED.equals(action)) {
      return BIT_TIMEZONE_CHANGED;
    }
    if (Intent.ACTION_SCREEN_OFF.equals(action)) {
      return BIT_SCREEN_OFF;
    }
    if (Intent.ACTION_SCREEN_ON.equals(action)) {
      return BIT_SCREEN_ON;
    }
    if (Intent.ACTION_BOOT_COMPLETED.equals(action)) {
      return BIT_BOOT_COMPLETED;
    }
    return 0;
  }

  // ── Helpers; the caller holds the class monitor ────────────────────────────────────────────

  private static int indexOf(BroadcastReceiver receiver) {
    for (int i = 0; i < sCount; i++) {
      if (sReceivers[i] == receiver) {
        return i;
      }
    }
    return -1;
  }

  private static void removeLocked(int at) {
    sCount--;
    for (int i = at; i < sCount; i++) {
      sReceivers[i] = sReceivers[i + 1];
      sFilters[i] = sFilters[i + 1];
      sOwners[i] = sOwners[i + 1];
    }
    sReceivers[sCount] = null;
    sFilters[sCount] = null;
    sOwners[sCount] = null;
  }

  /** The OR of every registered filter's system bits. */
  private static int interestLocked() {
    int mask = 0;
    for (int i = 0; i < sCount; i++) {
      mask |= sFilters[i].systemBits();
    }
    return mask;
  }

  /** Tell the framework which system actions to raise, when that set changed. */
  private static void publishInterest(int mask) {
    synchronized (BroadcastReceiver.class) {
      if (mask == sInterest) {
        return;
      }
      sInterest = mask;
    }
    nativeSetInterest(mask);
  }

  /** The system bits the event loop should watch for; 0 turns the watch off. */
  private static native void nativeSetInterest(int mask);
}
