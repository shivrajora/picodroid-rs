// SPDX-License-Identifier: GPL-3.0-only
package connectivity;

import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.content.Context;
import picodroid.net.ConnectivityManager;
import picodroid.net.Network;
import picodroid.net.NetworkCapabilities;
import picodroid.net.NetworkInfo;
import picodroid.net.NetworkRequest;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.widget.TextView;

/**
 * Android's way of hearing about the network, on picodroid: {@code ConnectivityManager} with a
 * {@code NetworkCallback}, instead of polling {@code NetworkInfo.isConnected()}.
 *
 * <p>Also the sim test for it (the {@code connectivity} row of {@code hil-tests.conf}): the app
 * registers, expects the link that is up at boot to be announced once its {@code onCreate} has
 * returned, then waits for the row's {@code test.ctrl} to take the simulated link down and up again
 * and checks {@code onLost} and a second {@code onAvailable} with a new {@code Network}.
 * Register-twice and unregister-unknown must throw as on Android; a request for a transport the
 * board lacks must never fire. Run by hand on a board with a network: {@code ./scripts/sim.sh --app
 * connectivity --board testbench_rp2350w}, then {@code net down} / {@code net up} on the control
 * channel ({@code scripts/sim-ctrl.sh}).
 */
public class Connectivity extends Activity {
  private static final String TAG = "Connectivity";

  /** If the link flap never comes, say so and leave rather than hang the row. */
  private static final int GIVE_UP_MS = 40_000;

  private ConnectivityManager cm;
  private TextView status;
  private int available;
  private Network lastNetwork;
  private boolean inRegister;
  private boolean done;
  private int failures;

  /** Registered against a transport this board does not have: must stay silent. */
  private final ConnectivityManager.NetworkCallback never =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          fail("cellular request satisfied on a board without cellular");
        }
      };

  private final ConnectivityManager.NetworkCallback callback =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          available++;
          Log.i(TAG, "onAvailable n=" + available + " net=" + network);
          check(!inRegister, "onAvailable ran from inside register");
          check(network != null, "onAvailable with a null Network");
          if (available == 1) {
            Network active = cm.getActiveNetwork();
            check(network.equals(active), "onAvailable network is not getActiveNetwork");
            check(NetworkInfo.isConnected(), "onAvailable while NetworkInfo says disconnected");
          } else {
            check(!network.equals(lastNetwork), "the returning link reused the lost Network");
            Log.i(TAG, "handle " + network.getNetworkHandle());
            finishRun();
          }
          lastNetwork = network;
          show("online " + network);
        }

        @Override
        public void onCapabilitiesChanged(Network network, NetworkCapabilities caps) {
          check(network.equals(lastNetwork), "capabilities for a network never announced");
          check(caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET), "no INTERNET");
          check(caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED), "no VALIDATED");
          boolean wifi = NetworkInfo.getType() == ConnectivityManager.TYPE_WIFI;
          check(
              caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == wifi,
              "transport disagrees with NetworkInfo.getType()");
          check(!caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR), "cellular transport");
          NetworkCapabilities again = cm.getNetworkCapabilities(network);
          check(
              again != null && again.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET),
              "getNetworkCapabilities disagrees");
          Log.i(TAG, "capabilities ok");
          if (available == 1) {
            Log.i(TAG, "ready for link change");
          }
        }

        @Override
        public void onLost(Network network) {
          Log.i(TAG, "onLost net=" + network);
          check(network.equals(lastNetwork), "onLost names a network never announced");
          check(cm.getActiveNetwork() == null, "getActiveNetwork after onLost");
          check(!NetworkInfo.isConnected(), "onLost while NetworkInfo says connected");
          check(cm.getNetworkCapabilities(network) == null, "capabilities of a lost network");
          show("offline");
          Log.i(TAG, "onLost ok");
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    status = new TextView(this);
    setContentView(status);
    show("starting");
    Log.i(TAG, "type=" + NetworkInfo.getType() + " connected=" + NetworkInfo.isConnected());

    cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
    check(cm != null, "getSystemService(CONNECTIVITY_SERVICE) is null");

    NetworkRequest cellular =
        new NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_CELLULAR)
            .build();
    check(cellular.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR), "request lost transport");
    check(
        cellular.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET),
        "request lost the default INTERNET capability");
    cm.registerNetworkCallback(cellular, never);

    try {
      new NetworkRequest.Builder().addTransportType(32);
      fail("transport 32 accepted");
    } catch (IllegalArgumentException e) {
      Log.i(TAG, "range check: " + e.getMessage());
    }
    try {
      cm.unregisterNetworkCallback(callback);
      fail("unregister of an unregistered callback did not throw");
    } catch (IllegalArgumentException e) {
      Log.i(TAG, "unregister unknown: " + e.getMessage());
    }

    inRegister = true;
    cm.registerDefaultNetworkCallback(callback);
    inRegister = false;
    try {
      cm.registerDefaultNetworkCallback(callback);
      fail("registering twice did not throw");
    } catch (IllegalArgumentException e) {
      Log.i(TAG, "register twice: " + e.getMessage());
    }

    Network active = cm.getActiveNetwork();
    Log.i(TAG, "registered active=" + active);
    if (active == null) {
      // A board whose link is still joining: the first onAvailable comes with the link.
      Log.i(TAG, "waiting for the link");
    }

    new Thread(
            () -> {
              SystemClock.sleep(GIVE_UP_MS);
              Executors.mainExecutor()
                  .execute(
                      () -> {
                        if (!done) {
                          fail("no link change within " + GIVE_UP_MS + " ms");
                          finishRun();
                        }
                      });
            },
            "give-up")
        .start();
  }

  private void finishRun() {
    if (done) {
      return;
    }
    done = true;
    cm.unregisterNetworkCallback(callback);
    cm.unregisterNetworkCallback(never);
    Log.i(TAG, failures == 0 ? "=== ALL PASSED ===" : "=== FAILED: " + failures + " ===");
    finish();
  }

  private void check(boolean ok, String what) {
    if (!ok) {
      fail(what);
    }
  }

  private void fail(String what) {
    failures++;
    Log.i(TAG, "FAIL: " + what);
  }

  private void show(String text) {
    status.setText("Connectivity: " + text);
  }
}
