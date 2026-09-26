// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import picodroid.concurrent.Executors;

/**
 * Tells an app what network the board has and when it comes and goes, mirroring {@code
 * android.net.ConnectivityManager}: obtain it with {@code
 * getSystemService(Context.CONNECTIVITY_SERVICE)}.
 *
 * <pre>{@code
 * ConnectivityManager cm = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
 * cm.registerDefaultNetworkCallback(new ConnectivityManager.NetworkCallback() {
 *   @Override public void onAvailable(Network network) { fetch(); }
 *   @Override public void onLost(Network network) { showOffline(); }
 * });
 * }</pre>
 *
 * <p>picodroid has one link per board — WiFi or Ethernet, a build-time fact that {@link
 * NetworkInfo#getType()} names — so there is one {@link Network} at a time: the link, once it is up
 * with an address. A registered callback hears {@code onAvailable} (then {@code
 * onCapabilitiesChanged}) when the link comes up and {@code onLost} when it drops. As on Android, a
 * callback registered while the link is already up hears {@code onAvailable} shortly afterwards,
 * never from inside {@code register}; a callback registered while it is down hears nothing until it
 * comes up. Callbacks run on the main thread, between frames, like every other framework callback,
 * and the framework's event loop delivers them, so an app with no Activity does not receive them
 * (the same rule as a posted {@code Runnable}). Register from any thread.
 *
 * <p>Not here: {@code LinkProperties} and {@code onLinkPropertiesChanged} — read the address with
 * {@link NetworkInfo#getIpAddress()}; a DHCP renewal that changes it keeps the same {@link Network}
 * and fires nothing. {@code onLosing} and {@code onUnavailable} are declared so overrides compile,
 * but nothing here loses a network gracefully or times a request out. The deprecated {@code
 * getActiveNetworkInfo()} has no instance to return: ask {@link NetworkInfo}'s static methods
 * directly. A link that drops and returns within one frame (16 ms) is not reported.
 */
public class ConnectivityManager {
  /** No network on this board ({@link NetworkInfo#getType()} on a board without one). */
  public static final int TYPE_NONE = -1;

  /** A WiFi link. */
  public static final int TYPE_WIFI = 1;

  /** A wired Ethernet link. */
  public static final int TYPE_ETHERNET = 9;

  /** The most callbacks an app may hold registered at once; Android allows 100. */
  private static final int MAX_CALLBACKS = 8;

  /**
   * Base class for callbacks about the network's arrival and loss, mirroring {@code
   * android.net.ConnectivityManager.NetworkCallback}. Override what you need; every method here
   * does nothing. Instances are registered with {@link #registerDefaultNetworkCallback} or {@link
   * #registerNetworkCallback} and unregistered with {@link #unregisterNetworkCallback}.
   */
  public static class NetworkCallback {
    /** The request this callback is registered under; null while unregistered. */
    NetworkRequest request;

    /** The network this callback was last told is available; null after {@code onLost}. */
    Network current;

    public NetworkCallback() {}

    /**
     * The link is up with an address and satisfies this callback's request. Followed by {@link
     * #onCapabilitiesChanged}.
     */
    public void onAvailable(Network network) {}

    /** Never called: picodroid has one link and nothing to hand over to. */
    public void onLosing(Network network, int maxMsToLive) {}

    /** The link dropped. {@code network} is the one {@link #onAvailable} announced. */
    public void onLost(Network network) {}

    /** Never called: {@link #requestNetwork} has no timeout form. */
    public void onUnavailable() {}

    /**
     * The network's capabilities, once after {@link #onAvailable}. They do not change while the
     * link is up.
     */
    public void onCapabilitiesChanged(Network network, NetworkCapabilities networkCapabilities) {}
  }

  private static final ConnectivityManager INSTANCE = new ConnectivityManager();

  /** What {@link #registerDefaultNetworkCallback} asks for: Android's default request. */
  private static final NetworkRequest DEFAULT_REQUEST = new NetworkRequest.Builder().build();

  private final NetworkCallback[] callbacks = new NetworkCallback[MAX_CALLBACKS];
  private int count;

  /** The link while it is up with an address; null while down. Guarded by {@code this}. */
  private Network active;

  private NetworkCapabilities activeCapabilities;

  /** Android numbers networks from 100; each time the link comes back it is a new one. */
  private int nextNetId = 100;

  private ConnectivityManager() {}

  public static ConnectivityManager getInstance() {
    return INSTANCE;
  }

  /** The network the board is on, or null while the link is down or has no address yet. */
  public Network getActiveNetwork() {
    synchronized (this) {
      syncActive();
      return active;
    }
  }

  /**
   * What {@code network} can do, or null if it is not the network the board is on now (a {@link
   * Network} from before the link last dropped, or null).
   */
  public NetworkCapabilities getNetworkCapabilities(Network network) {
    synchronized (this) {
      syncActive();
      return network != null && network.equals(active) ? activeCapabilities : null;
    }
  }

  /**
   * Hear about the board's network, whatever kind it is: {@code onAvailable} when the link is up
   * with an address (shortly after this call, if it already is), {@code onLost} when it drops.
   *
   * @throws IllegalArgumentException if {@code networkCallback} is null or already registered
   * @throws IllegalStateException if eight callbacks are registered already
   */
  public void registerDefaultNetworkCallback(NetworkCallback networkCallback) {
    registerNetworkCallback(DEFAULT_REQUEST, networkCallback);
  }

  /**
   * Hear about networks that satisfy {@code request}: every capability it asks for, and one of its
   * transport types when it names any. A request for {@link NetworkCapabilities#TRANSPORT_CELLULAR}
   * on a WiFi board is never satisfied, so its callback never fires.
   *
   * @throws IllegalArgumentException if either argument is null or the callback is already
   *     registered
   * @throws IllegalStateException if eight callbacks are registered already
   */
  public void registerNetworkCallback(NetworkRequest request, NetworkCallback networkCallback) {
    if (request == null || networkCallback == null) {
      throw new IllegalArgumentException("null NetworkRequest or NetworkCallback");
    }
    final Network network;
    final NetworkCapabilities capabilities;
    synchronized (this) {
      if (networkCallback.request != null) {
        throw new IllegalArgumentException("NetworkCallback was already registered");
      }
      if (count == MAX_CALLBACKS) {
        throw new IllegalStateException(
            "too many NetworkCallbacks registered (" + MAX_CALLBACKS + ")");
      }
      networkCallback.request = request;
      networkCallback.current = null;
      callbacks[count++] = networkCallback;
      syncActive();
      network = active;
      capabilities = activeCapabilities;
    }
    if (network != null && request.matches(capabilities)) {
      // Android never calls back from inside register: the app hears onAvailable once its own
      // frame is over, so an Activity that registers in onCreate has its views by then.
      Executors.mainExecutor()
          .execute(() -> deliverInitial(networkCallback, network, capabilities));
    }
  }

  /**
   * Android's "bring up a network for this request": there is nothing to bring up, so this is
   * {@link #registerNetworkCallback}. {@code onUnavailable} is never called.
   */
  public void requestNetwork(NetworkRequest request, NetworkCallback networkCallback) {
    registerNetworkCallback(request, networkCallback);
  }

  /**
   * Stop delivering to {@code networkCallback}. Nothing is delivered after this returns, and a
   * callback may unregister itself from inside one of its own methods.
   *
   * @throws IllegalArgumentException if {@code networkCallback} is null or not registered
   */
  public void unregisterNetworkCallback(NetworkCallback networkCallback) {
    if (networkCallback == null) {
      throw new IllegalArgumentException("null NetworkCallback");
    }
    synchronized (this) {
      int at = -1;
      for (int i = 0; i < count; i++) {
        if (callbacks[i] == networkCallback) {
          at = i;
          break;
        }
      }
      if (at < 0) {
        throw new IllegalArgumentException("NetworkCallback was not registered");
      }
      count--;
      for (int i = at; i < count; i++) {
        callbacks[i] = callbacks[i + 1];
      }
      callbacks[count] = null;
      networkCallback.request = null;
      networkCallback.current = null;
    }
  }

  /**
   * Adopt a link that is already up: the framework only reports changes, so the first look at a
   * link that came up before anyone asked happens here. Never drops the network — that is {@link
   * #fireLinkChange}'s, so every callback hears {@code onLost}. Caller holds {@code this}.
   */
  private void syncActive() {
    if (active == null && NetworkInfo.isConnected()) {
      active = new Network(nextNetId++);
      activeCapabilities = NetworkCapabilities.ofLink();
    }
  }

  /** The posted half of {@link #registerNetworkCallback}: main thread. */
  private void deliverInitial(
      NetworkCallback callback, Network network, NetworkCapabilities capabilities) {
    // Skipped if the callback was unregistered, already told by a link change that ran first, or
    // the link dropped meanwhile (then there is nothing to announce and nothing was lost).
    synchronized (this) {
      if (callback.request == null || callback.current != null || !network.equals(active)) {
        return;
      }
    }
    callback.current = network;
    callback.onAvailable(network);
    if (callback.request != null) { // not unregistered from inside onAvailable
      callback.onCapabilitiesChanged(network, capabilities);
    }
  }

  /**
   * The framework calls this on the main thread when the link comes up with an address or drops
   * (the event loop's connectivity dispatch; a change within one frame is folded into the state
   * seen at the frame). {@code up} is the state now, not an edge: a repeat changes nothing.
   */
  static void fireLinkChange(boolean up) {
    INSTANCE.linkChanged(up);
  }

  private void linkChanged(boolean up) {
    final Network network;
    final NetworkCapabilities capabilities;
    final NetworkCallback[] snapshot;
    synchronized (this) {
      if (up) {
        syncActive();
        if (active == null) {
          // Up without an address yet: nothing an app can use.
          return;
        }
        network = active;
        capabilities = activeCapabilities;
      } else {
        if (active == null) {
          return;
        }
        network = active;
        capabilities = null;
        active = null;
        activeCapabilities = null;
      }
      if (count == 0) {
        return;
      }
      // A callback may unregister itself, or another, from inside onLost: deliver to a copy.
      snapshot = new NetworkCallback[count];
      System.arraycopy(callbacks, 0, snapshot, 0, count);
    }
    for (NetworkCallback callback : snapshot) {
      if (callback.request == null) {
        continue; // unregistered by an earlier callback in this round
      }
      if (up) {
        if (callback.current == null && callback.request.matches(capabilities)) {
          callback.current = network;
          callback.onAvailable(network);
          if (callback.request != null) { // not unregistered from inside onAvailable
            callback.onCapabilitiesChanged(network, capabilities);
          }
        }
      } else if (callback.current != null) {
        Network lost = callback.current;
        callback.current = null;
        callback.onLost(lost);
      }
    }
  }
}
