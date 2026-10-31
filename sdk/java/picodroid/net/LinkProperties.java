// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.util.ArrayList;
import java.util.List;

/**
 * Mirrors {@code android.net.LinkProperties}, the address subset: what {@link
 * ConnectivityManager#getLinkProperties(Network)} says about the board's link, whatever kind it is
 * (WiFi, Ethernet, the simulator's host link). No routes, DNS servers, MTU or interface name.
 */
public final class LinkProperties {
  private final ArrayList<LinkAddress> addresses = new ArrayList<LinkAddress>();

  /** The link's address, packed as {@link InetAddress#getRawAddress} packs it; 0 for none. */
  LinkProperties(int rawAddress) {
    if (rawAddress != 0) {
      addresses.add(new LinkAddress(new InetAddress(rawAddress)));
    }
  }

  /** The addresses on the link: the one IPv4 address, or none. */
  public List<LinkAddress> getLinkAddresses() {
    return new ArrayList<LinkAddress>(addresses);
  }
}
