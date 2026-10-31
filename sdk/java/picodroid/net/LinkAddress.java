// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * Mirrors {@code android.net.LinkAddress}, the address half: one IPv4 address the board holds on
 * its link. No prefix length, flags or scope: the stack does not report them.
 */
public final class LinkAddress {
  private final InetAddress address;

  LinkAddress(InetAddress address) {
    this.address = address;
  }

  /** The address. */
  public InetAddress getAddress() {
    return address;
  }

  @Override
  public boolean equals(Object o) {
    return o instanceof LinkAddress
        && ((LinkAddress) o).address.getRawAddress() == address.getRawAddress();
  }

  @Override
  public int hashCode() {
    return address.getRawAddress();
  }

  @Override
  public String toString() {
    return address.getHostAddress();
  }
}
